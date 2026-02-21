# backup-worker

Background worker binary that consumes backup jobs from RabbitMQ, performs entropy analysis via C FFI, and reports results back to `backup-service` via gRPC.

## Architecture

```
RabbitMQ ──consume──► Worker ──gRPC──► backup-service
                        │
                        ▼
                   C FFI (entropy.c)
```

The worker is a single-binary, single-responsibility process:

1. **Connect** to RabbitMQ and the API's gRPC endpoint.
2. **Consume** `BackupJob` messages from the `backup_jobs` queue.
3. **Call** `UpdateStatus(running)` via gRPC.
4. **Analyze** — compute Shannon entropy on synthetic data via FFI (`backup_common::ffi`).
5. **Report** — call `ReportResult` with status, entropy value, and suspicion flag.
6. **Ack/Reject** the AMQP message based on success or failure.

## Internal Workflow

```
┌─────────────────────────────────────────────┐
│ Startup                                     │
│  1. Load .env + init telemetry              │
│  2. Connect RabbitMQ (retry loop)           │
│  3. Connect gRPC client to backup-service   │
│  4. Start consumer on backup_jobs queue     │
└──────────────────┬──────────────────────────┘
                   │
         ┌─────────▼──────────┐
         │ For each message:  │
         │  • Deserialize job │
         │  • gRPC: running   │
         │  • FFI: entropy    │
         │  • gRPC: result    │
         │  • Ack message     │
         └────────────────────┘
                   │
         ┌─────────▼──────────┐
         │ Shutdown           │
         │  • SIGTERM/SIGINT  │
         │  • Flush OTel spans│
         │  • Close AMQP conn │
         └────────────────────┘
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `AMQP_URL` | Yes | RabbitMQ connection string (e.g. `amqp://guest:guest@localhost:5672/%2f`) |
| `GRPC_TARGET` | No | gRPC endpoint of backup-service (default: `http://localhost:50051`) |
| `RUST_LOG` | No | Log level filter (default: `info`) |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | No | Jaeger OTLP endpoint for distributed tracing |

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `backup-common` | Shared proto types, AMQP helpers, FFI, telemetry |
| `tonic` | gRPC client (`BackupProcessorClient`) |
| `lapin` | RabbitMQ consumer |
| `tokio` / `tokio-stream` | Async runtime + stream processing |
| `tracing` | Structured logging |

## Running

```bash
# Requires RabbitMQ + backup-service gRPC to be reachable
cargo run -p backup-worker

# Or via Docker
docker build -f Dockerfile.worker -t backup-worker:latest .
docker run --rm -e AMQP_URL=amqp://... backup-worker:latest
```
