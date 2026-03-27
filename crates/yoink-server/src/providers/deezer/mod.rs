use std::{collections::HashMap, num::NonZeroU32, time::Duration};

use async_trait::async_trait;
use chrono::NaiveDate;
use governor::{Quota, RateLimiter, clock::DefaultClock, state::InMemoryState, state::NotKeyed};
use serde::Deserialize;
use serde_json::Value;
use tracing::warn;

use crate::db::provider::Provider;

use super::{
    MetadataProvider, ProviderAlbum, ProviderArtist, ProviderError, ProviderSearchAlbum,
    ProviderSearchTrack, ProviderTrack,
};

// ── Deezer API base ─────────────────────────────────────────────────

const DEEZER_API_BASE: &str = "https://api.deezer.com";

// ── API response models ─────────────────────────────────────────────

/// Wrapper for paginated list responses.
#[derive(Debug, Deserialize)]
struct DeezerList<T> {
    data: Vec<T>,
    #[allow(dead_code)]
    total: Option<u64>,
    next: Option<String>,
}

/// Top-level error object returned inside an otherwise-200 response.
#[derive(Debug, Deserialize)]
struct DeezerErrorBody {
    error: DeezerApiError,
}

#[derive(Debug, Deserialize)]
struct DeezerApiError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
    code: i32,
}

#[derive(Debug, Deserialize)]
struct DeezerArtist {
    id: u64,
    name: String,
    /// MD5 hash for artist picture (may be absent).
    #[serde(default)]
    picture_medium: Option<String>,
    #[serde(default)]
    picture_big: Option<String>,
    #[serde(default)]
    nb_album: Option<u32>,
    #[serde(default)]
    nb_fan: Option<u64>,
    #[serde(default)]
    link: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeezerAlbum {
    id: u64,
    title: String,
    #[serde(default)]
    record_type: Option<String>,
    /// "YYYY-MM-DD" or sometimes just "YYYY".
    #[serde(default)]
    release_date: Option<String>,
    /// MD5 hash for constructing CDN cover URLs.
    #[serde(default)]
    md5_image: Option<String>,
    #[serde(default)]
    explicit_lyrics: bool,
}

#[derive(Debug, Deserialize)]
struct DeezerTrack {
    id: u64,
    title: String,
    #[serde(default)]
    title_version: Option<String>,
    #[serde(default)]
    isrc: Option<String>,
    /// Duration in seconds.
    duration: i32,
    #[serde(default)]
    track_position: Option<i32>,
    #[serde(default)]
    disk_number: Option<i32>,
    #[serde(default)]
    explicit_lyrics: bool,
}

/// An album returned by `/search/album` — includes embedded artist info.
#[derive(Debug, Deserialize)]
struct DeezerSearchAlbum {
    id: u64,
    title: String,
    #[serde(default)]
    record_type: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    md5_image: Option<String>,
    #[serde(default)]
    explicit_lyrics: bool,
    /// Album link on Deezer.
    #[serde(default)]
    link: Option<String>,
    /// Primary artist embedded in album search results.
    #[serde(default)]
    artist: Option<DeezerSearchAlbumArtist>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerSearchAlbumArtist {
    id: u64,
    name: String,
}

/// A track returned by `/search/track` — includes embedded artist + album info.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DeezerSearchTrack {
    id: u64,
    title: String,
    #[serde(default)]
    title_version: Option<String>,
    #[serde(default)]
    isrc: Option<String>,
    /// Duration in seconds.
    duration: u32,
    #[serde(default)]
    explicit_lyrics: bool,
    #[serde(default)]
    link: Option<String>,
    /// Artist for this track.
    #[serde(default)]
    artist: Option<DeezerSearchTrackArtist>,
    /// Album this track belongs to.
    #[serde(default)]
    album: Option<DeezerSearchTrackAlbum>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerSearchTrackArtist {
    id: u64,
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerSearchTrackAlbum {
    id: u64,
    title: String,
    #[serde(default)]
    md5_image: Option<String>,
}

// ── DeezerProvider ──────────────────────────────────────────────────

pub(crate) struct DeezerProvider {
    http: reqwest::Client,
    rate_limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
}

impl DeezerProvider {
    pub fn new() -> Self {
        let user_agent = format!("Yoink/{} (flyinpancake@pm.me)", env!("CARGO_PKG_VERSION"));

        let http = reqwest::Client::builder()
            .user_agent(&user_agent)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client for Deezer");

        // Deezer allows 50 requests per 5 seconds.
        let quota = Quota::with_period(Duration::from_millis(100))
            .expect("non-zero period")
            .allow_burst(NonZeroU32::new(50).unwrap());

        let rate_limiter = RateLimiter::direct(quota);

        Self { http, rate_limiter }
    }

    /// Send a rate-limited GET request and deserialise the JSON body.
    ///
    /// Checks for Deezer's "error inside 200" pattern before attempting
    /// to deserialise into `T`.
    async fn deezer_get<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, ProviderError> {
        self.rate_limiter.until_ready().await;

        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| ProviderError::http("deezer", "request", e.to_string()))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ProviderError::parse("deezer", "response body", e.to_string()))?;

        // Deezer returns HTTP 200 even for errors — check the JSON body.
        if let Ok(err) = serde_json::from_str::<DeezerErrorBody>(&body) {
            if err.error.code == 4 || err.error.error_type.eq_ignore_ascii_case("QuotaException") {
                return Err(ProviderError::rate_limited(
                    "deezer",
                    format!(
                        "API error ({}): {} [code {}]",
                        err.error.error_type, err.error.message, err.error.code
                    ),
                ));
            }
            return Err(ProviderError::invalid_response(
                "deezer",
                format!(
                    "API error ({}): {} [code {}]",
                    err.error.error_type, err.error.message, err.error.code
                ),
            ));
        }

        if !status.is_success() {
            return Err(ProviderError::http(
                "deezer",
                "non-success status",
                format!("{status}: {body}"),
            ));
        }

        serde_json::from_str::<T>(&body)
            .map_err(|e| ProviderError::parse("deezer", "json decode", e.to_string()))
    }

