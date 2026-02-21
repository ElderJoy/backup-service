# Backup Service — Rust Backend Prototype

Test project to learn main concepts, used for building robust backend service in Rust.

## Architecture

The system consists of two separate binaries communicating via message queue and gRPC:

- **backup-service** — REST API server (Axum) + gRPC server (Tonic). Handles HTTP requests, publishes backup jobs to RabbitMQ, and receives processing results from the worker via gRPC.
- **backup-worker** — Standalone worker binary. Consumes jobs from RabbitMQ, performs entropy analysis (C FFI), and reports results back to the API service via gRPC.

```
┌──────────────┐     RabbitMQ      ┌──────────────┐
│ backup-      │ ── job queue ──>  │ backup-      │
│ service      │                   │ worker       │
│ (API + gRPC) │ <── gRPC ──────  │ (consumer +  │
│              │    results        │  FFI)        │
└──────┬───────┘                   └──────────────┘
       │
   PostgreSQL / Redis
```

## Concepts Exercised

| Guide Section | Implementation |
|--------------|---------------|
| **Rust Core** | Newtypes (`TenantId`, `BackupId`), enums (`BackupStatus`, `AppError`), traits (`FromRow`, `IntoResponse`), `Result`/`?` error propagation |
| **HTTP & REST** | Axum framework: routing, extractors (`Path`, `Query`, `Json`, `State`, `Extension`), Tower middleware, CORS, compression, timeouts, request body limits |
| **gRPC** | Tonic server/client, Protocol Buffers (`prost`), `tonic-build` code generation, inter-service communication |
| **Auth** | JWT creation/verification with `jsonwebtoken`, auth middleware, claims extraction, role-based access |
| **Databases** | SQLx with PostgreSQL: connection pooling, runtime queries, migrations, parameterized queries (SQL injection prevention) |
| **TCP/IP** | TCP listener binding, graceful shutdown with signal handling |
| **Concurrency** | `Arc` for shared state, `tokio::task::spawn_blocking` for CPU work, `tokio::sync::Mutex` for rate limiter, `watch` channel for shutdown coordination |
| **Async Rust** | Tokio runtime, async handlers, graceful shutdown with `tokio::select!`, background worker with `tokio::spawn` |
| **Testing** | Unit tests (`#[test]`), async integration tests (`#[tokio::test]`), input validation tests, rate limiter tests, proto message tests |
| **Redis** | Caching layer with TTL, cache invalidation on mutations, graceful degradation when Redis is down |
| **RabbitMQ** | AMQP producer/consumer with `lapin`, durable exchanges/queues, message acknowledgement/rejection, dead-letter handling |
| **Docker & K8s** | Separate Dockerfiles per binary (Option B): `Dockerfile` → API image, `Dockerfile.worker` → worker image; docker-compose and K8s use two images, explicit commands, worker Secret (AMQP only) |
| **Observability** | `tracing` structured logging, Prometheus metrics endpoint (`/metrics`), OpenTelemetry distributed tracing exported to Jaeger, health checks |
| **FFI** | C entropy calculator called from Rust via `extern "C"`, compiled with `cc` crate in `build.rs` |
| **Rate Limiting** | Per-tenant sliding window rate limiter with `X-RateLimit-*` response headers, configurable via environment |
| **Cargo Workspace** | Three-crate workspace (`backup-common`, `backup-service`, `backup-worker`), `[workspace.dependencies]` for version alignment, enforced dependency boundaries |
| **Code Quality** | Input validation (path traversal prevention), proper error types with `thiserror`, newtype pattern, SAFETY comments on FFI |

## Project Structure (Cargo Workspace)

The project uses a **Cargo workspace** with three crates, each with its own
dependency set — the idiomatic Rust pattern for multi-binary projects
(used by Tokio, Hyper, Tonic, and rustc itself).

