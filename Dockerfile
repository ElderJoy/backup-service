# Stage 1: Build
FROM rust:1.85-bookworm AS builder

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies — copy all Cargo.toml/lock first with stub sources
COPY Cargo.toml Cargo.lock ./
COPY crates/backup-common/Cargo.toml crates/backup-common/
COPY crates/backup-service/Cargo.toml crates/backup-service/
COPY crates/backup-worker/Cargo.toml crates/backup-worker/

# Create stub sources for dependency caching
RUN mkdir -p crates/backup-common/src \
    && echo "pub fn _stub() {}" > crates/backup-common/src/lib.rs \
    && mkdir -p crates/backup-common/src/ffi/c_src \
    && touch crates/backup-common/src/ffi/c_src/entropy.c \
    && mkdir -p proto && touch proto/backup.proto \
    && mkdir -p crates/backup-service/src \
    && echo "fn main() {}" > crates/backup-service/src/main.rs \
    && touch crates/backup-service/src/lib.rs \
    && mkdir -p crates/backup-worker/src \
    && echo "fn main() {}" > crates/backup-worker/src/main.rs

# Copy build scripts and real FFI/proto sources for dep caching
COPY crates/backup-common/build.rs crates/backup-common/
COPY crates/backup-common/src/ffi/c_src/entropy.c crates/backup-common/src/ffi/c_src/
COPY proto/ proto/
RUN cargo build --release 2>/dev/null || true
RUN rm -rf crates/*/src

# Build the real application
COPY crates/ crates/
COPY migrations/ migrations/
RUN touch crates/backup-common/src/lib.rs \
    crates/backup-service/src/main.rs crates/backup-service/src/lib.rs \
    crates/backup-worker/src/main.rs \
    && cargo build --release

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
