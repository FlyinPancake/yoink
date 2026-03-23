use std::path::Path;
use std::time::Duration;

use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::config::DOWNLOAD_CHUNK_SIZE;
use crate::error::{AppError, AppResult};

pub(crate) enum DownloadPayload {
    DirectUrl(String),
    DashSegmentUrls(Vec<String>),
}

pub(crate) async fn download_payload_to_file(
    http: &reqwest::Client,
    payload: &DownloadPayload,
    path: &Path,
) -> AppResult<()> {
    match payload {
        DownloadPayload::DirectUrl(url) => download_to_file(http, url, path).await,
        DownloadPayload::DashSegmentUrls(urls) => {
            download_dash_segments_to_file(http, urls, path).await
        }
    }
}

async fn download_to_file(http: &reqwest::Client, url: &str, path: &Path) -> AppResult<()> {
    let mut response = http
        .get(url)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|err| AppError::http("direct download request", err))?
        .error_for_status()
        .map_err(|err| AppError::http("direct download status", err))?;

    let mut file = fs::File::create(path)
        .await
        .map_err(|err| AppError::filesystem("create file", path.display().to_string(), err))?;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| AppError::http("read response chunk", err))?
    {
        if chunk.is_empty() {
            continue;
        }
        for slice in chunk.chunks(DOWNLOAD_CHUNK_SIZE) {
            file.write_all(slice).await.map_err(|err| {
                AppError::filesystem("write file", path.display().to_string(), err)
            })?;
        }
    }

    file.flush()
        .await
        .map_err(|err| AppError::filesystem("flush file", path.display().to_string(), err))?;
    Ok(())
}

async fn download_dash_segments_to_file(
    http: &reqwest::Client,
    urls: &[String],
    path: &Path,
) -> AppResult<()> {
    let mut file = fs::File::create(path)
        .await
        .map_err(|err| AppError::filesystem("create file", path.display().to_string(), err))?;

    for (idx, url) in urls.iter().enumerate() {
        let bytes = http
            .get(url)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|err| AppError::http(format!("dash segment request #{idx}"), err))?
            .error_for_status()
            .map_err(|err| AppError::http(format!("dash segment status #{idx}"), err))?
            .bytes()
            .await
            .map_err(|err| AppError::http(format!("dash segment body #{idx}"), err))?;

        if bytes.is_empty() {
            continue;
        }
        for slice in bytes.chunks(DOWNLOAD_CHUNK_SIZE) {
            file.write_all(slice).await.map_err(|err| {
                AppError::filesystem(
                    format!("write dash segment #{idx}"),
                    path.display().to_string(),
                    err,
                )
            })?;
        }
    }

    file.flush()
        .await
        .map_err(|err| AppError::filesystem("flush file", path.display().to_string(), err))?;
    Ok(())
}

pub(crate) async fn has_flac_stream_marker(path: &Path) -> AppResult<bool> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|err| AppError::filesystem("open file", path.display().to_string(), err))?;
    let mut header = [0u8; 4];
    let read = file
        .read(&mut header)
        .await
        .map_err(|err| AppError::filesystem("read header", path.display().to_string(), err))?;
    Ok(read == 4 && header == *b"fLaC")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaContainer {
    Flac,
    Mp4,
    Ogg,
    Wav,
    Mp3,
    Aac,
    Unknown,
}

impl MediaContainer {
    pub fn ext(&self) -> Option<&'static str> {
        match self {
            MediaContainer::Flac => Some("flac"),
            MediaContainer::Mp4 => Some("m4a"),
            MediaContainer::Ogg => Some("ogg"),
            MediaContainer::Wav => Some("wav"),
            MediaContainer::Mp3 => Some("mp3"),
            MediaContainer::Aac => Some("aac"),
            MediaContainer::Unknown => None,
        }
    }
}

pub(crate) async fn sniff_media_container(path: &Path) -> AppResult<MediaContainer> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|err| AppError::filesystem("open file", path.display().to_string(), err))?;
    let mut header = [0u8; 16];
    let read = file
        .read(&mut header)
        .await
        .map_err(|err| AppError::filesystem("read header", path.display().to_string(), err))?;
    if read >= 4 && header[..4] == *b"fLaC" {
        return Ok(MediaContainer::Flac);
    }
    if read >= 8 && header[4..8] == *b"ftyp" {
        return Ok(MediaContainer::Mp4);
    }
    if read >= 4 && header[..4] == *b"OggS" {
        return Ok(MediaContainer::Ogg);
    }
    if read >= 12 && header[..4] == *b"RIFF" && header[8..12] == *b"WAVE" {
        return Ok(MediaContainer::Wav);
    }
    if read >= 3 && header[..3] == *b"ID3" {
        return Ok(MediaContainer::Mp3);
    }
    if read >= 2 && header[0] == 0xFF && (header[1] & 0xE0) == 0xE0 {
        if read >= 3 && (header[1] & 0x16) == 0x10 {
            return Ok(MediaContainer::Aac);
        }
        return Ok(MediaContainer::Mp3);
    }
    Ok(MediaContainer::Unknown)
}

