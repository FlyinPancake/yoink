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

    async fn resolve_by_id(
        &self,
        _external_track_ids: &[String],
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        Ok(PlaybackInfo::DirectUrl(
            "https://example.com/mock.mp3".to_string(),
        ))
    }

    async fn resolve_by_metadata(
        &self,
        _metadata: &DownloadTrackContext,
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        Ok(PlaybackInfo::DirectUrl(
            "https://example.com/mock.mp3".to_string(),
        ))
    }
}

pub(crate) struct TestDownloadSource {
    provider: Provider,
    requires_linked_provider: bool,
}

impl TestDownloadSource {
    pub(crate) fn new(provider: Provider, requires_linked_provider: bool) -> Self {
        Self {
            provider,
            requires_linked_provider,
        }
    }
}

#[async_trait]
impl DownloadSource for TestDownloadSource {
    fn id(&self) -> Provider {
        self.provider
    }

    fn requires_linked_provider(&self) -> bool {
        self.requires_linked_provider
    }

    async fn resolve_by_id(
        &self,
        _external_track_ids: &[String],
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        Ok(PlaybackInfo::DirectUrl(
            "https://example.test/track.flac".to_string(),
        ))
    }

    async fn resolve_by_metadata(
        &self,
        _metadata: &DownloadTrackContext,
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        Ok(PlaybackInfo::DirectUrl(
            "https://example.test/track.flac".to_string(),
        ))
    }
}
