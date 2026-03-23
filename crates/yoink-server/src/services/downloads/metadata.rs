use std::collections::{HashMap, HashSet};
use std::path::Path;

use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{MimeType, Picture, PictureType},
    prelude::{Accessor, ItemKey},
    probe::Probe,
    tag::{Tag, TagType},
};
use serde_json::Value;

use crate::error::{AppError, AppResult};

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
    pub track_extra: &'a HashMap<String, Value>,
    pub album_extra: &'a HashMap<String, Value>,
    pub track_info_extra: Option<&'a HashMap<String, Value>>,
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

    if let Some(info) = meta.track_info_extra {
        if let Some(isrc) = value_as_string(info.get("isrc")) {
            tag.insert_text(ItemKey::Isrc, isrc);
        }
        if let Some(copyright) = value_as_string(info.get("copyright")) {
            tag.insert_text(ItemKey::CopyrightMessage, copyright);
        }
        if let Some(version) = value_as_string(info.get("version"))
            && !version.trim().is_empty()
        {
            tag.insert_text(ItemKey::TrackSubtitle, version);
        }
        if let Some(initial_key) = value_as_string(info.get("key")) {
            tag.insert_text(ItemKey::InitialKey, initial_key);
        }
        if let Some(bpm) = value_as_string(info.get("bpm")) {
            tag.insert_text(ItemKey::IntegerBpm, bpm);
        }
        if let Some(track_gain) = value_as_string(info.get("trackReplayGain")) {
            tag.insert_text(ItemKey::ReplayGainTrackGain, track_gain);
        }
        if let Some(track_peak) = value_as_string(info.get("trackPeakAmplitude")) {
            tag.insert_text(ItemKey::ReplayGainTrackPeak, track_peak);
        }
        if let Some(album_gain) = value_as_string(info.get("albumReplayGain")) {
            tag.insert_text(ItemKey::ReplayGainAlbumGain, album_gain);
        }
        if let Some(album_peak) = value_as_string(info.get("albumPeakAmplitude")) {
            tag.insert_text(ItemKey::ReplayGainAlbumPeak, album_peak);
        }
    }

    if let Some(jpeg) = meta.cover_art_jpeg {
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(Picture::new_unchecked(
            PictureType::CoverFront,
            Some(MimeType::Jpeg),
            None,
            jpeg.to_vec(),
        ));
    }

    if tag_type == TagType::VorbisComments {
        write_extra_vorbis(tag, "TIDAL_TRACK_", meta.track_extra);
        write_extra_vorbis(tag, "TIDAL_ALBUM_", meta.album_extra);
        if let Some(info) = meta.track_info_extra {
            write_extra_vorbis(tag, "TIDAL_INFO_", info);
        }
    }

    tagged_file
        .save_to_path(meta.path, WriteOptions::default())
        .map_err(|err| AppError::metadata("save metadata tags", err.to_string()))?;
    Ok(())
}

fn preferred_tag_type(_meta: &TrackMetadata<'_>, tagged_file: &impl TaggedFileExt) -> TagType {
    tagged_file.primary_tag_type()
}

fn write_extra_vorbis(tag: &mut Tag, prefix: &str, extra: &HashMap<String, Value>) {
    for (key, value) in extra {
        if let Some(text) = value_to_text(value) {
            let key = sanitize_vorbis_key(prefix, key);
            if !key.is_empty() {
                tag.insert_text(ItemKey::Unknown(key), text);
            }
        }
    }
}

fn value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).ok(),
    }
}

fn value_as_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    }
}