    /// Build a Deezer API URL with query parameters (handles encoding).
    fn api_url(path: &str, params: &[(&str, &str)]) -> String {
        let mut url = format!("{DEEZER_API_BASE}{path}");
        if !params.is_empty() {
            url.push('?');
            for (i, (key, value)) in params.iter().enumerate() {
                if i > 0 {
                    url.push('&');
                }
                url.push_str(key);
                url.push('=');
                // Percent-encode the value for URL safety.
                for byte in value.bytes() {
                    match byte {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                            url.push(byte as char)
                        }
                        _ => {
                            url.push('%');
                            url.push_str(&format!("{byte:02X}"));
                        }
                    }
                }
            }
        }
        url
    }

    /// Fetch a paginated list, following `next` links until exhausted.
    async fn deezer_get_all<T: serde::de::DeserializeOwned>(
        &self,
        initial_url: &str,
    ) -> Result<Vec<T>, ProviderError> {
        let mut all = Vec::new();
        let mut url = initial_url.to_string();

        loop {
            let page: DeezerList<T> = self.deezer_get(&url).await?;
            all.extend(page.data);

            match page.next {
                Some(next_url) if !next_url.is_empty() => url = next_url,
                _ => break,
            }
        }

        Ok(all)
    }
}

/// Map Deezer `nb_fan` (fan count) to a 0–100 popularity value.
///
/// Uses a logarithmic scale so the huge range of fan counts compresses
/// into something useful:
///   0        → 0
///   ~100     → ~20
///   ~10 000  → ~50
///   ~1M      → ~75
///   ~100M    → ~100
fn fan_count_to_popularity(nb_fan: u64) -> u8 {
    if nb_fan == 0 {
        return 0;
    }
    let log_val = (nb_fan as f64).log10(); // log10(100M) ≈ 8
    let scaled = (log_val / 8.0 * 100.0).round() as u8;
    scaled.min(100)
}

/// Map Deezer `record_type` to the uppercase album type used internally.
fn map_record_type(record_type: &str) -> &str {
    match record_type {
        "album" => "ALBUM",
        "single" => "SINGLE",
        "ep" => "EP",
        "compile" => "COMPILATION",
        _ => "OTHER",
    }
}

// ── MetadataProvider impl ───────────────────────────────────────────

#[async_trait]
impl MetadataProvider for DeezerProvider {
    fn id(&self) -> Provider {
        Provider::Deezer
    }

    fn display_name(&self) -> &str {
        "Deezer"
    }

