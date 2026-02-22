//! Apollo config updater: periodically fetch remote config and merge into AppState.
//!
//! When `APOLLO_CONFIG_URL` is set, a background task GETs the URL and applies
//! only the allowed fields (rate_limit, cache_ttl_secs, cached_list_*) to the
//! in-memory config. URLs, secrets, and bind addresses are never overwritten from remote.

mod fetch;
mod merge;

pub use fetch::{ApolloUpdaterConfig, fetch_config, run_one_update, run_updater_loop};
pub use merge::{RemoteConfigUpdate, apply_update};
