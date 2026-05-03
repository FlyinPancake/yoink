//! Manifest decoding for Tidal playback responses.
//!
//! Tidal returns playback information as a base64-encoded manifest whose
//! format is indicated by a MIME type. Two formats are supported:
//!
//! * **BTS** (`application/vnd.tidal.bts`) – a JSON envelope containing
//!   one or more direct download URLs.
//! * **DASH** (`application/dash+xml`) – an MPEG-DASH MPD document from
//!   which segment or base URLs are extracted.

use base64::Engine;
use roxmltree::{Document, Node};
use snafu::prelude::*;
use tracing::warn;

use super::models::{BtsManifest, HifiPlaybackData};
use crate::providers::{PlaybackInfo, ProviderError};

#[derive(Debug, Snafu)]
enum ManifestError {
    #[snafu(display("failed to decode manifest: {source}"))]
    Decode { source: base64::DecodeError },
    #[snafu(display("failed to parse BTS manifest: {source}"))]
    ParseBts { source: serde_json::Error },
    #[snafu(display("DASH manifest is not valid UTF-8: {source}"))]
    Utf8 { source: std::string::FromUtf8Error },
    #[snafu(display("unsupported manifest type '{mime_type}'"))]
    UnsupportedType { mime_type: String },
    #[snafu(display("{error}"))]
    DashStructure { error: &'static str },
    #[snafu(display("failed to parse DASH XML: {source}"))]
    DashXml { source: roxmltree::Error },
}

impl From<ManifestError> for ProviderError {
    fn from(value: ManifestError) -> Self {
        ProviderError::Parse {
            provider: crate::db::provider::Provider::Tidal,
            operation: "manifest".to_string(),
            reason: value.to_string(),
        }
    }
}

/// Decode the base64 manifest from a playback response and extract a
/// [`PlaybackInfo`] containing the download URL(s).
///
/// Returns an error when the manifest cannot be decoded, parsed,
/// or contains no usable URLs.
pub(crate) fn extract_download_payload(
    playback: &HifiPlaybackData,
) -> Result<PlaybackInfo, ProviderError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(playback.manifest.as_bytes())
        .context(DecodeSnafu)?;

    match playback.manifest_mime_type.as_str() {
        "application/vnd.tidal.bts" => {
            let manifest =
                serde_json::from_slice::<BtsManifest>(&decoded).context(ParseBtsSnafu)?;
            manifest
                .urls
                .first()
                .cloned()
                .map(PlaybackInfo::DirectUrl)
                .context(DashStructureSnafu {
                    error: "no track URL in BTS manifest",
                })
                .map_err(Into::into)
        }
        "application/dash+xml" => {
            let xml = String::from_utf8(decoded).context(Utf8Snafu)?;
            if let Ok(urls) = extract_dash_segment_urls(&xml)
                && !urls.is_empty()
            {
                return Ok(PlaybackInfo::SegmentUrls(urls));
            }
            extract_dash_base_url(&xml)
                .map(PlaybackInfo::DirectUrl)
                .map_err(ProviderError::from)
        }
        other => {
            warn!(manifest_mime_type = %other, "Unknown manifest type, attempting BTS parse as fallback");
            let manifest = serde_json::from_slice::<BtsManifest>(&decoded).map_err(|_| {
                ManifestError::UnsupportedType {
                    mime_type: other.to_string(),
                }
            })?;
            manifest
                .urls
                .first()
                .cloned()
                .map(PlaybackInfo::DirectUrl)
                .context(DashStructureSnafu {
                    error: "no track URL in fallback BTS manifest",
                })
                .map_err(Into::into)
        }
    }
}

/// Parse a raw DASH MPD document into downloadable segment URLs.
pub(crate) fn extract_dash_download_payload(xml: &str) -> Result<PlaybackInfo, ProviderError> {
    if let Ok(urls) = extract_dash_segment_urls(xml)
        && !urls.is_empty()
    {
        return Ok(PlaybackInfo::SegmentUrls(urls));
    }
    extract_dash_base_url(xml)
        .map(PlaybackInfo::DirectUrl)
        .map_err(ProviderError::from)
}