    async fn search_artists(&self, query: &str) -> Result<Vec<ProviderArtist>, ProviderError> {
        let url = Self::api_url("/search/artist", &[("q", query), ("limit", "25")]);
        let artists: DeezerList<DeezerArtist> = self.deezer_get(&url).await?;

        Ok(artists
            .data
            .into_iter()
            .map(|a| {
                // Extract md5 hash from picture URL and prefix with "artist:" to
                // distinguish from album cover hashes in image_url().
                let image_ref = extract_md5_from_picture_url(
                    a.picture_big.as_deref().or(a.picture_medium.as_deref()),
                )
                .map(|md5| format!("artist:{md5}"));

                let popularity = a.nb_fan.map(fan_count_to_popularity);
                let url = a
                    .link
                    .or_else(|| Some(format!("https://www.deezer.com/artist/{}", a.id)));

                let mut disambiguation_parts = Vec::new();
                if let Some(n) = a.nb_album {
                    disambiguation_parts.push(format!("{n} albums"));
                }
                if let Some(fans) = a.nb_fan
                    && fans > 0
                {
                    disambiguation_parts.push(format!("{} fans", format_fan_count(fans)));
                }
                let disambiguation = if disambiguation_parts.is_empty() {
                    None
                } else {
                    Some(disambiguation_parts.join(", "))
                };

                ProviderArtist {
                    external_id: a.id.to_string(),
                    name: a.name,
                    image_ref,
                    url,
                    disambiguation,
                    artist_type: None, // Deezer doesn't provide artist type
                    country: None,     // Deezer doesn't provide country
                    tags: Vec::new(),  // Deezer doesn't provide genre tags in search
                    popularity,
                }
            })
            .collect())
    }

    async fn fetch_albums(
        &self,
        external_artist_id: &str,
    ) -> Result<Vec<ProviderAlbum>, ProviderError> {
        let url = format!("{DEEZER_API_BASE}/artist/{external_artist_id}/albums?limit=50");
        let albums = self.deezer_get_all::<DeezerAlbum>(&url).await?;

        Ok(albums
            .into_iter()
            .map(|a| {
                let album_type = a
                    .record_type
                    .as_deref()
                    .map(|rt| map_record_type(rt).to_string());

                let url = Some(format!("https://www.deezer.com/album/{}", a.id));

                let release_date = a.release_date.as_deref().and_then(|d| {
                    // Normalize to "YYYY-MM-DD" if possible, or just "YYYY" if that's all we have.
                    if d.len() == 4 && d.chars().all(|c| c.is_ascii_digit()) {
                        match d.parse::<i32>() {
                            Ok(year) => NaiveDate::from_ymd_opt(year, 1, 1),
                            Err(_) => None,
                        }
                    } else if d.len() >= 10 && &d[4..5] == "-" && &d[7..8] == "-" {
                        NaiveDate::parse_from_str(&d[..10], "%Y-%m-%d").ok()
                    } else {
                        None
                    }
                });

                ProviderAlbum {
                    external_id: a.id.to_string(),
                    title: a.title,
                    album_type,
                    release_date,
                    cover_ref: a.md5_image,
                    url,
                    explicit: a.explicit_lyrics,
                }
            })
            .collect())
    }

    async fn fetch_tracks(
        &self,
        external_album_id: &str,
    ) -> Result<(Vec<ProviderTrack>, HashMap<String, Value>), ProviderError> {
        let url = format!("{DEEZER_API_BASE}/album/{external_album_id}/tracks?limit=200");
        let tracks = self.deezer_get_all::<DeezerTrack>(&url).await?;

        let album_extra = HashMap::new();

        let provider_tracks = tracks
            .into_iter()
            .map(|t| {
                let extra = HashMap::new();

                ProviderTrack {
                    external_id: t.id.to_string(),
                    title: t.title,
                    version: t.title_version.filter(|v| !v.is_empty()),
                    track_number: t.track_position.unwrap_or(1),
                    disc_number: t.disk_number,
                    duration_secs: t.duration,
                    isrc: t.isrc.filter(|s| !s.is_empty()),
                    explicit: t.explicit_lyrics,
                    extra,
                }
            })
            .collect();

        Ok((provider_tracks, album_extra))
    }

    async fn fetch_track_info_extra(
        &self,
        _external_track_id: &str,
    ) -> Option<HashMap<String, Value>> {
        // Deezer provides ISRC directly in track listings; no extra call needed.
        None
    }

