//! Fetch remote config via HTTP and run the updater loop.

use std::time::Duration;

use crate::apollo::merge::{RemoteConfigUpdate, apply_update};
use crate::state::AppState;

/// Config for the Apollo updater task.
#[derive(Clone)]
pub struct ApolloUpdaterConfig {
    pub url: String,
    pub interval: Duration,
    pub timeout: Duration,
}

/// Fetches config from the given URL. Returns error on HTTP or parse failure.
pub async fn fetch_config(
    client: &reqwest::Client,
    url: &str,
) -> Result<RemoteConfigUpdate, anyhow::Error> {
    let resp = client.get(url).send().await?;
    resp.error_for_status_ref()?;
    let update: RemoteConfigUpdate = resp.json().await?;
    Ok(update)
}

/// Runs the updater loop: every `config.interval`, fetches from `config.url` and merges into state.
/// Logs errors but does not panic. Run via `tokio::spawn`.
pub async fn run_updater_loop(
    state: AppState,
    config: ApolloUpdaterConfig,
    client: reqwest::Client,
) {
    let mut interval = tokio::time::interval(config.interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match fetch_config(&client, &config.url).await {
            Ok(update) => {
                state.with_config_mut(|cfg| apply_update(cfg, update));
                tracing::debug!(url = %config.url, "Apollo config updated");
            }
            Err(e) => {
                tracing::warn!(error = %e, url = %config.url, "Apollo config fetch failed");
            }
        }
    }
}

/// Runs a single update cycle (for tests). Returns true if fetch and merge succeeded.
pub async fn run_one_update(client: &reqwest::Client, url: &str, state: &AppState) -> bool {
    match fetch_config(client, url).await {
        Ok(update) => {
            state.with_config_mut(|cfg| apply_update(cfg, update));
            true
        }
        Err(e) => {
            tracing::warn!(error = %e, "run_one_update failed");
            false
        }
    }
}
