use async_trait::async_trait;

use crate::{
    api::Quality,
    db::provider::Provider,
    providers::{DownloadSource, DownloadTrackContext, PlaybackInfo, ProviderError},
};

pub struct MockProvider;

#[async_trait]
impl DownloadSource for MockProvider {
    fn id(&self) -> Provider {
        Provider::None
    }

    fn requires_linked_provider(&self) -> bool {
        false
    }

    async fn resolve_playback(
        &self,
        _external_track_id: &str,
        _quality: &Quality,
        _context: Option<&DownloadTrackContext>,
    ) -> Result<PlaybackInfo, ProviderError> {
        Ok(PlaybackInfo::DirectUrl(
            "https://example.com/mock.mp3".to_string(),
        ))
    }
}