pub(crate) fn sanitize_path_component(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string();

    if sanitized.is_empty() {
        "Unknown".to_string()
    } else {
        sanitized
    }
}

pub(crate) fn parse_track_number_from_path(path: &Path) -> Option<u32> {
    let stem = path.file_stem()?.to_str()?.trim();
    let digits = stem
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

pub(crate) fn extract_year(release_date: &str) -> String {
    let year = release_date.chars().take(4).collect::<String>();
    if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) {
        year
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    // ── sanitize_path_component ─────────────────────────────────

    #[test]
    fn sanitize_normal_text_unchanged() {
        assert_eq!(sanitize_path_component("Hello World"), "Hello World");
    }

    #[test]
    fn sanitize_replaces_path_separators() {
        assert_eq!(sanitize_path_component("AC/DC"), "AC_DC");
        assert_eq!(sanitize_path_component("back\\slash"), "back_slash");
    }

    #[test]
    fn sanitize_replaces_special_chars() {
        assert_eq!(sanitize_path_component("file:name"), "file_name");
        assert_eq!(sanitize_path_component("what*ever"), "what_ever");
        assert_eq!(sanitize_path_component("who?"), "who_");
        assert_eq!(sanitize_path_component("say\"what"), "say_what");
        assert_eq!(sanitize_path_component("<tag>"), "_tag_");
        assert_eq!(sanitize_path_component("pipe|char"), "pipe_char");
    }

    #[test]
    fn sanitize_replaces_control_chars() {
        assert_eq!(sanitize_path_component("hello\x00world"), "hello_world");
        assert_eq!(sanitize_path_component("tab\there"), "tab_here");
    }

    #[test]
    fn sanitize_trims_whitespace() {
        assert_eq!(sanitize_path_component("  spaced  "), "spaced");
    }

    #[test]
    fn sanitize_empty_becomes_unknown() {
        assert_eq!(sanitize_path_component(""), "Unknown");
    }

    #[test]
    fn sanitize_only_whitespace_becomes_unknown() {
        assert_eq!(sanitize_path_component("   "), "Unknown");
    }

    #[test]
    fn sanitize_mixed_input() {
        assert_eq!(
            sanitize_path_component("Artist: The Best Of (2024)"),
            "Artist_ The Best Of (2024)"
        );
    }

    #[test]
    fn sanitize_unicode_preserved() {
        assert_eq!(sanitize_path_component("Bjork"), "Bjork");
        assert_eq!(sanitize_path_component("Sigur Ros"), "Sigur Ros");
    }

    // ── parse_track_number_from_path ────────────────────────────

    #[test]
    fn parse_track_number_leading_digits() {
        assert_eq!(
            parse_track_number_from_path(Path::new("01 Song Title.flac")),
            Some(1)
        );
        assert_eq!(
            parse_track_number_from_path(Path::new("12 Another Song.m4a")),
            Some(12)
        );
    }

    #[test]
    fn parse_track_number_three_digits() {
        assert_eq!(
            parse_track_number_from_path(Path::new("101 Long Album.flac")),
            Some(101)
        );
    }

    #[test]
    fn parse_track_number_no_digits() {
        assert_eq!(
            parse_track_number_from_path(Path::new("Song Title.flac")),
            None
        );
    }

    #[test]
    fn parse_track_number_in_subdirectory() {
        assert_eq!(
            parse_track_number_from_path(Path::new("/music/artist/album/03 Track.flac")),
            Some(3)
        );
    }

    // ── extract_year ────────────────────────────────────────────

    #[test]
    fn extract_year_full_date() {
        assert_eq!(extract_year("2024-01-15"), "2024");
    }

    #[test]
    fn extract_year_just_year() {
        assert_eq!(extract_year("2024"), "2024");
    }

    #[test]
    fn extract_year_non_digit() {
        assert_eq!(extract_year("abcd-01-01"), "");
    }

    #[test]
    fn extract_year_too_short() {
        assert_eq!(extract_year("20"), "");
    }

    #[test]
    fn extract_year_empty() {
        assert_eq!(extract_year(""), "");
    }

    #[test]
    fn extract_year_mixed_prefix() {
        // "v202" has a non-digit 'v' at position 0
        assert_eq!(extract_year("v202"), "");
    }

    #[tokio::test]
    async fn sniff_media_container_detects_mp3_id3() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"ID3\x04\x00\x00\x00\x00\x00\x21")
            .await
            .unwrap();
        assert_eq!(
            sniff_media_container(file.path()).await.unwrap().ext(),
            Some("mp3")
        );
    }

    #[tokio::test]
    async fn sniff_media_container_detects_wav() {
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"RIFF\x24\x80\0\0WAVEfmt ")
            .await
            .unwrap();
        assert_eq!(
            sniff_media_container(file.path()).await.unwrap().ext(),
            Some("wav")
        );
    }
}