    fn validate_image_id(&self, image_id: &str) -> bool {
        // Accept "artist:{md5}" or bare 32-char hex (album cover).
        let md5 = image_id.strip_prefix("artist:").unwrap_or(image_id);
        md5.len() == 32 && md5.chars().all(|c| c.is_ascii_hexdigit())
    }

    fn image_url(&self, image_ref: &str, size: u16) -> String {
        // Snap to reasonable Deezer CDN sizes.
        let sz = if size <= 56 {
            56
        } else if size <= 250 {
            250
        } else if size <= 500 {
            500
        } else {
            1000
        };

        // Distinguish artist pictures from album covers via "artist:" prefix.
        let (img_type, md5) = if let Some(stripped) = image_ref.strip_prefix("artist:") {
            ("artist", stripped)
        } else {
            ("cover", image_ref)
        };

        format!("https://cdn-images.dzcdn.net/images/{img_type}/{md5}/{sz}x{sz}-000000-80-0-0.jpg")
    }

    async fn fetch_cover_art_bytes(&self, image_ref: &str) -> Option<Vec<u8>> {
        // Determine image type from ref prefix.
        let (img_type, md5) = if let Some(stripped) = image_ref.strip_prefix("artist:") {
            ("artist", stripped)
        } else {
            ("cover", image_ref)
        };
        let url = format!(
            "https://cdn-images.dzcdn.net/images/{img_type}/{md5}/1000x1000-000000-80-0-0.jpg"
        );

        self.rate_limiter.until_ready().await;

        let resp = self.http.get(&url).send().await.ok()?;

        if !resp.status().is_success() {
            warn!(
                status = %resp.status(),
                image_ref,
                "Deezer CDN returned non-success for cover art"
            );
            return None;
        }

        resp.bytes().await.ok().map(|b| b.to_vec())
    }

    async fn fetch_artist_image_ref(
        &self,
        external_artist_id: &str,
        _name_hint: Option<&str>,
    ) -> Option<String> {
        let url = format!("{DEEZER_API_BASE}/artist/{external_artist_id}");
        let artist: DeezerArtist = self.deezer_get(&url).await.ok()?;
        extract_md5_from_picture_url(
            artist
                .picture_big
                .as_deref()
                .or(artist.picture_medium.as_deref()),
        )
        .map(|md5| format!("artist:{md5}"))
    }

    async fn search_albums(&self, query: &str) -> Result<Vec<ProviderSearchAlbum>, ProviderError> {
        let url = Self::api_url("/search/album", &[("q", query), ("limit", "25")]);
        let albums: DeezerList<DeezerSearchAlbum> = self.deezer_get(&url).await?;

        Ok(albums
            .data
            .into_iter()
            .map(|a| {
                let (artist_name, artist_external_id) = a
                    .artist
                    .map(|ar| (ar.name, ar.id.to_string()))
                    .unwrap_or_else(|| (yoink_shared::UNKNOWN_ARTIST.to_string(), String::new()));

                let album_type = a
                    .record_type
                    .as_deref()
                    .map(|rt| map_record_type(rt).to_string());

                let url = a
                    .link
                    .or_else(|| Some(format!("https://www.deezer.com/album/{}", a.id)));

                ProviderSearchAlbum {
                    external_id: a.id.to_string(),
                    title: a.title,
                    album_type,
                    release_date: a.release_date,
                    cover_ref: a.md5_image,
                    url,
                    explicit: a.explicit_lyrics,
                    artist_name,
                    artist_external_id,
                }
            })
            .collect())
    }

