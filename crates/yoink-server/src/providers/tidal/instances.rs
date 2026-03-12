//! Hifi-api instance discovery, caching, and ranking.
//!
//! Periodically fetches uptime feeds that list known hifi-api instances,
//! filters out down hosts, and ranks the remainder so the provider can
//! fail over transparently.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::models::{DownInstance, FeedInstance, RankedInstance, UptimeFeed};

/// How long cached instance data is considered fresh before a re-fetch.
const INSTANCE_CACHE_TTL: Duration = Duration::from_secs(300);

/// Known uptime feed URLs polled to discover hifi-api instances.
pub(crate) const UPTIME_FEEDS: [&str; 2] = [
    "https://tidal-uptime.jiffy-puffs-1j.workers.dev/",
    "https://tidal-uptime.props-76styles.workers.dev/",
];

// ── Instance cache ──────────────────────────────────────────────────

/// In-memory cache of discovered hifi-api instances and their health status.
///
/// Protected by an [`RwLock`] at the provider level and refreshed when
/// [`is_stale`](Self::is_stale) returns `true`.
#[derive(Debug, Clone)]
pub(crate) struct InstanceCache {
    pub last_refresh: Option<DateTime<Utc>>,
    pub last_refresh_instant: Option<Instant>,
    pub api: Vec<FeedInstance>,
    pub streaming: Vec<FeedInstance>,
    pub down: Vec<DownInstance>,
    pub ranked: Vec<RankedInstance>,
    pub active_base_url: Option<String>,
}

impl InstanceCache {
    /// Create an empty, immediately-stale cache.
    pub fn new() -> Self {
        Self {
            last_refresh: None,
            last_refresh_instant: None,
            api: Vec::new(),
            streaming: Vec::new(),
            down: Vec::new(),
            ranked: Vec::new(),
            active_base_url: None,
        }
    }

    /// Returns `true` when the cache has never been populated or has exceeded
    /// [`INSTANCE_CACHE_TTL`].
    pub fn is_stale(&self) -> bool {
        match self.last_refresh_instant {
            Some(last) => last.elapsed() > INSTANCE_CACHE_TTL,
            None => true,
        }
    }
}

// ── Instance management functions ───────────────────────────────────

/// Build a deduplicated, priority-ordered list of base URLs to try.
///
/// Order: manual override → last-known active instance → ranked discovered instances.
pub(crate) async fn candidate_base_urls(
    manual_base_url: Option<&str>,
    cache: &RwLock<InstanceCache>,
    http: &reqwest::Client,
) -> Vec<String> {
    ensure_instances_fresh(cache, http).await;

    let mut candidates = Vec::new();
    if let Some(manual) = manual_base_url {
        candidates.push(manual.to_string());
    }

    {
        let c = cache.read().await;
        if let Some(active) = &c.active_base_url {
            candidates.push(active.clone());
        }
        candidates.extend(c.ranked.iter().map(|instance| instance.url.clone()));
    }

    let mut seen = HashSet::new();
    candidates.retain(|url| seen.insert(url.clone()));
    candidates
}

/// Refresh the instance cache if it is stale (no-op when still fresh).
pub(crate) async fn ensure_instances_fresh(cache: &RwLock<InstanceCache>, http: &reqwest::Client) {
    let should_refresh = {
        let c = cache.read().await;
        c.is_stale()
    };
    if should_refresh {
        refresh_instances(cache, http).await;
    }
}

/// Record `base_url` as the currently active (last successful) instance.
pub(crate) async fn set_active_instance(cache: &RwLock<InstanceCache>, base_url: &str) {
    let mut c = cache.write().await;
    c.active_base_url = Some(base_url.to_string());
}

