//! Low-level HTTP helpers for talking to hifi-api instances.
//!
//! The single public function [`hifi_get_json`] performs a `GET` request,
//! automatically trying each candidate instance URL until one succeeds.

use std::{sync::Arc, time::Duration};

use serde::de::DeserializeOwned;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::instances::{self, InstanceCache};
use crate::providers::ProviderError;

/// Perform a JSON `GET` against the hifi-api, with automatic instance failover.
///
/// Candidate URLs are resolved via [`instances::candidate_base_urls`] and tried
/// in order. The first instance that returns a valid, deserializable response is
/// promoted to the active instance in the cache.
///
/// Returns a provider error when all candidates fail.
pub(crate) async fn hifi_get_json<T: DeserializeOwned>(
    http: &reqwest::Client,
    manual_base_url: Option<&str>,
    cache: &Arc<RwLock<InstanceCache>>,
    path: &str,
    query: Vec<(String, String)>,
) -> Result<T, ProviderError> {
    let candidates = instances::candidate_base_urls(manual_base_url, cache, http).await;
    let mut last_error = None;

    for base_url in candidates {
        let response = http
            .get(format!("{base_url}{path}"))
            .query(&query)
            .timeout(Duration::from_secs(8))
            .send()
            .await;

        match response {
            Ok(resp) => match resp.error_for_status() {
                Ok(ok) => match ok.json::<T>().await {
                    Ok(parsed) => {
                        instances::set_active_instance(cache, &base_url).await;
                        return Ok(parsed);
                    }
                    Err(err) => {
                        debug!(base_url, error = %err, "Upstream JSON parse failed");
                        last_error = Some(format!("{base_url}: invalid JSON ({err})"));
                    }
                },
                Err(err) => {
                    debug!(base_url, error = %err, "Upstream HTTP status failed");
                    last_error = Some(format!("{base_url}: upstream status error ({err})"));
                }
            },
            Err(err) => {
                debug!(base_url, error = %err, "Upstream request failed");
                last_error = Some(format!("{base_url}: request failed ({err})"));
            }
        }
    }

    let error_msg =
        last_error.unwrap_or_else(|| "No healthy hifi-api instances available".to_string());
    warn!(error = %error_msg, "All hifi-api candidates failed");
    Err(ProviderError::Unavailable {
        provider: crate::db::provider::Provider::Tidal,
        reason: error_msg,
    })
}
