pub mod album_detail;
pub mod artist_detail;
pub mod artists;
pub mod dashboard;
pub mod import;
pub mod library;
pub mod library_albums;
pub mod library_tracks;
pub mod login;
pub mod merge_albums;
pub mod not_found;
pub mod search;
pub mod settings_security;
pub mod wanted;

/// Return an inline SVG string for a provider icon using simpleicons-rs.
/// Injects `fill="currentColor"` so the icon inherits the parent text color.
/// Falls back to a generic circle-dot icon for unknown providers.
pub fn provider_icon_svg(provider: &str) -> String {
    let slug = match provider {
        "tidal" => "tidal",
        "deezer" => "deezer",
        "musicbrainz" => "musicbrainz",
        _ => "",
    };
    if let Some(icon) = simpleicons_rs::slug(slug) {
        icon.svg.replace("<svg ", r#"<svg fill="currentColor" "#)
    } else {
        r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" xmlns="http://www.w3.org/2000/svg"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="1"/></svg>"#.to_string()
    }
}
