use std::collections::HashSet;
use std::path::Path;

use crate::{
    error::{AppError, AppResult},
    providers::ProviderTrackMetadata,
};
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{MimeType, Picture, PictureType},
    prelude::{Accessor, ItemKey},
    probe::Probe,
    tag::{Tag, TagType},
};

use super::io::extract_year;

/// All the metadata needed to tag a single audio file.
pub(crate) struct TrackMetadata<'a> {
    pub path: &'a Path,
    pub title: &'a str,
    pub track_artist: &'a str,
    pub album_artist: &'a str,
    pub album: &'a str,
    pub track_number: u32,
    pub disc_number: Option<u32>,
    pub total_tracks: u32,
    pub release_date: &'a str,
    pub provider_metadata: Option<&'a ProviderTrackMetadata>,
    pub lyrics_text: Option<&'a str>,
    pub cover_art_jpeg: Option<&'a [u8]>,
}

pub(crate) fn write_audio_metadata(meta: &TrackMetadata<'_>) -> AppResult<()> {
    let mut tagged_file = Probe::open(meta.path)
        .map_err(|err| AppError::metadata("open tagged file", err.to_string()))?
        .read()
        .map_err(|err| AppError::metadata("read tagged file", err.to_string()))?;

    let tag_type = preferred_tag_type(meta, &tagged_file);
    let tag = if let Some(existing) = tagged_file.tag_mut(tag_type) {
        existing
    } else {
        tagged_file.insert_tag(Tag::new(tag_type));
        tagged_file
            .tag_mut(tag_type)
            .ok_or_else(|| AppError::metadata("create metadata tag", "missing target tag"))?
    };

    tag.set_title(meta.title.to_string());
    tag.set_artist(meta.track_artist.to_string());
    tag.set_album(meta.album.to_string());
    if !meta.album_artist.trim().is_empty() {
        tag.insert_text(ItemKey::AlbumArtist, meta.album_artist.to_string());
    }
    tag.insert_text(ItemKey::TrackNumber, meta.track_number.to_string());
    if let Some(disc) = meta.disc_number {
        tag.insert_text(ItemKey::DiscNumber, disc.to_string());
    }
    if meta.total_tracks > 0 {
        tag.insert_text(ItemKey::TrackTotal, meta.total_tracks.to_string());
    }
    let year = extract_year(meta.release_date);
    if !year.is_empty() {
        tag.insert_text(ItemKey::Year, year);
    }
    if let Some(lyrics) = meta.lyrics_text.filter(|v| !v.trim().is_empty()) {
        tag.insert_text(ItemKey::Lyrics, lyrics.to_string());
    }

    if let Some(info) = meta.provider_metadata {
        if let Some(isrc) = &info.isrc {
            tag.insert_text(ItemKey::Isrc, isrc.clone());
        }
        if let Some(copyright) = &info.copyright {
            tag.insert_text(ItemKey::CopyrightMessage, copyright.clone());
        }
        if let Some(version) = &info.version
            && !version.trim().is_empty()
        {
            tag.insert_text(ItemKey::TrackSubtitle, version.clone());
        }
        if let Some(initial_key) = &info.initial_key {
            tag.insert_text(ItemKey::InitialKey, initial_key.clone());
        }
        if let Some(bpm) = info.bpm {
            tag.insert_text(ItemKey::IntegerBpm, bpm.to_string());
        }
        if let Some(track_gain) = info.track_replay_gain {
            tag.insert_text(ItemKey::ReplayGainTrackGain, track_gain.to_string());
        }
        if let Some(track_peak) = info.track_peak_amplitude {
            tag.insert_text(ItemKey::ReplayGainTrackPeak, track_peak.to_string());
        }
    }

    if let Some(jpeg) = meta.cover_art_jpeg {
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(
            Picture::unchecked(jpeg.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Jpeg)
                .build(),
        );
    }

    tagged_file
        .save_to_path(meta.path, WriteOptions::default())
        .map_err(|err| AppError::metadata("save metadata tags", err.to_string()))?;
    Ok(())
}

