# Backup Service — Rust Backend Prototype

Test project to learn main concepts, used for building robust backend service in Rust.

## Concepts Exercised

| Guide Section | Implementation |
|--------------|---------------|
| **Rust Core** | Newtypes (`TenantId`, `BackupId`), enums (`BackupStatus`, `AppError`), traits (`FromRow`, `IntoResponse`), `Result`/`?` error propagation |
| **HTTP & REST** | Axum framework: routing, extractors (`Path`, `Query`, `Json`, `State`, `Extension`), Tower middleware, CORS, compression, timeouts, request body limits |
| **Auth** | JWT creation/verification with `jsonwebtoken`, auth middleware, claims extraction, role-based access |
| **Databases** | SQLx with PostgreSQL: connection pooling, runtime queries, migrations, parameterized queries (SQL injection prevention) |
| **TCP/IP** | TCP listener binding, graceful shutdown with signal handling |
| **Concurrency** | `Arc` for shared state, `tokio::task::spawn_blocking` for CPU work, `tokio::sync::Mutex` for rate limiter, `watch` channel for shutdown coordination |
| **Async Rust** | Tokio runtime, async handlers, graceful shutdown with `tokio::select!`, background worker with `tokio::spawn` |
| **Testing** | Unit tests (`#[test]`), async integration tests (`#[tokio::test]`), input validation tests, rate limiter tests |
| **Redis** | Caching layer with TTL, cache invalidation on mutations, graceful degradation when Redis is down |
| **RabbitMQ** | AMQP producer/consumer with `lapin`, durable exchanges/queues, message acknowledgement/rejection, dead-letter handling |
| **Docker & K8s** | Multi-stage Dockerfile, docker-compose with PostgreSQL + Redis + RabbitMQ + Jaeger, Kubernetes manifests (Deployment, Service, Ingress, ConfigMap, Secret, HPA) |
| **Observability** | `tracing` structured logging, Prometheus metrics endpoint (`/metrics`), OpenTelemetry distributed tracing exported to Jaeger, health checks |
| **FFI** | C entropy calculator called from Rust via `extern "C"`, compiled with `cc` crate in `build.rs` |
| **Rate Limiting** | Per-tenant sliding window rate limiter with `X-RateLimit-*` response headers, configurable via environment |
| **Code Quality** | Input validation (path traversal prevention), proper error types with `thiserror`, newtype pattern, SAFETY comments on FFI |

## Project Structure

```
backup-service/
├── build.rs                    # Compiles C code for FFI
├── Cargo.toml                  # Dependencies
├── docker-compose.yml          # Local dev: app + Postgres + Redis + RabbitMQ + Jaeger
├── Dockerfile                  # Multi-stage production build
├── k8s/                        # Kubernetes manifests
│   ├── config.yaml             # Namespace, ServiceAccount, ConfigMap, Secret
│   ├── deployment.yaml         # API server + Worker deployments
│   ├── service.yaml            # ClusterIP Service + Ingress
│   └── hpa.yaml                # HorizontalPodAutoscaler for API + Worker
├── migrations/
│   └── 001_create_backups.sql  # Database schema
├── src/
│   ├── main.rs                 # Entry point: runtime, worker spawn, signal handling
│   ├── lib.rs                  # Public modules (for integration tests)
│   ├── config.rs               # Environment-based configuration
│   ├── state.rs                # Shared app state (Pool, Cache, RabbitMQ, RateLimiter)
│   ├── router.rs               # Route definitions + middleware stack
│   ├── errors.rs               # AppError enum → HTTP responses (thiserror)
│   ├── models.rs               # Domain types, newtypes, request/response DTOs
│   ├── cache.rs                # Redis caching with graceful degradation
│   ├── telemetry.rs            # OpenTelemetry + tracing-subscriber setup
│   ├── worker.rs               # RabbitMQ background job consumer
│   ├── db/
│   │   └── mod.rs              # Connection pool + migration runner
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── auth.rs             # Login endpoint (JWT issuance)
│   │   ├── backups.rs          # CRUD + entropy analysis + job enqueue
│   │   └── health.rs           # Liveness + readiness probes
│   ├── middleware/
│   │   ├── mod.rs
│   │   ├── auth.rs             # JWT verification middleware
│   │   └── rate_limit.rs       # Per-tenant rate limiting middleware
│   └── ffi/
│       ├── mod.rs              # Safe Rust wrapper for C entropy code
│       └── c_src/
│           └── entropy.c       # Shannon entropy in C (FFI target)
└── tests/
    └── api_tests.rs            # Integration tests (full HTTP lifecycle)
```

## Quick Start

### Option 1: Docker Compose (recommended)

```bash
docker-compose up --build
```

This starts:
- **backup-service** on `localhost:8080`
- **PostgreSQL 16** on `localhost:5432`
- **Redis 7** on `localhost:6379`
- **RabbitMQ 3** on `localhost:5672` (management UI at `localhost:15672`)
- **Jaeger** on `localhost:16686` (trace UI)

### Option 2: Local Development

```bash
# Start dependencies
docker-compose up db redis rabbitmq jaeger -d

# Set environment
cp .env.example .env

# Run the service
cargo run

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

# Enqueue for async processing (RabbitMQ)
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

Open the Jaeger UI at `http://localhost:16686` to view traces across services.

### Metrics (Prometheus)

Scrape `/metrics` for Prometheus-format metrics including:
- `backup_jobs_processed_total` (counter, labeled by status)
- Standard HTTP request metrics from `tower-http`

### Structured Logs

All logs use `tracing` with structured fields. Set `RUST_LOG` to control verbosity:
```bash
RUST_LOG=backup_service=debug,tower_http=debug,sqlx=warn
```

## Kubernetes Deployment

```bash
# Apply all manifests
kubectl apply -f k8s/config.yaml
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml
kubectl apply -f k8s/hpa.yaml

# Check status
kubectl get pods -n acronis
kubectl get hpa -n acronis
```

The K8s setup includes:
- **API Deployment** (3 replicas) with liveness/readiness/startup probes
- **Worker Deployment** (2 replicas) consuming from RabbitMQ
- **HPA** auto-scaling API (3-20 pods) and Worker (2-10 pods) on CPU usage
- **Ingress** with TLS termination
- **ConfigMap** + **Secret** for configuration
