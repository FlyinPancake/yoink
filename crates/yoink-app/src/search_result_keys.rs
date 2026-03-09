use uuid::Uuid;

use yoink_shared::MonitoredArtist;

pub(crate) fn provider_result_key(provider: &str, external_id: &str) -> String {
    format!(
        "{}:{provider}|{}:{external_id}",
        provider.len(),
        external_id.len()
    )
}

pub(crate) fn monitored_artist_key(artist: &MonitoredArtist) -> Uuid {
    artist.id
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;
    use uuid::Uuid;

    use yoink_shared::MonitoredArtist;

    use super::{monitored_artist_key, provider_result_key};

    #[test]
    fn provider_result_key_is_stable_for_same_identity() {
        assert_eq!(
            provider_result_key("tidal", "123"),
            provider_result_key("tidal", "123")
        );
    }

    #[test]
    fn provider_result_key_distinguishes_providers() {
        assert_ne!(
            provider_result_key("tidal", "123"),
            provider_result_key("deezer", "123")
        );
    }

    #[test]
    fn provider_result_key_avoids_delimiter_collisions() {
        let first = provider_result_key("spotify", "a:b");
        let second = provider_result_key("spotify:a", "b");

        assert_ne!(first, second);

        let mut entries = HashMap::new();
        entries.insert(first, "first");
        entries.insert(second, "second");

        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn monitored_artist_key_uses_uuid_identity() {
        let artist = MonitoredArtist {
            id: Uuid::from_u128(1),
            name: "Artist".to_string(),
            image_url: Some("https://example.com/image.jpg".to_string()),
            bio: Some("Bio".to_string()),
            monitored: true,
            added_at: Utc::now(),
        };

        assert_eq!(monitored_artist_key(&artist), artist.id);
    }
}
