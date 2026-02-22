# Apollo Config Updater

The backup-service can periodically pull configuration from a remote HTTP endpoint (e.g. an Apollo or other config service) and merge it into the in-memory `AppConfig`. Only a safe subset of options is updated; URLs, secrets, and bind addresses are never overwritten from the remote.

## When it runs

- **Disabled by default**: If `APOLLO_CONFIG_URL` is unset or empty, no updater task is spawned.
- **When enabled**: A background task runs for the lifetime of the process. It GETs the config URL on a fixed interval, parses the JSON, and merges the allowed fields into the shared config (held in `AppState`). The first fetch happens after the first interval; failures are logged and do not affect the running server.

## Environment variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `APOLLO_CONFIG_URL` | Full URL to fetch config (GET). If unset or empty, updater is disabled. | — |
| `APOLLO_POLL_INTERVAL_SECS` | Seconds between fetch attempts. | `60` |
| `APOLLO_TIMEOUT_SECS` | HTTP client timeout for each request. | `10` |

## Remote payload format

The endpoint must return JSON. Only the following fields are applied; all are optional. Omitted or `null` values leave the current config unchanged.

```json
{
  "rate_limit": {
    "max_requests": 100,
    "window_secs": 60
  },
  "cache_ttl_secs": 300,
  "cached_list_limit": 20,
  "cached_list_offset": 0
}
```

- **`rate_limit.max_requests`** — max requests per tenant per window.
- **`rate_limit.window_secs`** — rate limit window in seconds.
- **`cache_ttl_secs`** — Redis cache TTL for backup/list entries (seconds).
- **`cached_list_limit`** / **`cached_list_offset`** — default list params used for the cache key (first-page list).

**Not updatable from remote** (always from environment only): `database_url`, `redis_url`, `amqp_url`, `jwt_secret`, `listen_addr`, `grpc_addr`.

## Implementation summary

- **Config** ([config.rs](crates/backup-service/src/config.rs)): `apollo_config_url`, `apollo_poll_interval_secs`, `apollo_timeout_secs` on `AppConfig`, read in `from_env()`.
- **Module** ([apollo/](crates/backup-service/src/apollo/)): `RemoteConfigUpdate` DTO and `apply_update()` merge logic; `fetch_config()` and `run_updater_loop()` using `reqwest`; `run_one_update()` for tests.
- **State** ([state.rs](crates/backup-service/src/state.rs)): `with_config_mut()` for exclusive write access to config.
- **Main** ([main.rs](crates/backup-service/src/main.rs)): When `APOLLO_CONFIG_URL` is set, builds `reqwest::Client` and `ApolloUpdaterConfig`, spawns `run_updater_loop(state, config, client)`.

## Tests

- **Unit** (`apollo::merge::tests`): Merge logic — only set fields overwrite; empty update changes nothing; rate_limit partial and full updates.
- **Integration** (`tests/apollo_updater_tests.rs`): Wiremock mock server returns JSON; `run_one_update()` is called; assert `state.config()` has updated values. Second test: 500 response leaves config unchanged.

Run Apollo-related tests:

```bash
cargo test -p backup-service apollo::
cargo test -p backup-service --test apollo_updater_tests
```

Integration tests require `DATABASE_URL` (same as `api_tests`).

## Possible extensions

- Auth headers (e.g. `APOLLO_API_KEY`) for the config request.
- Metrics (success/failure counters) for the updater.
- Admin endpoint to trigger an immediate refresh.
- Graceful shutdown: signal the updater loop to exit before process exit.
