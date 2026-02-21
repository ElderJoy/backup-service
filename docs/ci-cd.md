# CI/CD (GitHub Actions)

## Overview

The [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) workflow runs on every push and pull request to `main`/`master`. It:

1. **Test & Clippy** — Always runs: `cargo fmt --check`, `cargo clippy`, `cargo test` for the whole workspace.
2. **Build** — After tests pass: `cargo build --release` and upload of `backup-service` and `backup-worker` binaries as artifacts.
3. **Docker (service)** — Builds the API image from `Dockerfile.service` **only when** service-related paths changed.
4. **Docker (worker)** — Builds the worker image from `Dockerfile.worker` **only when** worker-related paths changed.

## Path-based Docker builds

Docker jobs use [dorny/paths-filter](https://github.com/dorny/paths-filter) so we don’t build both images on every commit.

| Job              | Runs when any of these change |
|------------------|-------------------------------|
| **docker-service** | `Cargo.toml`, `Cargo.lock`, `crates/backup-common/**`, `crates/backup-service/**`, `Dockerfile.service`, `docker-compose.yml`, `proto/**`, `migrations/**` |
| **docker-worker**  | `Cargo.toml`, `Cargo.lock`, `crates/backup-common/**`, `crates/backup-worker/**`, `Dockerfile.worker`, `docker-compose.yml`, `proto/**` |

- Changes only in `crates/backup-service` (or service-only paths) → only **docker-service** runs.
- Changes only in `crates/backup-worker` (or worker-only paths) → only **docker-worker** runs.
- Changes in `crates/backup-common` or `proto/**` → **both** Docker jobs run.

Docker builds run only on `push` or on pull requests from the same repository (not from forks), to avoid extra load and permission issues.

## Why not separate builds for “service vs worker” for tests?

Tests and the release build always run for the **entire workspace**. That keeps behavior simple and guarantees that changes in `backup-common` don’t break either binary. Skipping tests for one crate based on paths would require path-aware test selection and could miss cross-crate breakages, so we run the full test suite every time.

## Local testing

### 1. Lint the workflow (catch syntax and action refs)

Install [actionlint](https://github.com/rhysd/actionlint) and run:

```bash
# Install (macOS)
brew install actionlint

# Lint workflow
actionlint .github/workflows/ci.yml
```

Fixes YAML issues and invalid action versions before you push.

### 2. Run the same steps as CI (no extra tools)

From the repo root, with Rust (stable + clippy) installed:

```bash
# Test job
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast

# Build job
cargo build --release
```

Docker (optional, same as CI):

```bash
docker build -f Dockerfile.service -t backup-service:latest .
docker build -f Dockerfile.worker -t backup-worker:latest .
```

### 3. Run the workflow with act (optional)

[act](https://github.com/nektos/act) runs GitHub Actions in Docker on your machine.

```bash
# Install (macOS)
brew install act

# List jobs (dry run)
act -n

# Run only the test job (no Docker jobs)
act -j test

# Run test + build (no Docker)
act -j test -j build

# Run everything (including Docker builds; needs Docker)
act
```

Note: Path-filter and event conditions may differ locally (e.g. `act` uses a default event), so the Docker jobs might run or be skipped. The **test** and **build** jobs always run and match CI.
