use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use musicbrainz_rs::{
    MusicBrainzClient,
    entity::{
        artist::{Artist as MbArtist, ArtistSearchQuery},
        release::Release as MbRelease,
        release_group::{ReleaseGroup, ReleaseGroupPrimaryType, ReleaseGroupSearchQuery},
    },
    prelude::*,
};
use serde_json::Value;
use tracing::{debug, warn};

use super::{
    MetadataProvider, ProviderAlbum, ProviderArtist, ProviderError, ProviderSearchAlbum,
    ProviderTrack,
};

// ── MusicBrainzProvider ─────────────────────────────────────────────

pub(crate) struct MusicBrainzProvider {
    client: MusicBrainzClient,
    http: reqwest::Client,
}

impl MusicBrainzProvider {
    pub fn new() -> Self {
        let mut client = MusicBrainzClient::default();
        let user_agent = format!("Yoink/{} (flyinpancake@pm.me)", env!("CARGO_PKG_VERSION"));
        client
            .set_user_agent(&user_agent)
            .expect("invalid MusicBrainz user-agent");
        let http = reqwest::Client::builder()
            .user_agent(&user_agent)
            .build()
            .expect("failed to build HTTP client");
        Self { client, http }
    }

    /// Browse all release groups for an artist, paginating through 100 at a time.
    async fn browse_all_release_groups(
        &self,
        artist_mbid: &str,
    ) -> Result<Vec<ReleaseGroup>, ProviderError> {
        let mut all = Vec::new();
        let mut offset: u16 = 0;
        const LIMIT: u8 = 100;

        loop {
            let page = ReleaseGroup::browse()
                .by_artist(artist_mbid)
                .offset(offset)
                .limit(LIMIT)
                .execute_with_client(&self.client)
                .await
                .map_err(|e| {
                    ProviderError::http("musicbrainz", "browse release groups", e.to_string())
                })?;

            let count = page.entities.len();
            all.extend(page.entities);

            if count < LIMIT as usize {
                break;
            }
            offset += count as u16;
        }
        Ok(all)
    }

    /// Pick the best release from a release group for track listing.
    /// Prefers: Official status, digital format, most tracks.
    async fn best_release_for_group(
        &self,
        release_group_id: &str,
    ) -> Result<Option<MbRelease>, ProviderError> {
        // Fetch the release group with releases included
        let rg = ReleaseGroup::fetch()
            .id(release_group_id)
            .with_releases()
            .execute_with_client(&self.client)
            .await
            .map_err(|e| {
                ProviderError::http("musicbrainz", "fetch release group", e.to_string())
            })?;

        let Some(releases) = rg.releases else {
            return Ok(None);
        };

        if releases.is_empty() {
            return Ok(None);
        }

        // Score each release to find the best one
        let best = releases.into_iter().max_by_key(|r| {
            let mut score: i32 = 0;
            // Prefer Official releases
            if r.status.as_ref().is_some_and(|s| {
                matches!(s, musicbrainz_rs::entity::release::ReleaseStatus::Official)
            }) {
                score += 100;
            }
            // Prefer releases with more media (they likely have tracks)
            if let Some(ref media) = r.media {
                let total_tracks: u32 = media.iter().map(|m| m.track_count).sum();
                score += total_tracks as i32;
            }
            score
        });

        Ok(best)
    }
}

