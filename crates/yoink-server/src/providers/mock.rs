use async_trait::async_trait;

use crate::{
    api::Quality,
    db::provider::Provider,
    providers::{
        DownloadTrackContext, LinkedTrackResolver, PlaybackInfo, ProviderError, SearchTrackResolver,
    },
};

pub struct MockProvider;

#[async_trait]
impl SearchTrackResolver for MockProvider {
    fn id(&self) -> Provider {
        Provider::None
    }

    async fn resolve(
        &self,
        _metadata: &DownloadTrackContext,
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        Ok(PlaybackInfo::DirectUrl(
            "https://example.com/mock.mp3".to_string(),
        ))
    }
}

pub(crate) struct TestLinkedTrackResolver {
    provider: Provider,
}

impl TestLinkedTrackResolver {
    pub(crate) fn new(provider: Provider) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl LinkedTrackResolver for TestLinkedTrackResolver {
    fn id(&self) -> Provider {
        self.provider
    }

    async fn resolve(
        &self,
        _external_track_ids: &[String],
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        Ok(PlaybackInfo::DirectUrl(
            "https://example.test/track.flac".to_string(),
        ))
    }
}

pub(crate) struct TestSearchTrackResolver {
    provider: Provider,
}

impl TestSearchTrackResolver {
    pub(crate) fn new(provider: Provider) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl SearchTrackResolver for TestSearchTrackResolver {
    fn id(&self) -> Provider {
        self.provider
    }

    async fn resolve(
        &self,
        _metadata: &DownloadTrackContext,
        _quality: &Quality,
    ) -> Result<PlaybackInfo, ProviderError> {
        Ok(PlaybackInfo::DirectUrl(
            "https://example.test/track.flac".to_string(),
        ))
    }
}