    async fn search_tracks(&self, query: &str) -> Result<Vec<ProviderSearchTrack>, ProviderError> {
        let url = Self::api_url("/search/track", &[("q", query), ("limit", "25")]);
        let tracks: DeezerList<DeezerSearchTrack> = self.deezer_get(&url).await?;

        Ok(tracks
            .data
            .into_iter()
            .map(|t| {
                let (artist_name, artist_external_id) = t
                    .artist
                    .map(|ar| (ar.name, ar.id.to_string()))
                    .unwrap_or_else(|| (yoink_shared::UNKNOWN_ARTIST.to_string(), String::new()));

                let (album_title, album_external_id, album_cover_ref) = t
                    .album
                    .map(|al| (al.title, al.id.to_string(), al.md5_image))
                    .unwrap_or_else(|| {
                        (yoink_shared::UNKNOWN_ALBUM.to_string(), String::new(), None)
                    });

                ProviderSearchTrack {
                    external_id: t.id.to_string(),
                    title: t.title,
                    version: t.title_version.filter(|v| !v.is_empty()),
                    duration_secs: t.duration,
                    isrc: t.isrc.filter(|s| !s.is_empty()),
                    explicit: t.explicit_lyrics,
                    artist_name,
                    artist_external_id,
                    album_title,
                    album_external_id,
                    album_cover_ref,
                }
            })
            .collect())
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Extract the MD5 hash from a Deezer CDN picture URL.
///
/// URLs look like: `https://cdn-images.dzcdn.net/images/artist/{md5}/{size}x{size}-...`
/// We extract the `{md5}` segment (32 hex chars).
fn extract_md5_from_picture_url(url: Option<&str>) -> Option<String> {
    let url = url?;
    // Split on '/' and find a 32-char hex segment.
    for segment in url.split('/') {
        if segment.len() == 32 && segment.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(segment.to_string());
        }
    }
    None
}

/// Format a fan count for display: "1.2M", "45K", "123", etc.
fn format_fan_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.0}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── fan_count_to_popularity ─────────────────────────────────

    #[test]
    fn popularity_zero_fans() {
        assert_eq!(fan_count_to_popularity(0), 0);
    }

    #[test]
    fn popularity_hundred_fans() {
        // log10(100) = 2, 2/8*100 = 25
        assert_eq!(fan_count_to_popularity(100), 25);
    }

    #[test]
    fn popularity_ten_thousand_fans() {
        // log10(10_000) = 4, 4/8*100 = 50
        assert_eq!(fan_count_to_popularity(10_000), 50);
    }

    #[test]
    fn popularity_one_million_fans() {
        // log10(1_000_000) = 6, 6/8*100 = 75
        assert_eq!(fan_count_to_popularity(1_000_000), 75);
    }

    #[test]
    fn popularity_hundred_million_fans() {
        // log10(100_000_000) = 8, 8/8*100 = 100
        assert_eq!(fan_count_to_popularity(100_000_000), 100);
    }

    #[test]
    fn popularity_clamped_at_100() {
        // Anything beyond 10^8 should still cap at 100.
        assert_eq!(fan_count_to_popularity(u64::MAX), 100);
    }

    // ── map_record_type ────────────────────────────────────────

    #[test]
    fn record_type_album() {
        assert_eq!(map_record_type("album"), "ALBUM");
    }

    #[test]
    fn record_type_single() {
        assert_eq!(map_record_type("single"), "SINGLE");
    }

    #[test]
    fn record_type_ep() {
        assert_eq!(map_record_type("ep"), "EP");
    }

    #[test]
    fn record_type_compile() {
        assert_eq!(map_record_type("compile"), "COMPILATION");
    }

    #[test]
    fn record_type_unknown() {
        assert_eq!(map_record_type("whatever"), "OTHER");
    }

    // ── extract_md5_from_picture_url ───────────────────────────

    #[test]
    fn extract_md5_real_url() {
        let url = "https://cdn-images.dzcdn.net/images/artist/abcdef0123456789abcdef0123456789/250x250-000000-80-0-0.jpg";
        assert_eq!(
            extract_md5_from_picture_url(Some(url)),
            Some("abcdef0123456789abcdef0123456789".to_string())
        );
    }

    #[test]
    fn extract_md5_none_input() {
        assert_eq!(extract_md5_from_picture_url(None), None);
    }

    #[test]
    fn extract_md5_no_hex_segment() {
        assert_eq!(
            extract_md5_from_picture_url(Some("https://example.com/no-md5-here")),
            None
        );
    }

    #[test]
    fn extract_md5_uppercase_hex() {
        let url = "https://cdn-images.dzcdn.net/images/cover/ABCDEF0123456789ABCDEF0123456789/500x500-000000-80-0-0.jpg";
        assert_eq!(
            extract_md5_from_picture_url(Some(url)),
            Some("ABCDEF0123456789ABCDEF0123456789".to_string())
        );
    }

    #[test]
    fn extract_md5_bare_string_no_slashes() {
        // A bare 32-char hex string is still found — split("/") yields the whole string.
        assert_eq!(
            extract_md5_from_picture_url(Some("abcdef0123456789abcdef0123456789")),
            Some("abcdef0123456789abcdef0123456789".to_string())
        );
    }

    // ── format_fan_count ───────────────────────────────────────

    #[test]
    fn format_fans_zero() {
        assert_eq!(format_fan_count(0), "0");
    }

    #[test]
    fn format_fans_small() {
        assert_eq!(format_fan_count(999), "999");
    }

    #[test]
    fn format_fans_thousands() {
        assert_eq!(format_fan_count(1_500), "2K");
    }

    #[test]
    fn format_fans_millions() {
        assert_eq!(format_fan_count(2_500_000), "2.5M");
    }

    // ── DeezerProvider::api_url ────────────────────────────────

    #[test]
    fn api_url_no_params() {
        let url = DeezerProvider::api_url("/search/artist", &[]);
        assert_eq!(url, "https://api.deezer.com/search/artist");
    }

    #[test]
    fn api_url_single_param() {
        let url = DeezerProvider::api_url("/search/artist", &[("q", "daft punk")]);
        assert_eq!(url, "https://api.deezer.com/search/artist?q=daft%20punk");
    }

    #[test]
    fn api_url_special_chars() {
        let url = DeezerProvider::api_url("/search/artist", &[("q", "björk & co")]);
        // 'ö' is 0xC3 0xB6 in UTF-8, '&' is 0x26, space is 0x20
        assert!(url.contains("q=bj%C3%B6rk%20%26%20co"));
    }

    #[test]
    fn api_url_multiple_params() {
        let url = DeezerProvider::api_url("/search/artist", &[("q", "test"), ("limit", "25")]);
        assert_eq!(url, "https://api.deezer.com/search/artist?q=test&limit=25");
    }

    // ── validate_image_id ──────────────────────────────────────

    #[test]
    fn validate_bare_md5() {
        let provider = DeezerProvider::new();
        assert!(provider.validate_image_id("abcdef0123456789abcdef0123456789"));
    }

    #[test]
    fn validate_artist_prefixed_md5() {
        let provider = DeezerProvider::new();
        assert!(provider.validate_image_id("artist:abcdef0123456789abcdef0123456789"));
    }

    #[test]
    fn validate_too_short() {
        let provider = DeezerProvider::new();
        assert!(!provider.validate_image_id("abcdef"));
    }

    #[test]
    fn validate_non_hex() {
        let provider = DeezerProvider::new();
        assert!(!provider.validate_image_id("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }

    #[test]
    fn validate_artist_prefix_short_hash() {
        let provider = DeezerProvider::new();
        assert!(!provider.validate_image_id("artist:tooshort"));
    }

    // ── image_url ──────────────────────────────────────────────

    #[test]
    fn image_url_cover_small() {
        let provider = DeezerProvider::new();
        let url = provider.image_url("abcdef0123456789abcdef0123456789", 50);
        assert_eq!(
            url,
            "https://cdn-images.dzcdn.net/images/cover/abcdef0123456789abcdef0123456789/56x56-000000-80-0-0.jpg"
        );
    }

    #[test]
    fn image_url_cover_large() {
        let provider = DeezerProvider::new();
        let url = provider.image_url("abcdef0123456789abcdef0123456789", 800);
        assert_eq!(
            url,
            "https://cdn-images.dzcdn.net/images/cover/abcdef0123456789abcdef0123456789/1000x1000-000000-80-0-0.jpg"
        );
    }

    #[test]
    fn image_url_artist_prefix() {
        let provider = DeezerProvider::new();
        let url = provider.image_url("artist:abcdef0123456789abcdef0123456789", 200);
        assert_eq!(
            url,
            "https://cdn-images.dzcdn.net/images/artist/abcdef0123456789abcdef0123456789/250x250-000000-80-0-0.jpg"
        );
    }

    #[test]
    fn image_url_exact_boundary_500() {
        let provider = DeezerProvider::new();
        let url = provider.image_url("abcdef0123456789abcdef0123456789", 500);
        assert_eq!(
            url,
            "https://cdn-images.dzcdn.net/images/cover/abcdef0123456789abcdef0123456789/500x500-000000-80-0-0.jpg"
        );
    }

    // ── JSON deserialization ───────────────────────────────────

    #[test]
    fn deserialize_error_body() {
        let json = r#"{"error":{"type":"QuotaException","message":"Too many requests","code":4}}"#;
        let err: DeezerErrorBody = serde_json::from_str(json).expect("should parse error body");
        assert_eq!(err.error.error_type, "QuotaException");
        assert_eq!(err.error.message, "Too many requests");
        assert_eq!(err.error.code, 4);
    }

    #[test]
    fn deserialize_artist_search_response() {
        let json = r#"{
            "data": [
                {
                    "id": 27,
                    "name": "Daft Punk",
                    "picture_medium": "https://cdn-images.dzcdn.net/images/artist/f2bc007e9133c946ac3c3907ddc5571b/250x250-000000-80-0-0.jpg",
                    "picture_big": "https://cdn-images.dzcdn.net/images/artist/f2bc007e9133c946ac3c3907ddc5571b/500x500-000000-80-0-0.jpg",
                    "nb_album": 32,
                    "nb_fan": 4875548,
                    "link": "https://www.deezer.com/artist/27"
                }
            ],
            "total": 1,
            "next": null
        }"#;

        let resp: DeezerList<DeezerArtist> =
            serde_json::from_str(json).expect("should parse artist list");

        assert_eq!(resp.data.len(), 1);
        let artist = &resp.data[0];
        assert_eq!(artist.id, 27);
        assert_eq!(artist.name, "Daft Punk");
        assert_eq!(artist.nb_album, Some(32));
        assert_eq!(artist.nb_fan, Some(4875548));
        assert!(artist.picture_big.is_some());
        assert!(resp.next.is_none());
    }

    #[test]
    fn deserialize_album_response() {
        let json = r#"{
            "data": [
                {
                    "id": 302127,
                    "title": "Discovery",
                    "record_type": "album",
                    "release_date": "2001-03-07",
                    "md5_image": "2e018122cb56986277102d2041a592c8",
                    "explicit_lyrics": false
                }
            ],
            "total": 1
        }"#;

        let resp: DeezerList<DeezerAlbum> =
            serde_json::from_str(json).expect("should parse album list");

        assert_eq!(resp.data.len(), 1);
        let album = &resp.data[0];
        assert_eq!(album.id, 302127);
        assert_eq!(album.title, "Discovery");
        assert_eq!(album.record_type.as_deref(), Some("album"));
        assert_eq!(album.release_date.as_deref(), Some("2001-03-07"));
        assert_eq!(
            album.md5_image.as_deref(),
            Some("2e018122cb56986277102d2041a592c8")
        );
        assert!(!album.explicit_lyrics);
    }

    #[test]
    fn deserialize_track_response() {
        let json = r#"{
            "data": [
                {
                    "id": 3135556,
                    "title": "One More Time",
                    "title_version": "",
                    "isrc": "GBDUW0000059",
                    "duration": 320,
                    "track_position": 1,
                    "disk_number": 1,
                    "explicit_lyrics": false
                }
            ],
            "total": 1
        }"#;

        let resp: DeezerList<DeezerTrack> =
            serde_json::from_str(json).expect("should parse track list");

        assert_eq!(resp.data.len(), 1);
        let track = &resp.data[0];
        assert_eq!(track.id, 3135556);
        assert_eq!(track.title, "One More Time");
        assert_eq!(track.isrc.as_deref(), Some("GBDUW0000059"));
        assert_eq!(track.duration, 320);
        assert_eq!(track.track_position, Some(1));
        assert_eq!(track.disk_number, Some(1));
        assert!(!track.explicit_lyrics);
    }

    #[test]
    fn deserialize_missing_optional_fields() {
        // Deezer sometimes omits optional fields entirely.
        let json = r#"{
            "data": [
                {
                    "id": 999,
                    "title": "Minimal Track",
                    "duration": 180
                }
            ],
            "total": 1
        }"#;

        let resp: DeezerList<DeezerTrack> =
            serde_json::from_str(json).expect("should parse with missing optionals");

        let track = &resp.data[0];
        assert_eq!(track.id, 999);
        assert!(track.title_version.is_none());
        assert!(track.isrc.is_none());
        assert_eq!(track.track_position, None);
        assert_eq!(track.disk_number, None);
        assert!(!track.explicit_lyrics); // default false
    }

    #[test]
    fn error_body_does_not_match_normal_response() {
        // A normal list response should NOT deserialize as DeezerErrorBody.
        let json = r#"{"data":[],"total":0}"#;
        assert!(serde_json::from_str::<DeezerErrorBody>(json).is_err());
    }
}
