# Docker & Kubernetes Strategy: Single vs Separate Images

## Implemented: Option B (Separate Dockerfiles, Separate Images)

The project uses **Option B**:

| Asset | Approach |
|-------|----------|
| **Dockerfile.service** | Builds **only** the API binary (`backup-service`). Image contains a single executable. |
| **Dockerfile.worker** | Builds **only** the worker binary (`backup-worker`). Image contains a single executable. |
| **docker-compose** | `app` builds with `Dockerfile.service` → `backup-service:latest`; `worker` builds with `Dockerfile.worker` → `backup-worker:latest`. Both use explicit `command`. |
| **K8s** | API Deployment uses `image: backup-service:latest` and `command: ["backup-service"]`; Worker Deployment uses `image: backup-worker:latest` and `command: ["backup-worker"]`. Worker uses its own Secret (`backup-worker-secrets`) with only `AMQP_URL` (least privilege). |

So: **two images, one process per image**, with explicit commands and separate worker secrets.

---

## Option A: Single Dockerfile, Single Image (Current)

**What it is:** One Dockerfile builds the whole workspace and copies both binaries into one image. Runtime selects process via `ENTRYPOINT`/`command`.

### Pros

| # | Benefit |
|---|--------|
| 1 | **Single build** — one `docker build`, one push; CI is simple. |
| 2 | **Shared layers** — both binaries come from the same build cache; changing one crate still invalidates the final stage but dependency layers are shared. |
| 3 | **Version alignment** — API and worker always from the same build; no “API v1.2 vs worker v1.1” mismatch. |
| 4 | **Less to maintain** — one Dockerfile, one image name/tag in registries and K8s. |
| 5 | **Smaller total storage** — one image (with both binaries) instead of two images (each with one binary plus base). |

### Cons

| # | Drawback |
|---|----------|
| 1 | **Image contains unused binary** — API image ships the worker binary (and vice versa); slightly larger image and unnecessary surface. |
| 2 | **No independent deploy** — you cannot roll out “only worker” from a new image without that image also containing the API binary (and vice versa). You still version as one unit. |
| 3 | **Single point of build** — any change in any crate triggers a full image rebuild (though layer caching helps). |
| 4 | **ENTRYPOINT is API** — default `ENTRYPOINT ["backup-service"]` means worker must override; if someone runs the image without overriding, they get the API. |

### Best-practice fit

- **Good for:** Same repo, same release train, “we always deploy API and worker together from one tag.” Common in small/medium services and monorepos.

---

## Option B: Separate Dockerfiles, Separate Images

**What it is:** Two Dockerfiles (e.g. `Dockerfile.api`, `Dockerfile.worker`), each building only one binary. Two images: e.g. `backup-service:latest`, `backup-worker:latest`.

### Pros

| # | Benefit |
|---|--------|
| 1 | **Smaller per-image size** — API image contains only `backup-service`; worker image only `backup-worker`. Worker image can be noticeably smaller (no axum, sqlx, redis, etc.). |
| 2 | **Independent builds & deploys** — CI can build/push only the changed component; K8s can roll out API and worker on different schedules. |
| 3 | **Clear separation** — each Dockerfile documents exactly what that process needs; aligns with “one process per image” and microservice-style releases. |
| 4 | **Security** — each image has a smaller attack surface (no unused binary). |
| 5 | **Parallel builds** — two Docker builds can run in parallel in CI. |

### Cons

| # | Drawback |
|---|----------|
| 1 | **Two Dockerfiles to maintain** — build args, base image, and tooling updates must be kept in sync (can be mitigated with a shared base or build script). |
| 2 | **Version skew risk** — if API and worker are tagged independently, you can accidentally run incompatible versions; need a convention (e.g. same tag for both, or compatibility matrix). |
| 3 | **More CI/compose/K8s wiring** — two build jobs, two image names, two tags; docker-compose and K8s reference two images. |
| 4 | **No shared layer for “both binaries”** — if you often deploy both, total storage can be two images (each with base + one binary). |

### Best-practice fit

- **Good for:** Microservices, separate release trains, stricter size/security requirements, or when API and worker are owned/deployed by different teams.

---

## Option C: Single Dockerfile, Multiple Targets (BuildStage per Binary)

**What it is:** One Dockerfile with multiple named build stages; each stage builds only one binary. Use `docker build --target backup-service` or `--target backup-worker` to produce two different images from the same file.

### Pros

| # | Benefit |
|---|--------|
| 1 | **One Dockerfile** — single place for base image, tooling, and build logic. |
| 2 | **Two images** — each image contains only one binary; smaller images and independent deploy. |
| 3 | **Shared dependency caching** — first stage can build `backup-common` (and optionally both binaries for cache); then API/worker stages copy only the artifact they need. |
| 4 | **Same tag convention** — e.g. build with same tag for both targets so “release 1.2” stays aligned. |

### Cons

| # | Drawback |
|---|----------|
| 1 | **Dockerfile complexity** — multi-target Dockerfiles are longer and need careful ordering so cache is reusable. |
| 2 | **Build command differs per image** — CI must run `docker build --target backup-service` and `docker build --target backup-worker` (and optionally two pushes). |
| 3 | **Caching subtleties** — if both targets share an early stage, changing that stage invalidates both; still, better than building both binaries in one final stage. |

### Best-practice fit