/// Parse a DASH MPD document and resolve all segment URLs from the
/// highest-bandwidth audio `Representation`.
///
/// Returns the initialization URL (if present) followed by all media
/// segment URLs derived from the `SegmentTimeline`.
fn extract_dash_segment_urls(xml: &str) -> Result<Vec<String>, ManifestError> {
    let doc = Document::parse(xml).context(DashXmlSnafu)?;

    let mpd = doc
        .descendants()
        .find(|n| n.has_tag_name("MPD"))
        .context(DashStructureSnafu {
            error: "DASH manifest has no MPD element",
        })?;
    let period = mpd
        .children()
        .find(|n| n.has_tag_name("Period"))
        .context(DashStructureSnafu {
            error: "DASH manifest has no Period element",
        })?;

    let adaptation_sets: Vec<Node<'_, '_>> = period
        .children()
        .filter(|n| n.has_tag_name("AdaptationSet"))
        .collect();

    ensure!(
        !adaptation_sets.is_empty(),
        DashStructureSnafu {
            error: "DASH manifest has no AdaptationSet"
        }
    );

    let audio_set = adaptation_sets
        .iter()
        .copied()
        .find(|set| {
            set.attribute("mimeType")
                .map(|v| v.starts_with("audio"))
                .unwrap_or(false)
                || set
                    .attribute("contentType")
                    .map(|v| v.eq_ignore_ascii_case("audio"))
                    .unwrap_or(false)
        })
        .unwrap_or(adaptation_sets[0]);

    let mut reps: Vec<Node<'_, '_>> = audio_set
        .children()
        .filter(|n| n.has_tag_name("Representation"))
        .collect();

    ensure!(
        !reps.is_empty(),
        DashStructureSnafu {
            error: "DASH manifest has no Representation",
        }
    );

    reps.sort_by_key(|rep| {
        rep.attribute("bandwidth")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    });
    reps.reverse();
    let rep = reps[0];

    let rep_id = rep.attribute("id").unwrap_or("");
    let segment_template = rep
        .children()
        .find(|n| n.has_tag_name("SegmentTemplate"))
        .or_else(|| {
            audio_set
                .children()
                .find(|n| n.has_tag_name("SegmentTemplate"))
        })
        .context(DashStructureSnafu {
            error: "DASH manifest has no SegmentTemplate",
        })?;

    let initialization = segment_template.attribute("initialization");
    let media = segment_template
        .attribute("media")
        .context(DashStructureSnafu {
            error: "DASH SegmentTemplate has no media template",
        })?;
    let start_number = segment_template
        .attribute("startNumber")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1);

    let base_url = rep
        .children()
        .find(|n| n.has_tag_name("BaseURL"))
        .and_then(|n| n.text())
        .or_else(|| {
            audio_set
                .children()
                .find(|n| n.has_tag_name("BaseURL"))
                .and_then(|n| n.text())
        })
        .or_else(|| {
            period
                .children()
                .find(|n| n.has_tag_name("BaseURL"))
                .and_then(|n| n.text())
        })
        .or_else(|| {
            mpd.children()
                .find(|n| n.has_tag_name("BaseURL"))
                .and_then(|n| n.text())
        })
        .unwrap_or("")
        .trim()
        .to_string();

    let timeline = segment_template
        .children()
        .find(|n| n.has_tag_name("SegmentTimeline"))
        .context(DashStructureSnafu {
            error: "DASH SegmentTemplate has no SegmentTimeline",
        })?;

    let mut entries = Vec::new();
    let mut current_time = 0u64;
    let mut current_number = start_number;
    for s in timeline.children().filter(|n| n.has_tag_name("S")) {
        if let Some(t) = s.attribute("t").and_then(|v| v.parse::<u64>().ok()) {
            current_time = t;
        }
        let duration = s
            .attribute("d")
            .and_then(|v| v.parse::<u64>().ok())
            .context(DashStructureSnafu {
                error: "DASH timeline entry missing duration",
            })?;
        let repeats = s
            .attribute("r")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);

        entries.push((current_number, current_time));
        current_number += 1;
        current_time += duration;

        for _ in 0..repeats.max(0) {
            entries.push((current_number, current_time));
            current_number += 1;
            current_time += duration;
        }
    }

    let mut urls = Vec::with_capacity(entries.len() + 1);
    if let Some(init) = initialization {
        let init_path = resolve_dash_template(init, rep_id, 0, 0);
        urls.push(join_dash_url(&base_url, &init_path));
    }
    for (number, time) in entries {
        let path = resolve_dash_template(media, rep_id, number, time);
        urls.push(join_dash_url(&base_url, &path));
    }

    ensure!(
        !urls.is_empty(),
        DashStructureSnafu {
            error: "DASH generated no segment URLs",
        }
    );

    Ok(urls)
}

