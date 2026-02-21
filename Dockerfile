# Stage 1: Build
FROM rust:1.85-bookworm AS builder

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && mkdir -p src/bin && echo "fn main() {}" > src/bin/backup-worker.rs \
    && mkdir -p src/ffi/c_src && touch src/ffi/c_src/entropy.c \
    && mkdir -p proto && touch proto/backup.proto
COPY build.rs .
COPY src/ffi/c_src/entropy.c src/ffi/c_src/
COPY proto/ proto/
RUN cargo build --release 2>/dev/null || true
RUN rm -rf src

# Build the real application
COPY src/ src/
COPY migrations/ migrations/
RUN touch src/main.rs src/bin/backup-worker.rs && cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r app && useradd -r -g app app

COPY --from=builder /app/target/release/backup-service /usr/local/bin/
COPY --from=builder /app/target/release/backup-worker /usr/local/bin/

USER app
EXPOSE 8080 50051

HEALTHCHECK --interval=30s --timeout=3s \
    CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["backup-service"]