/// Fetch all uptime feeds, merge and deduplicate the results, filter out
/// down instances, rank the survivors, and store everything in the cache.
async fn refresh_instances(cache: &RwLock<InstanceCache>, http: &reqwest::Client) {
    debug!("Refreshing hifi instance feed cache");
    let mut merged_api = Vec::new();
    let mut merged_streaming = Vec::new();
    let mut merged_down = Vec::new();

    for feed_url in UPTIME_FEEDS {
        let send_res = http
            .get(feed_url)
            .timeout(Duration::from_secs(6))
            .send()
            .await;

        let Ok(resp) = send_res else {
            debug!(feed_url, "Failed to fetch uptime feed");
            continue;
        };

        let Ok(ok_resp) = resp.error_for_status() else {
            debug!(feed_url, "Uptime feed returned non-success status");
            continue;
        };

        let Ok(feed) = ok_resp.json::<UptimeFeed>().await else {
            debug!(feed_url, "Failed to parse uptime feed JSON");
            continue;
        };

        merged_api.extend(feed.api);
        merged_streaming.extend(feed.streaming);
        merged_down.extend(feed.down);
    }

    let down_set = merged_down
        .iter()
        .map(|entry| entry.url.clone())
        .collect::<HashSet<_>>();

    merged_api = dedup_instances(merged_api)
        .into_iter()
        .filter(|instance| !down_set.contains(&instance.url))
        .collect();
    merged_streaming = dedup_instances(merged_streaming)
        .into_iter()
        .filter(|instance| !down_set.contains(&instance.url))
        .collect();

    let ranked = rank_instances(&merged_streaming, &merged_api);
    info!(
        api = merged_api.len(),
        streaming = merged_streaming.len(),
        down = merged_down.len(),
        ranked = ranked.len(),
        "Refreshed hifi instance cache"
    );

    let mut c = cache.write().await;
    c.last_refresh = Some(Utc::now());
    c.last_refresh_instant = Some(Instant::now());
    c.api = merged_api;
    c.streaming = merged_streaming;
    c.down = dedup_down(merged_down);
    c.ranked = ranked;
}

// ── Pure helpers ────────────────────────────────────────────────────

/// Deduplicate instances by URL, keeping the entry with the highest version.
fn dedup_instances(instances: Vec<FeedInstance>) -> Vec<FeedInstance> {
    let mut by_url: HashMap<String, FeedInstance> = HashMap::new();
    for instance in instances {
        by_url
            .entry(instance.url.clone())
            .and_modify(|existing| {
                if version_key(&instance.version) > version_key(&existing.version) {
                    *existing = instance.clone();
                }
            })
            .or_insert(instance);
    }
    let mut deduped: Vec<_> = by_url.into_values().collect();
    deduped.sort_by(|a, b| {
        version_key(&b.version)
            .cmp(&version_key(&a.version))
            .then_with(|| a.url.cmp(&b.url))
    });
    deduped
}

/// Deduplicate down-instance entries by URL.
fn dedup_down(entries: Vec<DownInstance>) -> Vec<DownInstance> {
    let mut by_url: HashMap<String, DownInstance> = HashMap::new();
    for entry in entries {
        by_url.insert(entry.url.clone(), entry);
    }
    let mut deduped: Vec<_> = by_url.into_values().collect();
    deduped.sort_by(|a, b| a.url.cmp(&b.url));
    deduped
}

/// Merge streaming and API instance lists into a single ranked list.
///
/// Streaming-capable instances are preferred over API-only ones, and
/// higher version numbers break ties within the same source category.
fn rank_instances(streaming: &[FeedInstance], api: &[FeedInstance]) -> Vec<RankedInstance> {
    let mut by_url: HashMap<String, RankedInstance> = HashMap::new();
    for item in api {
        by_url.insert(
            item.url.clone(),
            RankedInstance {
                url: item.url.clone(),
                version: item.version.clone(),
                source: "api".to_string(),
            },
        );
    }
    for item in streaming {
        by_url
            .entry(item.url.clone())
            .and_modify(|existing| {
                existing.source = "streaming".to_string();
                if version_key(&item.version) > version_key(&existing.version) {
                    existing.version = item.version.clone();
                }
            })
            .or_insert(RankedInstance {
                url: item.url.clone(),
                version: item.version.clone(),
                source: "streaming".to_string(),
            });
    }
    let mut ranked: Vec<_> = by_url.into_values().collect();
    ranked.sort_by(|a, b| {
        source_priority(&a.source)
            .cmp(&source_priority(&b.source))
            .then_with(|| version_key(&b.version).cmp(&version_key(&a.version)))
            .then_with(|| a.url.cmp(&b.url))
    });
    ranked
}

/// Map a source label to a sort key (lower is better).
fn source_priority(source: &str) -> u8 {
    match source {
        "streaming" => 0,
        _ => 1,
    }
}

/// Parse a semver-like `"major.minor.patch"` string into a comparable tuple.
fn version_key(version: &str) -> (u16, u16, u16) {
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
