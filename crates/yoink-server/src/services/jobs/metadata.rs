use std::time::Duration;

use crate::{db, db::provider::Provider, state::AppState};

/// Fetch album cover art JPEG bytes for embedding into track tags.
///
/// Downloads the album's stored cover URL through Yoink's image proxy.
pub async fn fetch_album_cover_art(
    state: &AppState,
    album: &db::album::ModelEx,
) -> Option<Vec<u8>> {
    if let Some(ref cover_url) = album.cover_url {
        let cover_url = if cover_url.starts_with("https://") {
            safe_absolute_cover_url(cover_url)
        } else {
            let (provider, image_ref) = parse_provider_image_url(cover_url)?;
            let metadata = state.registry.metadata_provider(provider)?;
            metadata
                .validate_image_id(image_ref)
                .then(|| metadata.image_url(image_ref, 1080))
        };
        let cover_url = cover_url?;
        match state
            .http
            .get(cover_url.to_string())
            .timeout(Duration::from_secs(20))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(bytes) = resp.bytes().await {
                    tracing::debug!(url = %cover_url, bytes = bytes.len(), "Fetched cover art from cover_url");
                    return Some(bytes.to_vec());
                }
            }
            Ok(resp) => {
                tracing::warn!(url = %cover_url, status = %resp.status(), "cover_url returned non-success status");
            }
            Err(err) => {
                tracing::warn!(url = %cover_url, error = %err, "Failed to fetch cover art from cover_url");
            }
        }
    }

    tracing::debug!(album_id = %album.id, "No cover art available for album");
    None
}

fn safe_absolute_cover_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    matches!(
        host,
        "resources.tidal.com" | "cdn-images.dzcdn.net" | "coverartarchive.org"
    )
    .then(|| parsed.to_string())
}

fn parse_provider_image_url(url: &str) -> Option<(Provider, &str)> {
    let path = url.strip_prefix("/api/image/")?;
    let (provider, remainder) = path.split_once('/')?;
    let (image_ref, size) = remainder.rsplit_once('/')?;
    size.parse::<u16>().ok()?;
    let provider = provider.parse().ok()?;
    (!image_ref.is_empty()).then_some((provider, image_ref))
}

#[cfg(test)]
mod tests {
    use crate::db::provider::Provider;

    use super::{parse_provider_image_url, safe_absolute_cover_url};

    #[test]
    fn parses_provider_image_proxy_url() {
        assert_eq!(
            parse_provider_image_url("/api/image/tidal/abc-def/640"),
            Some((Provider::Tidal, "abc-def")),
        );
        assert_eq!(
            parse_provider_image_url("/api/image/deezer/artist:abc123/250"),
            Some((Provider::Deezer, "artist:abc123")),
        );
    }

    #[test]
    fn rejects_non_provider_image_urls() {
        assert!(parse_provider_image_url("/covers/local.jpg").is_none());
        assert!(parse_provider_image_url("/api/image/unknown/ref/640").is_none());
        assert!(parse_provider_image_url("/api/image/tidal/ref/not-a-size").is_none());
    }

    #[test]
    fn only_allows_known_provider_cover_hosts() {
        assert!(safe_absolute_cover_url("https://resources.tidal.com/image.jpg").is_some());
        assert!(safe_absolute_cover_url("https://cdn-images.dzcdn.net/image.jpg").is_some());
        assert!(safe_absolute_cover_url("http://localhost:3000/private").is_none());
        assert!(safe_absolute_cover_url("https://example.com/image.jpg").is_none());
    }
}
