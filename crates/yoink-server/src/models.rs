// Re-export shared types so the rest of the binary crate can keep using
// `crate::models::MonitoredAlbum` etc. without changes.
pub(crate) use yoink_shared::{
    DownloadJob, DownloadStatus, MonitoredAlbum, MonitoredArtist, TrackInfo,
};
