# backup-service

REST API server and gRPC callback endpoint. This is the main user-facing binary — it handles HTTP requests, manages backups in PostgreSQL, caches with Redis, and dispatches async work to the worker via RabbitMQ.

## Architecture

```
                HTTP :8080               gRPC :50051
Client ──────► Axum Router ──────┐       Tonic Server ◄──── backup-worker
               │                 │           │
               ▼                 │           ▼
           Handlers              │       Update DB
               │                 │
        ┌──────┼──────┐          │
        ▼      ▼      ▼          │
    PostgreSQL Redis  RabbitMQ ──┘
```

- **HTTP** (Axum) serves REST endpoints for CRUD, auth, health, and metrics.
- **gRPC** (Tonic) listens for status/result callbacks from the worker.
- Both servers run concurrently in the same Tokio runtime with graceful shutdown.

## Modules

| Module | Responsibility |
|--------|---------------|
| `main.rs` | Entry point — starts HTTP + gRPC servers, Apollo updater (if configured), signal handling |
| `router.rs` | Route tree + middleware stack (auth, rate limit, CORS, compression, tracing) |
| `config.rs` | `AppConfig` loaded from environment variables (incl. Apollo URL, poll interval, timeout) |
| `state.rs` | `AppState` — shared state (DB pool, cache, config, rate limiter, AMQP channel) |
| `apollo/` | Remote config updater: fetch from URL, merge into config, background loop |
| `db.rs` | PostgreSQL connection pool + migration runner |
| `cache.rs` | Redis caching layer with TTL, graceful degradation when Redis is down |
| `errors.rs` | `AppError` enum → HTTP status codes (via `IntoResponse`) |
| `models.rs` | Domain types: `Backup`, `BackupStatus`, DTOs, `Claims`, newtypes (`TenantId`, `BackupId`) |
| `grpc.rs` | `BackupProcessor` gRPC service impl (receives worker callbacks) |
| `handlers/auth.rs` | `POST /api/v1/auth/login` — JWT issuance |
| `handlers/backups.rs` | CRUD + entropy analysis + job enqueue |
| `handlers/health.rs` | `/health` (liveness) + `/ready` (readiness: DB + Redis) |
| `middleware/auth.rs` | JWT verification middleware, claims extraction |
| `middleware/rate_limit.rs` | Per-tenant sliding window rate limiter with `X-RateLimit-*` headers |

## API Endpoints

### Public

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/auth/login` | Get JWT token |
| `GET` | `/health` | Liveness probe |
| `GET` | `/ready` | Readiness probe (DB + Redis) |
| `GET` | `/metrics` | Prometheus metrics |

### Protected (`Authorization: Bearer <token>`)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/backups` | Create backup |
| `GET` | `/api/v1/backups` | List (paginated, filterable by status) |
| `GET` | `/api/v1/backups/{id}` | Get by ID |
| `PATCH` | `/api/v1/backups/{id}` | Update status/size |
| `DELETE` | `/api/v1/backups/{id}` | Delete |
| `POST` | `/api/v1/backups/{id}/analyze` | Run entropy analysis (FFI) |
| `POST` | `/api/v1/backups/{id}/process` | Enqueue job to RabbitMQ |

Rate-limited per tenant — see `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` response headers.

### gRPC (Internal, port 50051)

| RPC | Called by | Description |
|-----|-----------|-------------|
| `UpdateStatus` | Worker | Report job status change (e.g. `running`) |
| `ReportResult` | Worker | Report final result (completed/failed, size, entropy) |

## Middleware Stack

Applied in order (outermost first):

1. **CORS** — permissive for development
2. **Compression** — gzip response compression
3. **Timeout** — 30s request timeout
4. **Body limit** — 1 MB max request body
5. **Tracing** — request/response logging via `tower-http`
6. **Auth** — JWT verification (protected routes only)
7. **Rate limit** — per-tenant sliding window (protected routes only)

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `axum` | HTTP framework (routing, extractors, middleware) |
| `tonic` | gRPC server |
| `sqlx` | PostgreSQL (async, compile-time-safe queries) |
| `redis` | Caching with connection manager |
| `lapin` | RabbitMQ producer |
| `jsonwebtoken` | JWT creation and verification |
| `tower` / `tower-http` | Middleware (CORS, compression, tracing, timeout) |
| `metrics-exporter-prometheus` | `/metrics` endpoint |
| `thiserror` / `anyhow` | Error types |

## Configuration (environment)

In addition to `DATABASE_URL`, `REDIS_URL`, `AMQP_URL`, `JWT_SECRET`, `LISTEN_ADDR`, `GRPC_LISTEN_ADDR`, and rate-limit/cache vars, the following enable optional remote config updates:

| Variable | Purpose | Default |
|----------|---------|---------|
| `APOLLO_CONFIG_URL` | URL to fetch config (GET). If set, a background task merges allowed fields into config. | — (disabled) |
| `APOLLO_POLL_INTERVAL_SECS` | Seconds between fetches. | `60` |
| `APOLLO_TIMEOUT_SECS` | HTTP timeout for config request. | `10` |

See [Apollo Config Updater](../../docs/apollo-config-updater.md) for payload format and which fields are updated.

## Testing

Integration tests in `tests/api_tests.rs` run the full HTTP lifecycle (auth → create → list → update → delete) against a real PostgreSQL database. `tests/apollo_updater_tests.rs` tests the Apollo config updater with a mock HTTP server.

```bash
cargo test -p backup-service --lib       # unit tests (no DB)
cargo test -p backup-service             # all tests (needs PostgreSQL)
```