impl MusicBrainzProvider {
    /// Fetch the Wikipedia extract for a MusicBrainz artist.
    /// Steps: fetch artist with URL rels → find Wikipedia/Wikidata link → get summary.
    async fn fetch_wikipedia_extract(&self, mbid: &str) -> Option<String> {
        // Fetch artist with URL relations to find Wikipedia link
        debug!(
            mbid,
            "Fetching MusicBrainz artist with URL relations for bio"
        );
        let artist = match MbArtist::fetch()
            .id(mbid)
            .with_url_relations()
            .execute_with_client(&self.client)
            .await
        {
            Ok(a) => a,
            Err(e) => {
                warn!(mbid, error = %e, "Failed to fetch MusicBrainz artist for bio");
                return None;
            }
        };

        let relations = match artist.relations.as_ref() {
            Some(r) => r,
            None => {
                debug!(mbid, artist_name = %artist.name, "Artist has no relations at all");
                return None;
            }
        };

        debug!(
            mbid,
            artist_name = %artist.name,
            relation_count = relations.len(),
            "Scanning artist relations for Wikipedia/Wikidata URLs"
        );

        // Log all URL relations for debugging
        for rel in relations {
            if let musicbrainz_rs::entity::relations::RelationContent::Url(url_entity) =
                &rel.content
            {
                debug!(
                    mbid,
                    relation_type = %rel.relation_type,
                    url = %url_entity.resource,
                    "Found URL relation"
                );
            }
        }

        // Look for a Wikipedia or Wikidata relation
        let wiki_url = relations.iter().find_map(|rel| {
            if let musicbrainz_rs::entity::relations::RelationContent::Url(url_entity) =
                &rel.content
            {
                let url = &url_entity.resource;
                if url.contains("wikipedia.org/wiki/") || url.contains("wikidata.org/wiki/") {
                    return Some(url.clone());
                }
            }
            None
        });

        let wiki_url = match wiki_url {
            Some(url) => {
                debug!(mbid, %url, "Found wiki URL relation");
                url
            }
            None => {
                // Try Wikidata relation type as fallback — some artists only have
                // a wikidata.org/entity/Qxxx link (not /wiki/).
                let wikidata_id = relations.iter().find_map(|rel| {
                    if let musicbrainz_rs::entity::relations::RelationContent::Url(url_entity) =
                        &rel.content
                    {
                        let url = &url_entity.resource;
                        // Match https://www.wikidata.org/wiki/Qxxx or /entity/Qxxx
                        if url.contains("wikidata.org") {
                            // Extract the Qxxx ID from the URL
                            let id = url.rsplit('/').next()?;
                            if id.starts_with('Q') {
                                return Some(id.to_string());
                            }
                        }
                    }
                    None
                });

                match wikidata_id {
                    Some(qid) => {
                        debug!(mbid, %qid, "No Wikipedia URL found, falling back to Wikidata entity");
                        return self.fetch_extract_via_wikidata(&qid).await;
                    }
                    None => {
                        debug!(mbid, "No Wikipedia or Wikidata URL relation found");
                        return None;
                    }
                }
            }
        };

        // If it's a Wikidata URL, resolve via Wikidata API
        if wiki_url.contains("wikidata.org") {
            let qid = wiki_url.rsplit('/').next()?;
            if qid.starts_with('Q') {
                debug!(mbid, qid, "Wiki URL is Wikidata, resolving via API");
                return self.fetch_extract_via_wikidata(qid).await;
            }
            debug!(mbid, %wiki_url, "Wikidata URL but could not extract QID");
            return None;
        }

        // It's a direct Wikipedia URL — extract the page title and fetch the summary
        if wiki_url.contains("wikipedia.org/wiki/") {
            return self.fetch_wikipedia_summary_from_url(&wiki_url, mbid).await;
        }

        debug!(mbid, %wiki_url, "Wiki URL does not match expected patterns");
        None
    }

    /// Fetch Wikipedia summary given a direct Wikipedia article URL.
    async fn fetch_wikipedia_summary_from_url(&self, wiki_url: &str, mbid: &str) -> Option<String> {
        // URL format: https://en.wikipedia.org/wiki/Page_Title
        let parts: Vec<&str> = wiki_url.splitn(2, "/wiki/").collect();
        if parts.len() != 2 {
            debug!(mbid, %wiki_url, "Could not split Wikipedia URL at /wiki/");
            return None;
        }
        let page_title = parts[1];
        let lang = wiki_url.strip_prefix("https://")?.split('.').next()?;

        let api_url = format!("https://{lang}.wikipedia.org/api/rest_v1/page/summary/{page_title}");

        debug!(mbid, %api_url, "Fetching Wikipedia summary");

        let resp = match self
            .http
            .get(&api_url)
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(mbid, %api_url, error = %e, "Wikipedia summary request failed");
                return None;
            }
        };

        if !resp.status().is_success() {
            warn!(mbid, status = %resp.status(), %api_url, "Wikipedia summary returned non-success");
            return None;
        }

        #[derive(serde::Deserialize)]
        struct WikiSummary {
            extract: Option<String>,
        }

        let summary: WikiSummary = match resp.json().await {
            Ok(s) => s,
            Err(e) => {
                warn!(mbid, error = %e, "Failed to parse Wikipedia summary JSON");
                return None;
            }
        };

        match &summary.extract {
            Some(e) if !e.is_empty() => {
                debug!(mbid, extract_len = e.len(), "Got Wikipedia extract");
                Some(e.clone())
            }
            _ => {
                debug!(mbid, "Wikipedia summary extract was empty");
                None
            }
        }
    }

    /// Resolve a Wikidata QID to a Wikipedia article, then fetch its summary.
    async fn fetch_extract_via_wikidata(&self, qid: &str) -> Option<String> {
        // Wikidata API: get English Wikipedia sitelink for the entity
        let url = format!(
            "https://www.wikidata.org/w/api.php?action=wbgetentities&ids={qid}&props=sitelinks&sitefilter=enwiki&format=json"
        );

        debug!(%qid, "Resolving Wikidata entity to English Wikipedia article");

        let resp = match self
            .http
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!(%qid, error = %e, "Wikidata API request failed");
                return None;
            }
        };

        if !resp.status().is_success() {
            warn!(%qid, status = %resp.status(), "Wikidata API returned non-success");
            return None;
        }

        let body: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                warn!(%qid, error = %e, "Failed to parse Wikidata response");
                return None;
            }
        };

        let title = body
            .get("entities")?
            .get(qid)?
            .get("sitelinks")?
            .get("enwiki")?
            .get("title")?
            .as_str()?;

        debug!(%qid, %title, "Resolved Wikidata to Wikipedia article");

        let wiki_url = format!("https://en.wikipedia.org/wiki/{}", title.replace(' ', "_"));
        self.fetch_wikipedia_summary_from_url(&wiki_url, qid).await
    }
}