```
backup-service/                     # workspace root
├── Cargo.toml                      # [workspace] + [workspace.dependencies]
├── Cargo.lock                      # shared lockfile
├── Dockerfile                      # API image only (backup-service binary)
├── Dockerfile.worker               # Worker image only (backup-worker binary)
├── docker-compose.yml              # Local dev: API + worker + infra (two images)
├── proto/
│   └── backup.proto                # gRPC service definition (BackupProcessor)
├── k8s/                            # Kubernetes manifests
│   ├── config.yaml                 # Namespace, ServiceAccount, ConfigMaps, Secret
│   ├── deployment.yaml             # API server + Worker deployments
│   ├── service.yaml                # ClusterIP Service (HTTP + gRPC) + Ingress
│   └── hpa.yaml                    # HPA for API + Worker auto-scaling
├── migrations/
│   └── 001_create_backups.sql      # Database schema
│
├── crates/
│   ├── backup-common/              # shared library crate
│   │   ├── Cargo.toml
│   │   ├── build.rs                # cc (entropy.c) + tonic-build (proto)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── worker.rs           # BackupJob, AMQP topology, publish_job
│   │       ├── telemetry.rs        # OpenTelemetry + tracing-subscriber setup
│   │       ├── ffi/
│   │       │   ├── mod.rs          # Safe Rust wrapper for C entropy code
│   │       │   └── c_src/
│   │       │       └── entropy.c   # Shannon entropy in C (FFI target)
│   │       └── proto.rs            # tonic::include_proto! — shared gRPC types
│   │
│   ├── backup-service/             # API server binary crate
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs             # HTTP + gRPC server entry point
│   │   │   ├── lib.rs              # Module declarations
│   │   │   ├── config.rs           # Environment-based configuration
│   │   │   ├── state.rs            # AppState (Pool, Cache, RabbitMQ, RateLimiter)
│   │   │   ├── router.rs           # Route definitions + middleware stack
│   │   │   ├── grpc.rs             # gRPC server (BackupProcessor impl)
│   │   │   ├── errors.rs           # AppError enum → HTTP responses
│   │   │   ├── models.rs           # Domain types, newtypes, request/response DTOs
│   │   │   ├── cache.rs            # Redis caching with graceful degradation
│   │   │   ├── db/mod.rs           # Connection pool + migration runner
│   │   │   ├── handlers/
│   │   │   │   ├── auth.rs         # Login endpoint (JWT issuance)
│   │   │   │   ├── backups.rs      # CRUD + entropy analysis + job enqueue
│   │   │   │   └── health.rs       # Liveness + readiness probes
│   │   │   └── middleware/
│   │   │       ├── auth.rs         # JWT verification middleware
│   │   │       └── rate_limit.rs   # Per-tenant rate limiting middleware
│   │   └── tests/
│   │       └── api_tests.rs        # Integration tests (full HTTP lifecycle)
│   │
│   └── backup-worker/              # worker binary crate
│       ├── Cargo.toml
│       └── src/
│           └── main.rs             # RabbitMQ consumer + gRPC client + FFI
│
└── docs/                           # Architecture & migration documents
```

## Quick Start

### Option 1: Docker Compose (recommended)

```bash
docker-compose up --build
```

This builds two images (`backup-service:latest`, `backup-worker:latest`) and starts:
- **backup-service** on `localhost:8080` (HTTP) + `localhost:50051` (gRPC)
- **backup-worker** connected to RabbitMQ + API gRPC
- **PostgreSQL 16** on `localhost:5432`
- **Redis 7** on `localhost:6379`
- **RabbitMQ 3** on `localhost:5672` (management UI at `localhost:15672`)
- **Jaeger** on `localhost:16686` (trace UI)

To build images individually:
```bash
docker build -t backup-service:latest -f Dockerfile .
docker build -t backup-worker:latest -f Dockerfile.worker .
```

**Testing with Podman** (Docker-compatible CLI): ensure the Podman machine is running (`podman machine start` on macOS), then use `podman` in place of `docker`:
```bash
podman build -t backup-service:latest -f Dockerfile .
podman build -t backup-worker:latest -f Dockerfile.worker .
podman run --rm backup-service:latest backup-service  # exits when DB unavailable; confirms binary runs
podman run --rm backup-worker:latest backup-worker    # exits when AMQP unavailable; confirms binary runs
```
Full stack: `podman compose up --build` (or `docker compose up --build`).

### Option 2: Local Development

```bash
# Start dependencies
docker-compose up db redis rabbitmq jaeger -d

# Set environment
cp .env.example .env

# Run the API service
cargo run --bin backup-service

# Run the worker (in another terminal)
cargo run --bin backup-worker

# Or run tests
cargo test --lib          # unit tests only (no DB needed)
cargo test                # all tests (needs running PostgreSQL)
```

