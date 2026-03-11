mod album;
mod artist;
mod download;
mod helpers;
mod library;
mod matching;
mod track;

use crate::{error::AppResult, state::AppState};

/// Execute a `ServerAction` against the real `AppState`.
pub(crate) async fn dispatch_action_impl(
    state: AppState,
    action: yoink_shared::ServerAction,
) -> AppResult<()> {
    use yoink_shared::ServerAction;

    match action {
        // ── Artist ──────────────────────────────────────────────
        ServerAction::AddArtist {
            name,
            provider,
            external_id,
            image_url,
            external_url,
        } => {
            artist::add_artist(&state, name, provider, external_id, image_url, external_url)
                .await?;
        }

        ServerAction::RemoveArtist {
            artist_id,
            remove_files,
        } => {
            artist::remove_artist(&state, artist_id, remove_files).await?;
        }

        ServerAction::UpdateArtist {
            artist_id,
            name,
            image_url,
        } => {
            artist::update_artist(&state, artist_id, name, image_url).await?;
        }

        ServerAction::ToggleArtistMonitor {
            artist_id,
            monitored,
        } => {
            artist::toggle_artist_monitor(&state, artist_id, monitored).await?;
        }

        ServerAction::FetchArtistBio { artist_id } => {
            artist::fetch_artist_bio(&state, artist_id).await?;
        }

        ServerAction::SyncArtistAlbums { artist_id } => {
            artist::sync_artist_albums(&state, artist_id).await?;
        }

        ServerAction::LinkArtistProvider {
            artist_id,
            provider,
            external_id,
            external_url,
            external_name,
            image_ref,
        } => {
            artist::link_artist_provider(
                &state,
                artist_id,
                provider,
                external_id,
                external_url,
                external_name,
                image_ref,
            )
            .await?;
        }

        ServerAction::UnlinkArtistProvider {
            artist_id,
            provider,
            external_id,
        } => {
            artist::unlink_artist_provider(&state, artist_id, provider, external_id).await?;
        }

        // ── Album ───────────────────────────────────────────────
        ServerAction::ToggleAlbumMonitor {
            album_id,
            monitored,
        } => {
            album::toggle_album_monitor(&state, album_id, monitored).await?;
        }

        ServerAction::SetAlbumQuality { album_id, quality } => {
            album::set_album_quality(&state, album_id, quality).await?;
        }

        ServerAction::BulkMonitor {
            artist_id,
            monitored,
        } => {
            album::bulk_monitor(&state, artist_id, monitored).await?;
        }

        ServerAction::MergeAlbums {
            target_album_id,
            source_album_id,
            result_title,
            result_cover_url,
        } => {
            album::merge_albums(
                &state,
                target_album_id,
                source_album_id,
                result_title,
                result_cover_url,
            )
            .await?;
        }

        ServerAction::RemoveAlbumFiles {
            album_id,
            unmonitor,
        } => {
            album::remove_album_files(&state, album_id, unmonitor).await?;
        }

        ServerAction::AddAlbumArtist {
            album_id,
            artist_id,
        } => {
            album::add_album_artist(&state, album_id, artist_id).await?;
        }

        ServerAction::RemoveAlbumArtist {
            album_id,
            artist_id,
        } => {
            album::remove_album_artist(&state, album_id, artist_id).await?;
        }

        ServerAction::AddAlbum {
            provider,
            external_album_id,
            artist_external_id,
            artist_name,
            monitor_all,
        } => {
            album::add_album(
                &state,
                provider,
                external_album_id,
                artist_external_id,
                artist_name,
                monitor_all,
            )
            .await?;
        }

        // ── Track ───────────────────────────────────────────────
        ServerAction::AddTrack {
            provider,
            external_track_id,
            external_album_id,
            artist_external_id,
            artist_name,
        } => {
            track::add_track(
                &state,
                provider,
                external_track_id,
                external_album_id,
                artist_external_id,
                artist_name,
            )
            .await?;
        }

        ServerAction::ToggleTrackMonitor {
            track_id,
            album_id,
            monitored,
        } => {
            track::toggle_track_monitor(&state, track_id, album_id, monitored).await?;
        }

        ServerAction::SetTrackQuality {
            album_id,
            track_id,
            quality,
        } => {
            track::set_track_quality(&state, album_id, track_id, quality).await?;
        }

        ServerAction::BulkToggleTrackMonitor {
            album_id,
            monitored,
        } => {
            track::bulk_toggle_track_monitor(&state, album_id, monitored).await?;
        }

        // ── Download ────────────────────────────────────────────
        ServerAction::CancelDownload { job_id } => {
            download::cancel_download(&state, job_id).await?;
        }

        ServerAction::ClearCompleted => {
            download::clear_completed(&state).await?;
        }

        ServerAction::RetryDownload { album_id } => {
            download::retry_download(&state, album_id).await?;
        }

        // ── Matching ────────────────────────────────────────────
        ServerAction::AcceptMatchSuggestion { suggestion_id } => {
            matching::accept_match_suggestion(&state, suggestion_id).await?;
        }

        ServerAction::DismissMatchSuggestion { suggestion_id } => {
            matching::dismiss_match_suggestion(&state, suggestion_id).await?;
        }

        ServerAction::RefreshMatchSuggestions { artist_id } => {
            matching::refresh_match_suggestions(&state, artist_id).await?;
        }

        // ── Library ─────────────────────────────────────────────
        ServerAction::RetagLibrary => {
            library::retag_library(&state).await?;
        }

        ServerAction::ScanImportLibrary => {
            library::scan_import_library(&state).await?;
        }

        ServerAction::ConfirmImport { items } => {
            library::confirm_import(&state, items).await?;
        }

        ServerAction::ConfirmExternalImport {
            source_path,
            mode,
            items,
        } => {
            library::confirm_external_import(&state, source_path, mode, items).await?;
        }
    }

    Ok(())
}