fn primary_type_str(pt: &ReleaseGroupPrimaryType) -> &'static str {
    match pt {
        ReleaseGroupPrimaryType::Album => "ALBUM",
        ReleaseGroupPrimaryType::Single => "SINGLE",
        ReleaseGroupPrimaryType::Ep => "EP",
        ReleaseGroupPrimaryType::Broadcast => "BROADCAST",
        ReleaseGroupPrimaryType::Other => "OTHER",
        _ => "OTHER",
    }
}

#[async_trait]
impl MetadataProvider for MusicBrainzProvider {
    fn id(&self) -> &str {
        "musicbrainz"
    }

    fn display_name(&self) -> &str {
        "MusicBrainz"
    }

    async fn search_artists(&self, query: &str) -> Result<Vec<ProviderArtist>, ProviderError> {
        let lucene_query = ArtistSearchQuery::query_builder().artist(query).build();

        let result = MbArtist::search(lucene_query)
            .execute_with_client(&self.client)
            .await
            .map_err(|e| ProviderError::http("musicbrainz", "artist search", e.to_string()))?;

        Ok(result
            .entities
            .into_iter()
            .take(25)
            .map(|a| {
                let url = format!("https://musicbrainz.org/artist/{}", &a.id);
                let disambiguation = if a.disambiguation.is_empty() {
                    None
                } else {
                    Some(a.disambiguation)
                };
                let artist_type = a.artist_type.as_ref().map(|t| format!("{t:?}"));
                let country = a.area.as_ref().map(|area| area.name.clone());
                // Collect top tags by vote count, capped at 5.
                let mut tag_pairs: Vec<(i32, String)> = a
                    .tags
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|t| t.count.unwrap_or(0) > 0)
                    .map(|t| (t.count.unwrap_or(0), t.name))
                    .collect();
                tag_pairs.sort_by(|a, b| b.0.cmp(&a.0));
                let tags: Vec<String> = tag_pairs.into_iter().take(5).map(|(_, n)| n).collect();

                ProviderArtist {
                    external_id: a.id,
                    name: a.name,
                    image_ref: None, // MusicBrainz has no artist images
                    url: Some(url),
                    disambiguation,
                    artist_type,
                    country,
                    tags,
                    popularity: None,
                }
            })
            .collect())
    }

    async fn fetch_albums(
        &self,
        external_artist_id: &str,
    ) -> Result<Vec<ProviderAlbum>, ProviderError> {
        let release_groups = self.browse_all_release_groups(external_artist_id).await?;

        Ok(release_groups
            .into_iter()
            .map(|rg| {
                let album_type = rg
                    .primary_type
                    .as_ref()
                    .map(|pt| primary_type_str(pt).to_string());
                let release_date = rg.first_release_date.map(|d| d.0);
                // Use release group MBID as cover art ref (for Cover Art Archive)
                let cover_ref = Some(rg.id.clone());
                let url = Some(format!("https://musicbrainz.org/release-group/{}", &rg.id));

                ProviderAlbum {
                    external_id: rg.id,
                    title: rg.title,
                    album_type,
                    release_date,
                    cover_ref,
                    url,
                    explicit: false, // MusicBrainz doesn't track explicit content
                    artists: Vec::new(),
                }
            })
            .collect())
    }

    async fn fetch_tracks(
        &self,
        external_album_id: &str,
    ) -> Result<(Vec<ProviderTrack>, HashMap<String, Value>), ProviderError> {
        // external_album_id is a release group MBID
        // Find the best concrete release for this group
        let release = self
            .best_release_for_group(external_album_id)
            .await?
            .ok_or_else(|| ProviderError::not_found("musicbrainz", "release in release group"))?;

        // Now fetch the full release with recordings (which contain ISRCs)
        let full_release = MbRelease::fetch()
            .id(&release.id)
            .with_recordings()
            .with_artist_credits()
            .execute_with_client(&self.client)
            .await
            .map_err(|e| ProviderError::http("musicbrainz", "fetch release", e.to_string()))?;

        let mut tracks = Vec::new();
        let album_extra = HashMap::new();

        if let Some(media) = full_release.media {
            for medium in &media {
                let disc_number = medium.position.unwrap_or(1);
                if let Some(ref medium_tracks) = medium.tracks {
                    for track in medium_tracks {
                        let isrc = track
                            .recording
                            .as_ref()
                            .and_then(|rec| rec.isrcs.as_ref())
                            .and_then(|isrcs| isrcs.first().cloned());

                        let duration_ms = track.length.unwrap_or(0);
                        let duration_secs = duration_ms / 1000;

                        let recording_id = track
                            .recording
                            .as_ref()
                            .map(|r| r.id.clone())
                            .unwrap_or_default();

                        let mut extra = HashMap::new();
                        extra.insert("mb_recording_id".to_string(), Value::String(recording_id));
                        if let Some(ref isrc_val) = isrc {
                            extra.insert("isrc".to_string(), Value::String(isrc_val.clone()));
                        }

                        // Build display-ready artist string from credits (prefer track, fall back to recording)
                        let artists = track
                            .artist_credit
                            .as_ref()
                            .or_else(|| {
                                track
                                    .recording
                                    .as_ref()
                                    .and_then(|r| r.artist_credit.as_ref())
                            })
                            .map(|credits| {
                                let mut s = String::new();
                                for ac in credits {
                                    s.push_str(&ac.name);
                                    if let Some(ref jp) = ac.joinphrase {
                                        s.push_str(jp);
                                    }
                                }
                                s.trim().to_string()
                            })
                            .filter(|s| !s.is_empty());

                        tracks.push(ProviderTrack {
                            external_id: track.id.clone(),
                            title: track.title.clone(),
                            version: None,
                            track_number: track.position,
                            disc_number: Some(disc_number),
                            duration_secs,
                            isrc,
                            artists,
                            explicit: false,
                            extra,
                        });
                    }
                }
            }
        }

        Ok((tracks, album_extra))
    }

    async fn fetch_track_info_extra(
        &self,
        _external_track_id: &str,
    ) -> Option<HashMap<String, Value>> {
        // Track IDs in MB are release-specific track IDs, not much extra to fetch.
        // ISRCs are already extracted during fetch_tracks via recordings.
        None
    }

    fn validate_image_id(&self, image_id: &str) -> bool {
        // MusicBrainz IDs are UUIDs: 36 chars, hex digits and hyphens
        image_id.len() == 36 && image_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
    }

    fn image_url(&self, image_ref: &str, size: u16) -> String {
        // Cover Art Archive: use release-group front image
        // Valid sizes: 250, 500, 1200. Pick closest.
        let caa_size = if size <= 250 {
            250
        } else if size <= 500 {
            500
        } else {
            1200
        };
        format!("https://coverartarchive.org/release-group/{image_ref}/front-{caa_size}")
    }

    async fn fetch_cover_art_bytes(&self, image_ref: &str) -> Option<Vec<u8>> {
        let url = format!("https://coverartarchive.org/release-group/{image_ref}/front-1200");
        let resp = self
            .http
            .get(&url)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            warn!(
                status = %resp.status(),
                image_ref,
                "Cover Art Archive returned non-success for release-group"
            );
            return None;
        }

        resp.bytes().await.ok().map(|b| b.to_vec())
    }

    async fn fetch_artist_bio(&self, external_artist_id: &str) -> Option<String> {
        self.fetch_wikipedia_extract(external_artist_id).await
    }

    async fn search_albums(&self, query: &str) -> Result<Vec<ProviderSearchAlbum>, ProviderError> {
        let lucene_query = ReleaseGroupSearchQuery::query_builder()
            .release_group(query)
            .build();

        let result = ReleaseGroup::search(lucene_query)
            .execute_with_client(&self.client)
            .await
            .map_err(|e| {
                ProviderError::http("musicbrainz", "release group search", e.to_string())
            })?;

        Ok(result
            .entities
            .into_iter()
            .take(25)
            .map(|rg| {
                let album_type = rg
                    .primary_type
                    .as_ref()
                    .map(|pt| primary_type_str(pt).to_string());

                let release_date = rg.first_release_date.map(|d| d.0);

                let cover_ref = Some(rg.id.clone());
                let url = Some(format!("https://musicbrainz.org/release-group/{}", &rg.id));

                // Extract primary artist from artist credits.
                let (artist_name, artist_external_id) = rg
                    .artist_credit
                    .as_ref()
                    .and_then(|credits| credits.first())
                    .map(|ac| (ac.name.clone(), ac.artist.id.clone()))
                    .unwrap_or_else(|| (yoink_shared::UNKNOWN_ARTIST.to_string(), String::new()));

                ProviderSearchAlbum {
                    external_id: rg.id,
                    title: rg.title,
                    album_type,
                    release_date,
                    cover_ref,
                    url,
                    explicit: false,
                    artist_name,
                    artist_external_id,
                }
            })
            .collect())
    }
}