fn preferred_tag_type(_meta: &TrackMetadata<'_>, tagged_file: &impl TaggedFileExt) -> TagType {
    tagged_file.primary_tag_type()
}

pub(crate) fn build_full_artist_string(
    title: &str,
    provider_metadata: Option<&ProviderTrackMetadata>,
    fallback_artist: &str,
) -> String {
    let mut artists = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    {
        let mut push_artist = |name: &str| push_unique_artist(name, &mut artists, &mut seen);

        if let Some(metadata) = provider_metadata {
            for artist in &metadata.artists {
                push_artist(artist);
            }
        }
        for featured in parse_featured_artists(title) {
            push_artist(&featured);
        }
    }

    if artists.is_empty() {
        push_unique_artist(fallback_artist, &mut artists, &mut seen);
    }

    artists.join("; ")
}

fn push_unique_artist(name: &str, artists: &mut Vec<String>, seen: &mut HashSet<String>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    let key = trimmed.to_ascii_lowercase();
    if seen.insert(key) {
        artists.push(trimmed.to_string());
    }
}

fn parse_featured_artists(title: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let lower = title.to_ascii_lowercase();
    while let Some(open_rel) = lower[start..].find('(') {
        let open = start + open_rel;
        let Some(close_rel) = lower[open + 1..].find(')') else {
            break;
        };
        let close = open + 1 + close_rel;
        let inner = title[open + 1..close].trim();
        let inner_lower = inner.to_ascii_lowercase();
        let markers = ["feat.", "feat", "ft.", "ft", "with "];
        if let Some(marker) = markers.iter().find(|m| inner_lower.starts_with(**m)) {
            let raw = inner[marker.len()..].trim();
            for piece in raw.split(',') {
                for p in piece.split('&') {
                    let name = p.trim();
                    if !name.is_empty() {
                        out.push(name.to_string());
                    }
                }
            }
        }
        start = close + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use super::*;
    use lofty::{
        file::{FileType, TaggedFile, TaggedFileExt},
        probe::Probe,
        properties::FileProperties,
        tag::TagType,
    };

    fn test_meta<'a>(path: &'a Path, cover_art_jpeg: Option<&'a [u8]>) -> TrackMetadata<'a> {
        TrackMetadata {
            path,
            title: "Track",
            track_artist: "Artist",
            album_artist: "Artist",
            album: "Album",
            track_number: 1,
            disc_number: Some(1),
            total_tracks: 1,
            release_date: "2024-01-01",
            provider_metadata: None,
            lyrics_text: None,
            cover_art_jpeg,
        }
    }

    fn tagged_file(file_type: FileType) -> TaggedFile {
        TaggedFile::new(file_type, FileProperties::default(), Vec::new())
    }

    // ── parse_featured_artists ──────────────────────────────────

    #[test]
    fn parse_featured_single_artist() {
        assert_eq!(
            parse_featured_artists("Song (feat. Artist)"),
            vec!["Artist"]
        );
    }

    #[test]
    fn parse_featured_ft_dot() {
        assert_eq!(
            parse_featured_artists("Song (ft. Someone)"),
            vec!["Someone"]
        );
    }

    #[test]
    fn parse_featured_multiple_comma_and_ampersand() {
        assert_eq!(
            parse_featured_artists("Song (feat. A, B & C)"),
            vec!["A", "B", "C"]
        );
    }

    #[test]
    fn parse_featured_no_parens() {
        let result = parse_featured_artists("Just A Regular Title");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_featured_non_feat_parens() {
        // "(Deluxe Edition)" should not be parsed as featuring
        let result = parse_featured_artists("Album (Deluxe Edition)");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_featured_with_marker() {
        assert_eq!(
            parse_featured_artists("Song (with Someone)"),
            vec!["Someone"]
        );
    }

    #[test]
    fn parse_featured_feat_no_dot() {
        assert_eq!(parse_featured_artists("Song (feat Artist)"), vec!["Artist"]);
    }

    #[test]
    fn parse_featured_multiple_parens() {
        let result = parse_featured_artists("Song (Remix) (feat. A) (Live)");
        assert_eq!(result, vec!["A"]);
    }

    #[test]
    fn parse_featured_unclosed_paren() {
        let result = parse_featured_artists("Song (feat. Artist");
        assert!(result.is_empty());
    }

    // ── build_full_artist_string ────────────────────────────────

    #[test]
    fn build_artist_from_provider_metadata() {
        let metadata = ProviderTrackMetadata {
            artists: vec!["Artist A".to_string(), "Artist B".to_string()],
            ..Default::default()
        };
        let result = build_full_artist_string("Song", Some(&metadata), "Fallback");
        assert_eq!(result, "Artist A; Artist B");
    }

    #[test]
    fn build_artist_from_single_provider_artist() {
        let metadata = ProviderTrackMetadata {
            artists: vec!["Solo Artist".to_string()],
            ..Default::default()
        };
        let result = build_full_artist_string("Song", Some(&metadata), "Fallback");
        assert_eq!(result, "Solo Artist");
    }

    #[test]
    fn build_artist_deduplicates_case_insensitive() {
        let metadata = ProviderTrackMetadata {
            artists: vec!["Artist".to_string(), "artist".to_string()],
            ..Default::default()
        };
        let result = build_full_artist_string("Song", Some(&metadata), "Fallback");
        assert_eq!(result, "Artist");
    }

    #[test]
    fn build_artist_falls_back_without_provider_metadata() {
        let result = build_full_artist_string("Song Without Featured", None, "Fallback Artist");
        assert_eq!(result, "Fallback Artist");
    }

    #[test]
    fn build_artist_merges_featured_from_title() {
        let metadata = ProviderTrackMetadata {
            artists: vec!["Main Artist".to_string()],
            ..Default::default()
        };
        let result =
            build_full_artist_string("Song (feat. Featured One)", Some(&metadata), "Fallback");
        assert_eq!(result, "Main Artist; Featured One");
    }

    #[test]
    fn build_artist_deduplicates_featured_with_provider_metadata() {
        let metadata = ProviderTrackMetadata {
            artists: vec!["Main".to_string(), "Featured".to_string()],
            ..Default::default()
        };
        let result = build_full_artist_string("Song (feat. Featured)", Some(&metadata), "Fallback");
        assert_eq!(result, "Main; Featured");
    }

    #[test]
    fn preferred_tag_type_uses_wav_primary_id3v2() {
        let meta = test_meta(Path::new("track.wav"), Some(b"jpeg"));
        let tagged_file = tagged_file(FileType::Wav);
        assert_eq!(preferred_tag_type(&meta, &tagged_file), TagType::Id3v2);
    }

    #[test]
    fn preferred_tag_type_uses_mp4_primary_tag() {
        let meta = test_meta(Path::new("track.m4a"), Some(b"jpeg"));
        let tagged_file = tagged_file(FileType::Mp4);
        assert_eq!(preferred_tag_type(&meta, &tagged_file), TagType::Mp4Ilst);
    }

    #[test]
    #[ignore = "developer regression test; requires ffmpeg"]
    fn writes_cover_art_to_common_containers() {
        for extension in ["mp3", "m4a", "flac"] {
            let temp = tempfile::tempdir().unwrap();
            let audio_path = temp.path().join(format!("sample.{extension}"));

            let status = Command::new("ffmpeg")
                .args([
                    "-f",
                    "lavfi",
                    "-i",
                    "anullsrc=r=44100:cl=stereo",
                    "-t",
                    "1",
                    "-y",
                ])
                .arg(&audio_path)
                .status()
                .unwrap();
            assert!(status.success(), "ffmpeg failed for {extension}");

            let cover = vec![
                0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00,
                0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
            ];
            let meta = test_meta(&audio_path, Some(&cover));
            write_audio_metadata(&meta).unwrap();

            let tagged = Probe::open(&audio_path).unwrap().read().unwrap();
            let tag = tagged.primary_tag().expect("missing primary tag");
            assert!(
                !tag.pictures().is_empty(),
                "missing cover art for {extension}"
            );
        }
    }
}
