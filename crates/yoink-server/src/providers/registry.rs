use std::collections::HashSet;
use std::sync::Arc;

use crate::db::provider::Provider;

use super::{
    DownloadSource, MetadataProvider, ProviderArtist, ProviderSearchAlbum, ProviderSearchTrack,
};

/// Central registry that holds all enabled providers and dispatches operations.
pub(crate) struct ProviderRegistry {
    metadata: Vec<Arc<dyn MetadataProvider>>,
    download: Vec<DownloadSource>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            metadata: Vec::new(),
            download: Vec::new(),
        }
    }

    /// Register a provider that implements MetadataProvider.
    pub fn register_metadata(&mut self, provider: Arc<dyn MetadataProvider>) {
        if let Some(existing) = self
            .metadata
            .iter_mut()
            .find(|existing| existing.id() == provider.id())
        {
            *existing = provider;
        } else {
            self.metadata.push(provider);
        }
    }

    /// Register a provider that implements DownloadSource.
    pub fn register_download(&mut self, source: DownloadSource) {
        if let Some(existing) = self
            .download
            .iter_mut()
            .find(|existing| existing.id() == source.id())
        {
            *existing = source;
        } else {
            self.download.push(source);
        }
    }

    /// Fan-out search to all metadata providers concurrently.
    /// Returns a list of (provider_id, results).
    /// `providers` restricts the fan-out to the given set; `None` searches all.
    pub async fn search_artists_all(
        &self,
        query: &str,
        providers: Option<&HashSet<Provider>>,
    ) -> Vec<(Provider, Vec<ProviderArtist>)> {
        let mut handles = Vec::new();

        for provider in self.filtered_metadata(providers) {
            let p = Arc::clone(provider);
            let q = query.to_string();
            handles.push(tokio::spawn(async move {
                let id = p.id();
                match p.search_artists(&q).await {
                    Ok(artists) => (id, artists),
                    Err(error) => {
                        log_search_error(id, "artist", &error);
                        (id, Vec::new())
                    }
                }
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }
        results
    }

    /// Fan-out album search to all metadata providers concurrently.
    /// Returns a list of (provider, results).
    pub async fn search_albums_all(
        &self,
        query: &str,
        providers: Option<&HashSet<Provider>>,
    ) -> Vec<(Provider, Vec<ProviderSearchAlbum>)> {
        let mut handles = Vec::new();

        for provider in self.filtered_metadata(providers) {
            let p = Arc::clone(provider);
            let q = query.to_string();
            handles.push(tokio::spawn(async move {
                let id = p.id();
                match p.search_albums(&q).await {
                    Ok(albums) => (id, albums),
                    Err(error) => {
                        log_search_error(id, "album", &error);
                        (id, Vec::new())
                    }
                }
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }
        results
    }

    /// Fan-out track search to all metadata providers concurrently.
    /// Returns a list of (provider, results).
    pub async fn search_tracks_all(
        &self,
        query: &str,
        providers: Option<&HashSet<Provider>>,
    ) -> Vec<(Provider, Vec<ProviderSearchTrack>)> {
        let mut handles = Vec::new();

        for provider in self.filtered_metadata(providers) {
            let p = Arc::clone(provider);
            let q = query.to_string();
            handles.push(tokio::spawn(async move {
                let id = p.id();
                match p.search_tracks(&q).await {
                    Ok(tracks) => (id, tracks),
                    Err(error) => {
                        log_search_error(id, "track", &error);
                        (id, Vec::new())
                    }
                }
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }
        results
    }

    /// Metadata providers restricted to `providers`; `None` yields all.
    fn filtered_metadata<'a>(
        &'a self,
        providers: Option<&'a HashSet<Provider>>,
    ) -> impl Iterator<Item = &'a Arc<dyn MetadataProvider>> {
        self.metadata
            .iter()
            .filter(move |p| providers.is_none_or(|filter| filter.contains(&p.id())))
    }

    /// Get a specific metadata provider by ID.
    pub fn metadata_provider(&self, id: Provider) -> Option<Arc<dyn MetadataProvider>> {
        self.metadata.iter().find(|p| p.id() == id).cloned()
    }

    /// Get a specific download source by ID.
    pub fn download_source(&self, id: Provider) -> Option<DownloadSource> {
        self.download.iter().find(|s| s.id() == id).cloned()
    }

    /// List all enabled metadata provider IDs.
    pub fn metadata_provider_ids(&self) -> Vec<String> {
        self.metadata.iter().map(|p| p.id().to_string()).collect()
    }

    /// List all enabled download sources.
    pub fn download_sources(&self) -> &[DownloadSource] {
        &self.download
    }
}

fn log_search_error(provider: Provider, resource: &str, error: &super::ProviderError) {
    if matches!(error, super::ProviderError::NotSupported { .. }) {
        tracing::debug!(%provider, resource, "Provider does not support this search");
    } else {
        tracing::warn!(%provider, resource, %error, "Provider search failed");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::ProviderRegistry;
    use crate::{
        db::provider::Provider,
        providers::{
            DownloadSource,
            mock::{TestLinkedTrackResolver, TestMetadataProvider, TestSearchTrackResolver},
        },
    };

    #[test]
    fn registering_same_download_provider_replaces_existing_entry() {
        let mut registry = ProviderRegistry::new();
        registry.register_download(DownloadSource::Linked(std::sync::Arc::new(
            TestLinkedTrackResolver::new(Provider::Tidal),
        )));
        registry.register_download(DownloadSource::Search(std::sync::Arc::new(
            TestSearchTrackResolver::new(Provider::Tidal),
        )));

        assert_eq!(registry.download_sources().len(), 1);
        assert!(matches!(
            registry.download_source(Provider::Tidal),
            Some(DownloadSource::Search(_))
        ));
    }

    #[tokio::test]
    async fn search_fans_out_only_to_filtered_providers() {
        let mut registry = ProviderRegistry::new();
        registry.register_metadata(std::sync::Arc::new(TestMetadataProvider::new(
            Provider::Tidal,
        )));
        registry.register_metadata(std::sync::Arc::new(TestMetadataProvider::new(
            Provider::Deezer,
        )));

        let all = registry.search_artists_all("query", None).await;
        assert_eq!(all.len(), 2);

        let filter: HashSet<Provider> = [Provider::Deezer].into();
        let filtered = registry.search_artists_all("query", Some(&filter)).await;
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, Provider::Deezer);
    }
}
