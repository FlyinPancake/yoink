use std::path::PathBuf;

use sea_orm::{ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set};

use crate::{
    app_config::AuthConfig,
    db::{self, album_type::AlbumType, quality::Quality, wanted_status::WantedStatus},
    providers::registry::ProviderRegistry,
    state::AppState,
};

pub(crate) async fn test_state() -> AppState {
    test_state_with_music_root_and_registry(PathBuf::from("./music"), ProviderRegistry::new()).await
}

pub(crate) async fn test_state_with_music_root(music_root: PathBuf) -> AppState {
    test_state_with_music_root_and_registry(music_root, ProviderRegistry::new()).await
}

pub(crate) async fn test_state_with_registry(registry: ProviderRegistry) -> AppState {
    test_state_with_music_root_and_registry(PathBuf::from("./music"), registry).await
}

pub(crate) async fn seed_artist(
    state: &AppState,
    name: impl Into<String>,
    monitored: bool,
) -> db::artist::Model {
    db::artist::ActiveModel {
        name: Set(name.into()),
        image_url: Set(None),
        bio: Set(None),
        monitored: Set(monitored),
        ..db::artist::ActiveModel::new()
    }
    .insert(&state.db)
    .await
    .expect("insert artist")
}

pub(crate) async fn seed_album(
    state: &AppState,
    title: impl Into<String>,
    wanted_status: WantedStatus,
) -> db::album::Model {
    db::album::ActiveModel {
        title: Set(title.into()),
        album_type: Set(AlbumType::Album),
        release_date: Set(None),
        cover_url: Set(None),
        explicit: Set(false),
        wanted_status: Set(wanted_status),
        requested_quality: Set(None),
        ..db::album::ActiveModel::new()
    }
    .insert(&state.db)
    .await
    .expect("insert album")
}

pub(crate) async fn link_album_artist(
    state: &AppState,
    album_id: uuid::Uuid,
    artist_id: uuid::Uuid,
    priority: i32,
) -> db::album_artist::Model {
    db::album_artist::ActiveModel {
        album_id: Set(album_id),
        artist_id: Set(artist_id),
        priority: Set(priority),
    }
    .insert(&state.db)
    .await
    .expect("insert album artist")
}

pub(crate) async fn seed_root_folder(
    state: &AppState,
    path: impl Into<String>,
) -> db::root_folder::Model {
    db::root_folder::ActiveModel {
        path: Set(path.into()),
        ..db::root_folder::ActiveModel::new()
    }
    .insert(&state.db)
    .await
    .expect("insert root folder")
}

pub(crate) async fn seed_track(
    state: &AppState,
    album_id: uuid::Uuid,
    title: impl Into<String>,
    track_number: i32,
    status: WantedStatus,
) -> db::track::Model {
    db::track::ActiveModel {
        title: Set(title.into()),
        version: Set(None),
        disc_number: Set(Some(1)),
        track_number: Set(Some(track_number)),
        duration: Set(Some(180)),
        album_id: Set(album_id),
        explicit: Set(false),
        isrc: Set(None),
        root_folder_id: Set(None),
        status: Set(status),
        quality_override: Set(None),
        file_path: Set(None),
        ..db::track::ActiveModel::new()
    }
    .insert(&state.db)
    .await
    .expect("insert track")
}

async fn test_state_with_music_root_and_registry(
    music_root: PathBuf,
    registry: ProviderRegistry,
) -> AppState {
    let db_path = format!(
        "sqlite:/tmp/yoink-test-{}.db?mode=rwc",
        uuid::Uuid::now_v7()
    );

    AppState::new(
        music_root,
        Quality::Lossless,
        false,
        1,
        &db_path,
        registry,
        AuthConfig {
            enabled: false,
            session_secret: String::new(),
            init_admin_username: None,
            init_admin_password: None,
        },
    )
    .await
}