## API Endpoints

### Public

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/auth/login` | Get JWT token |
| `GET` | `/health` | Liveness probe |
| `GET` | `/ready` | Readiness probe (checks DB + Redis) |
| `GET` | `/metrics` | Prometheus metrics |

### Protected (requires `Authorization: Bearer <token>`)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/backups` | Create a backup |
| `GET` | `/api/v1/backups` | List backups (paginated, filterable) |
| `GET` | `/api/v1/backups/{id}` | Get a specific backup |
| `PATCH` | `/api/v1/backups/{id}` | Update backup status/size |
| `DELETE` | `/api/v1/backups/{id}` | Delete a backup |
| `POST` | `/api/v1/backups/{id}/analyze` | Run entropy analysis (FFI demo) |
| `POST` | `/api/v1/backups/{id}/process` | Enqueue backup job to RabbitMQ |

### Rate Limiting

All protected endpoints are rate-limited per tenant. Response headers:
- `X-RateLimit-Limit`: max requests per window
- `X-RateLimit-Remaining`: requests remaining
- `X-RateLimit-Reset`: window reset time in seconds
- Returns `429 Too Many Requests` when exceeded

## gRPC Service (Internal)

The `BackupProcessor` gRPC service is hosted by backup-service on port `50051` and used by backup-worker:

| RPC | Description |
|-----|-------------|
| `UpdateStatus` | Worker reports job status change (e.g., "running") |
| `ReportResult` | Worker reports final result (completed/failed, size, entropy) |

## Usage Examples

```bash
# Login (demo credentials: admin/admin or user/user)
TOKEN=$(curl -s localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}' | jq -r .token)

# Create a backup
curl -s localhost:8080/api/v1/backups \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"source_path":"/data/important","encryption_enabled":true}' | jq

# List backups
curl -s "localhost:8080/api/v1/backups?limit=10" \
  -H "Authorization: Bearer $TOKEN" | jq

# Get backup ID
BACKUP_ID=$(curl -s localhost:8080/api/v1/backups \
  -H "Authorization: Bearer $TOKEN" | jq -r '.items[0].id')

# Analyze entropy (FFI demo)
curl -s -X POST "localhost:8080/api/v1/backups/$BACKUP_ID/analyze" \
  -H "Authorization: Bearer $TOKEN" | jq

# Enqueue for async processing (RabbitMQ → worker → gRPC callback)
curl -s -X POST "localhost:8080/api/v1/backups/$BACKUP_ID/process" \
  -H "Authorization: Bearer $TOKEN" | jq

# Health checks
curl -s localhost:8080/health | jq
curl -s localhost:8080/ready | jq

# Prometheus metrics
curl -s localhost:8080/metrics
```

## Observability

### Tracing (Jaeger)

When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, distributed traces are exported to Jaeger via OTLP/gRPC.

Open the Jaeger UI at `http://localhost:16686` to view traces across both services.

### Metrics (Prometheus)

Scrape `/metrics` for Prometheus-format metrics including:
- `backup_jobs_processed_total` (counter, labeled by status)
- Standard HTTP request metrics from `tower-http`

### Structured Logs

All logs use `tracing` with structured fields. Set `RUST_LOG` to control verbosity:
```bash
RUST_LOG=backup_service=debug,tower_http=debug,sqlx=warn  # API service
RUST_LOG=backup_worker=debug                               # Worker
```

## Kubernetes Deployment

```bash
# Apply all manifests
kubectl apply -f k8s/config.yaml
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml
kubectl apply -f k8s/hpa.yaml

# Check status
kubectl get pods -n backup-service
kubectl get hpa -n backup-service
```

The K8s setup includes:
- **API Deployment** (`image: backup-service:latest`, explicit `command: ["backup-service"]`) with liveness/readiness/startup probes, HTTP + gRPC ports
- **Worker Deployment** (`image: backup-worker:latest`, explicit `command: ["backup-worker"]`) consuming from RabbitMQ, reporting via gRPC; uses **backup-worker-secrets** (AMQP only, least privilege)
- **HPA** auto-scaling API (3-20 pods) and Worker (2-10 pods) on CPU usage
- **Ingress** with TLS termination
- **ConfigMap** + **Secret** per component (API: backup-service-config + backup-service-secrets; Worker: backup-worker-config + backup-worker-secrets)
