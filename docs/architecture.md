# Architecture

## System Diagram

![Architecture diagram](architecture-diagram.png)

## Data Flow

1. **Client** sends a REST request (e.g. `POST /api/v1/backups/{id}/process`).
2. **backup-service** authenticates (JWT), validates, writes to **PostgreSQL**, and publishes a `BackupJob` message to **RabbitMQ**.
3. **backup-worker** consumes the message, calls `UpdateStatus` (gRPC) to mark the job as *running*.
4. Worker runs **Shannon entropy analysis** via C FFI (`entropy.c`).
5. Worker calls `ReportResult` (gRPC) with the outcome — service updates the database.
6. Both binaries export **distributed traces** to **Jaeger** via OpenTelemetry OTLP.

## Crate Dependency Graph

```
backup-service ──depends──► backup-common
backup-worker  ──depends──► backup-common
```

`backup-common` is the shared library; the two binaries never depend on each other.

## Technology Stack

| Layer | Technology |
|-------|-----------|
| HTTP framework | Axum 0.8 |
| gRPC | Tonic 0.12 + Prost 0.13 |
| Database | PostgreSQL 16 via SQLx 0.8 |
| Cache | Redis 7 via `redis` crate |
| Message queue | RabbitMQ 3 via `lapin` 2 |
| Auth | JWT (`jsonwebtoken` 9) |
| Observability | `tracing` + OpenTelemetry → Jaeger |
| Metrics | Prometheus (`metrics-exporter-prometheus`) |
| FFI | C entropy calculator compiled with `cc` |
| Containers | Docker (separate images), docker-compose |
| Orchestration | Kubernetes (Deployments, HPA, Ingress) |
| CI | GitHub Actions |