- **Good for:** One repo, one Dockerfile, but desire for two smaller images and independent deploy without maintaining two full Dockerfiles.

---

## Recommendation Summary

| Context | Recommended option |
|--------|----------------------|
| **Learning / demo / single team, same release** | **A (current)** — one image, two entrypoints is simple and correct. |
| **Production, same release train but want smaller images** | **C** — one Dockerfile, multi-target; two images, one file to maintain. |
| **Production, separate release trains or teams** | **B** — separate Dockerfiles and images for maximum independence. |

For this project (demonstration of Rust backend patterns, same repo, same release), **Option A is acceptable**. If the goal is to also demonstrate “one process per image” and smaller worker images, **Option C** is the best trade-off: one Dockerfile, two images, no duplicate Dockerfile logic.

---

## Suggested Improvements (Regardless of Option)

These improve the current setup whether you stay with one image or move to two.

### 1. **Do not bake default ENTRYPOINT for API only**

**Issue:** Today the image has `ENTRYPOINT ["backup-service"]`. The worker then **overrides** with `command: ["backup-worker"]`. That works but couples the “default” to the API.

**Improvement:** Prefer a **neutral entrypoint** so the image does not imply “this is the API” when used without override. Two approaches:

- **A) No ENTRYPOINT, only default CMD:**  
  - In Dockerfile: remove `ENTRYPOINT`, set e.g. `CMD ["backup-service"]`.  
  - Compose/K8s: `app` uses `command: ["backup-service"]`, `worker` uses `command: ["backup-worker"]`.  
  - Then both roles are explicit in orchestration.

- **B) Keep ENTRYPOINT as API, document it**  
  - Leave as is, but in README/Docker docs state: “Default process is the API server; for the worker set `command: ["backup-worker"]`.”  
  - No change to files, just clarity.

Recommendation: **A** — explicit `command` for both services makes intent clear and avoids “why does worker need command?” confusion.

### 2. **docker-compose: explicit command for API**

**Current:** Only `worker` has `entrypoint: ["backup-worker"]`; `app` relies on image default.

**Improvement:** Set `command: ["backup-service"]` (or equivalent) for `app` as well, so both services explicitly declare their process. Aligns with “neutral image” above.

### 3. **K8s: explicit command for API**

**Current:** API Deployment has no `command`; worker has `command: ["backup-worker"]`.

**Improvement:** Add `command: ["backup-service"]` to the API Deployment so both Deployments explicitly specify the binary. Makes manifests self-describing and consistent.

### 4. **Image naming in K8s**

**Current:** Both Deployments use `image: backup-service:latest`.

**Improvement:**  
- If you stay with **one image**: keep `backup-service:latest` but consider tagging by version (e.g. `backup-service:1.2.3`) in real deployments.  
- If you move to **two images**: use `backup-service:latest` for the API and `backup-worker:latest` for the worker (and versioned tags in production). K8s manifests would reference the appropriate image per Deployment.

### 5. **Worker health check**

**Current:** Worker has a liveness probe `exec: ["true"]`, which always succeeds.

**Improvement:** If the worker can expose a trivial “am I alive” check (e.g. process responds to a signal or a tiny TCP port), consider using it. Otherwise, documenting that “worker has no HTTP endpoint, so we use `exec: true`” is sufficient. No change strictly required.

### 6. **Secrets: worker does not need DB/Redis/JWT**

**Current:** Worker Deployment uses `secretRef: backup-service-secrets`, which includes `DATABASE_URL`, `REDIS_URL`, `JWT_SECRET`. The worker only needs `AMQP_URL` (and optionally `GRPC_TARGET` if not in ConfigMap).

**Improvement:**  
- Use a **separate Secret** (e.g. `backup-worker-secrets`) with only `AMQP_URL` (and any worker-specific secrets).  
- Or keep one Secret but document that worker ignores DB/Redis/JWT.  
- Best practice: **least privilege** — worker Secret should not contain DB or JWT secrets.

---

## Summary Table

| Aspect | Current | Option A (keep 1 image) | Option B (2 Dockerfiles) | Option C (1 Dockerfile, 2 targets) |
|--------|--------|--------------------------|---------------------------|-------------------------------------|
| Dockerfiles | 1 | 1 | 2 | 1 |
| Images | 1 | 1 | 2 | 2 |
| Image size (worker) | Larger (has API binary too) | Same | Smallest | Smaller (worker only) |
| Independent deploy | No | No | Yes | Yes |
| CI complexity | Low | Low | Medium | Medium |
| Version alignment | Natural | Natural | Need process | Same tag for both |
| Best for | Demo/simple | Same | Microservices | This repo (if you want 2 images) |

**Conclusion:**  
- **Keep one image (Option A)** if you want minimal change and are fine with “one image, two entrypoints.” Apply the suggested improvements (explicit commands, optional Secret split, docs).  
- **Move to Option C** if you want two smaller, process-specific images and independent deploy while keeping a single Dockerfile.  
- **Move to Option B** only if you need fully separate build/release pipelines or different base images per process.

I can next:
1. **Implement Option A improvements only** (explicit commands, optional worker Secret, README note).  
2. **Implement Option C** (one Dockerfile with two targets, update compose and K8s to use two image names).  
3. **Implement Option B** (two Dockerfiles, two images, update compose and K8s).

Tell me which you prefer (1, 2, or 3).
