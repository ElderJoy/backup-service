# Backup Service — Rust Backend Prototype

## Concepts Exercised

| Guide Section | Implementation |
|--------------|---------------|
| **Rust Core** | Newtypes (`TenantId`, `BackupId`), enums (`BackupStatus`, `AppError`), traits (`FromRow`, `IntoResponse`), `Result`/`?` error propagation |
| **HTTP & REST** | Axum framework: routing, extractors (`Path`, `Query`, `Json`, `State`, `Extension`), Tower middleware, CORS, compression, timeouts |
| **Auth** | JWT creation/verification with `jsonwebtoken`, auth middleware, claims extraction, role-based access |
| **Databases** | SQLx with PostgreSQL: connection pooling, runtime queries, migrations, parameterized queries (SQL injection prevention) |
| **TCP/IP** | TCP listener binding, graceful shutdown with signal handling |
| **Concurrency** | `Arc` for shared state, `tokio::task::spawn_blocking` for CPU work, async/await throughout |
| **Async Rust** | Tokio runtime, async handlers, graceful shutdown with `tokio::select!` |
| **Testing** | Unit tests (`#[test]`), async integration tests (`#[tokio::test]`), input validation tests |
| **Redis** | Caching layer with TTL, cache invalidation on mutations, graceful degradation when Redis is down |
| **Docker & K8s** | Multi-stage Dockerfile, docker-compose with PostgreSQL + Redis, health/readiness probes |
| **Observability** | `tracing` structured logging, Prometheus metrics endpoint (`/metrics`), health checks |
| **FFI** | C entropy calculator called from Rust via `extern "C"`, compiled with `cc` crate in `build.rs` |
| **Code Quality** | Input validation (path traversal prevention), proper error types with `thiserror`, newtype pattern |

## Project Structure

```
backup-service/
├── build.rs                    # Compiles C code for FFI
├── Cargo.toml                  # Dependencies
├── docker-compose.yml          # Local dev environment
├── Dockerfile                  # Multi-stage production build
├── migrations/
│   └── 001_create_backups.sql  # Database schema
├── src/
│   ├── main.rs                 # Entry point: runtime setup, signal handling
│   ├── lib.rs                  # Public modules (for integration tests)
│   ├── config.rs               # Environment-based configuration
│   ├── state.rs                # Shared application state (Arc<Pool>, Cache)
│   ├── router.rs               # Route definitions + middleware stack
│   ├── errors.rs               # AppError enum → HTTP responses (thiserror)
│   ├── models.rs               # Domain types, newtypes, request/response DTOs
│   ├── cache.rs                # Redis caching with graceful degradation
│   ├── db/
│   │   └── mod.rs              # Connection pool + migration runner
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── auth.rs             # Login endpoint (JWT issuance)
│   │   ├── backups.rs          # CRUD + entropy analysis (FFI demo)
│   │   └── health.rs           # Liveness + readiness probes
│   ├── middleware/
│   │   ├── mod.rs
│   │   └── auth.rs             # JWT verification middleware
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

### Option 2: Local Development

```bash
# Start dependencies
docker-compose up db redis -d

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

# Analyze entropy (FFI demo)
BACKUP_ID=$(curl -s localhost:8080/api/v1/backups \
  -H "Authorization: Bearer $TOKEN" | jq -r '.items[0].id')

curl -s -X POST "localhost:8080/api/v1/backups/$BACKUP_ID/analyze" \
  -H "Authorization: Bearer $TOKEN" | jq

# Health checks
curl -s localhost:8080/health | jq
curl -s localhost:8080/ready | jq

# Prometheus metrics
curl -s localhost:8080/metrics
```