/// Expand a DASH URL template by replacing `$RepresentationID$`, `$Number$`,
/// and `$Time$` placeholders (including zero-padded variants).
fn resolve_dash_template(template: &str, rep_id: &str, number: u64, time: u64) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let bytes = template.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        let Some(end_rel) = template[i + 1..].find('$') else {
            out.push('$');
            i += 1;
            continue;
        };
        let end = i + 1 + end_rel;
        let token = &template[i + 1..end];
        if token == "RepresentationID" {
            out.push_str(rep_id);
        } else if let Some(width) = token
            .strip_prefix("Number%0")
            .and_then(|s| s.strip_suffix('d'))
        {
            let w = width.parse::<usize>().unwrap_or(0);
            if w > 0 {
                out.push_str(&format!("{number:0w$}"));
            } else {
                out.push_str(&number.to_string());
            }
        } else if token == "Number" {
            out.push_str(&number.to_string());
        } else if let Some(width) = token
            .strip_prefix("Time%0")
            .and_then(|s| s.strip_suffix('d'))
        {
            let w = width.parse::<usize>().unwrap_or(0);
            if w > 0 {
                out.push_str(&format!("{time:0w$}"));
            } else {
                out.push_str(&time.to_string());
            }
        } else if token == "Time" {
            out.push_str(&time.to_string());
        } else {
            out.push('$');
            out.push_str(token);
            out.push('$');
        }
        i = end + 1;
    }
    out
}

/// Join a base URL and a relative segment path, handling absolute URLs
/// and trailing-slash edge cases.
fn join_dash_url(base: &str, part: &str) -> String {
    if part.starts_with("http://") || part.starts_with("https://") {
        return part.to_string();
    }
    if base.is_empty() {
        return part.to_string();
    }
    if base.ends_with('/') || part.starts_with('/') {
        format!("{base}{part}")
    } else {
        format!("{base}/{part}")
    }
}

/// Fallback extractor: scan the DASH XML for the first absolute `<BaseURL>`
/// element, or failing that, the first bare `https://` URL in the document.
fn extract_dash_base_url(xml: &str) -> Result<String, ManifestError> {
    let mut scan_from = 0usize;
    while let Some(tag_start_rel) = xml[scan_from..].find("<BaseURL") {
        let tag_start = scan_from + tag_start_rel;
        let after_open = &xml[tag_start..];
        let Some(open_end_rel) = after_open.find('>') else {
            break;
        };
        let content_start = tag_start + open_end_rel + 1;

        let after_content = &xml[content_start..];
        let Some(close_rel) = after_content.find("</BaseURL>") else {
            scan_from = content_start;
            continue;
        };
        let raw = after_content[..close_rel].trim();
        let url = raw
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#x2F;", "/");

        if url.starts_with("http://") || url.starts_with("https://") {
            return Ok(url);
        }

        scan_from = content_start + close_rel + "</BaseURL>".len();
    }

    if let Some(start) = xml.find("https://").or_else(|| xml.find("http://")) {
        let tail = &xml[start..];
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '<' || c == '"')
            .unwrap_or(tail.len());
        let candidate = tail[..end].trim();
        if candidate.starts_with("http://") || candidate.starts_with("https://") {
            return Ok(candidate.to_string());
        }
    }

    DashStructureSnafu {
        error: "no absolute URL found in DASH manifest",
    }
    .fail()
}
