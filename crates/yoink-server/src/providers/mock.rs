use async_trait::async_trait;

use crate::{
    api::Quality,
    db::provider::Provider,
    providers::{
        DownloadTrackContext, LinkedTrackResolver, MetadataProvider, PlaybackInfo, ProviderArtist,
        ProviderError, SearchTrackResolver,
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

pub(crate) struct TestMetadataProvider {
    provider: Provider,
}

impl TestMetadataProvider {
    pub(crate) fn new(provider: Provider) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl MetadataProvider for TestMetadataProvider {
    fn id(&self) -> Provider {
        self.provider
    }

    async fn search_artists(&self, _query: &str) -> Result<Vec<ProviderArtist>, ProviderError> {
        Ok(vec![ProviderArtist {
            external_id: format!("{}-artist", self.provider),
            name: self.provider.to_string(),
            image_ref: None,
            url: None,
            disambiguation: None,
            artist_type: None,
            country: None,
            tags: Vec::new(),
            popularity: None,
        }])
    }

    async fn fetch_albums(
        &self,
        _external_artist_id: &str,
    ) -> Result<Vec<super::ProviderAlbum>, ProviderError> {
        Ok(Vec::new())
    }

    async fn fetch_tracks(
        &self,
        _external_album_id: &str,
    ) -> Result<Vec<super::ProviderTrack>, ProviderError> {
        Ok(Vec::new())
    }

    fn image_url(&self, image_ref: &str, _size: u16) -> String {
        format!("https://example.test/{image_ref}")
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