pub(crate) fn build_full_artist_string(
    title: &str,
    track_extra: &HashMap<String, Value>,
    track_info_extra: Option<&HashMap<String, Value>>,
    fallback_artist: &str,
) -> String {
    let mut artists = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    {
        let mut push_artist = |name: &str| push_unique_artist(name, &mut artists, &mut seen);

        collect_artists_from_map(track_extra, &mut push_artist);
        if let Some(extra) = track_info_extra {
            collect_artists_from_map(extra, &mut push_artist);
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

fn collect_artists_from_map(map: &HashMap<String, Value>, push: &mut dyn FnMut(&str)) {
    for key in ["artists", "artist"] {
        if let Some(value) = map.get(key) {
            collect_artist_names(value, push);
        }
    }
}

fn collect_artist_names(value: &Value, push: &mut dyn FnMut(&str)) {
    match value {
        Value::String(s) => push(s),
        Value::Array(items) => {
            for item in items {
                collect_artist_names(item, push);
            }
        }
        Value::Object(obj) => {
            if let Some(Value::String(name)) = obj.get("name") {
                push(name);
            } else if let Some(Value::String(name)) = obj.get("title") {
                push(name);
            }
        }
        _ => {}
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

pub(crate) fn extract_disc_number(
    track_extra: &HashMap<String, Value>,
    track_info_extra: Option<&HashMap<String, Value>>,
) -> Option<i32> {
    for key in ["volumeNumber", "discNumber", "volume_number", "disc_number"] {
        if let Some(val) = track_info_extra
            .and_then(|m| m.get(key))
            .or_else(|| track_extra.get(key))
        {
            match val {
                Value::Number(n) => {
                    if let Some(v) = n.as_i64().and_then(|v| i32::try_from(v).ok()) {
                        return Some(v);
                    }
                }
                Value::String(s) => {
                    if let Ok(v) = s.trim().parse::<i32>() {
                        return Some(v);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn sanitize_vorbis_key(prefix: &str, key: &str) -> String {
    let normalized = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    format!("{}{}", prefix, normalized)
}

// fetch_cover_art_bytes and fetch_track_info_extra moved to providers

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::process::Command;

    use lofty::{
        file::{FileType, TaggedFile, TaggedFileExt},
        probe::Probe,
        properties::FileProperties,
        tag::TagType,
    };
    use serde_json::{Value, json};

    use super::*;

    fn test_meta<'a>(path: &'a Path, cover_art_jpeg: Option<&'a [u8]>) -> TrackMetadata<'a> {
        let track_extra = Box::leak(Box::new(HashMap::new()));
        let album_extra = Box::leak(Box::new(HashMap::new()));
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
            track_extra,
            album_extra,
            track_info_extra: None,
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
    fn build_artist_from_track_extra_array_of_objects() {
        let mut track_extra = HashMap::new();
        track_extra.insert(
            "artists".to_string(),
            json!([{"name": "Artist A"}, {"name": "Artist B"}]),
        );
        let result = build_full_artist_string("Song", &track_extra, None, "Fallback");
        assert_eq!(result, "Artist A; Artist B");
    }

    #[test]
    fn build_artist_from_track_extra_string() {
        let mut track_extra = HashMap::new();
        track_extra.insert("artist".to_string(), json!("Solo Artist"));
        let result = build_full_artist_string("Song", &track_extra, None, "Fallback");
        assert_eq!(result, "Solo Artist");
    }

    #[test]
    fn build_artist_deduplicates_case_insensitive() {
        let mut track_extra = HashMap::new();
        track_extra.insert(
            "artists".to_string(),
            json!([{"name": "Artist"}, {"name": "artist"}]),
        );
        let result = build_full_artist_string("Song", &track_extra, None, "Fallback");
        assert_eq!(result, "Artist");
    }

    #[test]
    fn build_artist_falls_back_when_no_extra() {
        let track_extra = HashMap::new();
        let result = build_full_artist_string(
            "Song Without Featured",
            &track_extra,
            None,
            "Fallback Artist",
        );
        assert_eq!(result, "Fallback Artist");
    }

    #[test]
    fn build_artist_merges_featured_from_title() {
        let mut track_extra = HashMap::new();
        track_extra.insert("artists".to_string(), json!([{"name": "Main Artist"}]));
        let result =
            build_full_artist_string("Song (feat. Featured One)", &track_extra, None, "Fallback");
        assert_eq!(result, "Main Artist; Featured One");
    }

    #[test]
    fn build_artist_deduplicates_featured_with_extra() {
        let mut track_extra = HashMap::new();
        track_extra.insert(
            "artists".to_string(),
            json!([{"name": "Main"}, {"name": "Featured"}]),
        );
        // Featured is already in extra, so title feat should not add duplicate
        let result =
            build_full_artist_string("Song (feat. Featured)", &track_extra, None, "Fallback");
        assert_eq!(result, "Main; Featured");
    }

    #[test]
    fn build_artist_merges_track_info_extra() {
        let track_extra = HashMap::new();
        let mut info_extra = HashMap::new();
        info_extra.insert("artists".to_string(), json!([{"name": "Info Artist"}]));
        let result = build_full_artist_string("Song", &track_extra, Some(&info_extra), "Fallback");
        assert_eq!(result, "Info Artist");
    }

    #[test]
    fn build_artist_title_key_fallback() {
        let mut track_extra = HashMap::new();
        track_extra.insert(
            "artists".to_string(),
            json!([{"title": "Artist Via Title Key"}]),
        );
        let result = build_full_artist_string("Song", &track_extra, None, "Fallback");
        assert_eq!(result, "Artist Via Title Key");
    }

    // ── extract_disc_number ─────────────────────────────────────

    #[test]
    fn extract_disc_from_info_extra_number() {
        let track_extra = HashMap::new();
        let mut info_extra = HashMap::new();
        info_extra.insert("volumeNumber".to_string(), json!(2));
        assert_eq!(
            extract_disc_number(&track_extra, Some(&info_extra)),
            Some(2)
        );
    }

    #[test]
    fn extract_disc_from_track_extra_string() {
        let mut track_extra = HashMap::new();
        track_extra.insert("discNumber".to_string(), json!("3"));
        assert_eq!(extract_disc_number(&track_extra, None), Some(3));
    }

    #[test]
    fn extract_disc_from_info_extra_takes_precedence() {
        let mut track_extra = HashMap::new();
        track_extra.insert("volumeNumber".to_string(), json!(5));
        let mut info_extra = HashMap::new();
        info_extra.insert("volumeNumber".to_string(), json!(1));
        // info_extra is checked first for each key
        assert_eq!(
            extract_disc_number(&track_extra, Some(&info_extra)),
            Some(1)
        );
    }

    #[test]
    fn extract_disc_none_when_missing() {
        let track_extra = HashMap::new();
        assert_eq!(extract_disc_number(&track_extra, None), None);
    }

    #[test]
    fn extract_disc_snake_case_key() {
        let mut track_extra = HashMap::new();
        track_extra.insert("disc_number".to_string(), json!(4));
        assert_eq!(extract_disc_number(&track_extra, None), Some(4));
    }

    // ── value_to_text ───────────────────────────────────────────

    #[test]
    fn value_to_text_string() {
        assert_eq!(value_to_text(&json!("hello")), Some("hello".to_string()));
    }

    #[test]
    fn value_to_text_number() {
        assert_eq!(value_to_text(&json!(42)), Some("42".to_string()));
    }

    #[test]
    fn value_to_text_bool() {
        assert_eq!(value_to_text(&json!(true)), Some("true".to_string()));
    }

    #[test]
    fn value_to_text_null() {
        assert_eq!(value_to_text(&Value::Null), None);
    }

    #[test]
    fn value_to_text_array() {
        let result = value_to_text(&json!([1, 2, 3]));
        assert!(result.is_some());
        assert!(result.unwrap().contains("[1,2,3]"));
    }

    // ── value_as_string ─────────────────────────────────────────

    #[test]
    fn value_as_string_from_string() {
        assert_eq!(
            value_as_string(Some(&json!("hello"))),
            Some("hello".to_string())
        );
    }

    #[test]
    fn value_as_string_from_number() {
        assert_eq!(value_as_string(Some(&json!(42))), Some("42".to_string()));
    }

    #[test]
    fn value_as_string_from_null() {
        assert_eq!(value_as_string(Some(&Value::Null)), None);
    }

    #[test]
    fn value_as_string_from_none() {
        assert_eq!(value_as_string(None), None);
    }

    // ── sanitize_vorbis_key ─────────────────────────────────────

    #[test]
    fn sanitize_vorbis_key_uppercase_and_prefix() {
        assert_eq!(
            sanitize_vorbis_key("TIDAL_TRACK_", "artistName"),
            "TIDAL_TRACK_ARTISTNAME"
        );
    }

    #[test]
    fn sanitize_vorbis_key_non_alphanumeric_replaced() {
        assert_eq!(
            sanitize_vorbis_key("PREFIX_", "some-key.here"),
            "PREFIX_SOME_KEY_HERE"
        );
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
