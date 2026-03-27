use std::collections::HashSet;

use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter,
};
use tracing::warn;
use uuid::Uuid;

use crate::{
    db::{self, match_status::MatchStatus, provider::Provider},
    error::{AppError, AppResult},
    services,
    state::AppState,
};

pub(crate) async fn accept_match_suggestion(
    state: &AppState,
    suggestion_id: Uuid,
) -> AppResult<()> {
    if let Some(suggestion) = db::artist_match_suggestion::Entity::find_by_id(suggestion_id)
        .one(&state.db)
        .await?
    {
        accept_artist_match_suggestion(state, suggestion).await?;
        state.notify_sse();
        return Ok(());
    }

    if let Some(suggestion) = db::album_match_suggestion::Entity::find_by_id(suggestion_id)
        .one(&state.db)
        .await?
    {
        accept_album_match_suggestion(state, suggestion).await?;
        state.notify_sse();
        return Ok(());
    }

    Err(AppError::not_found(
        "match suggestion",
        Some(suggestion_id.to_string()),
    ))
}

pub(crate) async fn dismiss_match_suggestion(
    state: &AppState,
    suggestion_id: Uuid,
) -> AppResult<()> {
    if let Some(suggestion) = db::artist_match_suggestion::Entity::find_by_id(suggestion_id)
        .one(&state.db)
        .await?
    {
        let mut model: db::artist_match_suggestion::ActiveModel = suggestion.into();
        model.status = Set(MatchStatus::Dismissed);
        model.update(&state.db).await?;
        state.notify_sse();
        return Ok(());
    }

    if let Some(suggestion) = db::album_match_suggestion::Entity::find_by_id(suggestion_id)
        .one(&state.db)
        .await?
    {
        let album_id = suggestion.album_id;
        let mut model: db::album_match_suggestion::ActiveModel = suggestion.into();
        model.status = Set(MatchStatus::Dismissed);
        model.update(&state.db).await?;

        if let Some(artist_id) =
            services::matching::primary_artist_id_for_album(state, album_id).await?
        {
            services::helpers::spawn_recompute_artist_match_suggestions(state, artist_id);
        }

        state.notify_sse();
        return Ok(());
    }

    Err(AppError::not_found(
        "match suggestion",
        Some(suggestion_id.to_string()),
    ))
}

async fn accept_artist_match_suggestion(
    state: &AppState,
    suggestion: db::artist_match_suggestion::Model,
) -> AppResult<()> {
    let artist_id = suggestion.artist_id;
    let linked_pairs: HashSet<(Provider, String)> =
        db::artist_provider_link::Entity::find_by_artist(artist_id)
            .all(&state.db)
            .await?
            .into_iter()
            .map(|link| (link.provider, link.external_id))
            .collect();

    let (target_provider, target_external_id, use_right) = choose_target_pair(
        &linked_pairs,
        suggestion.left_provider,
        &suggestion.left_external_id,
        suggestion.right_provider,
        &suggestion.right_external_id,
    );

    if let Some(existing) = db::artist_provider_link::Entity::find_by_provider_external(
        target_provider,
        &target_external_id,
    )
    .one(&state.db)
    .await?
        && existing.artist_id != artist_id
    {
        return Err(AppError::conflict(
            "provider artist is already linked to another local artist",
        ));
    }

    if !linked_pairs.contains(&(target_provider, target_external_id.clone())) {
        let link = db::artist_provider_link::ActiveModel {
            artist_id: Set(artist_id),
            provider: Set(target_provider),
            external_id: Set(target_external_id),
            external_url: Set(if use_right {
                suggestion.external_url.clone()
            } else {
                None
            }),
            external_name: Set(if use_right {
                suggestion.external_name.clone()
            } else {
                None
            }),
            ..db::artist_provider_link::ActiveModel::new()
        };
        link.insert(&state.db).await?;
    }

    let mut model: db::artist_match_suggestion::ActiveModel = suggestion.into();
    model.status = Set(MatchStatus::Accepted);
    model.update(&state.db).await?;

    let state = state.clone();
    tokio::spawn(async move {
        if let Err(err) = services::artist::sync_artist_and_refresh(&state, artist_id).await {
            warn!(artist_id = %artist_id, error = %err, "Background artist sync after match accept failed");
        }
    });

    Ok(())
}

async fn accept_album_match_suggestion(
    state: &AppState,
    suggestion: db::album_match_suggestion::Model,
) -> AppResult<()> {
    let linked_pairs: HashSet<(Provider, String)> = db::album_provider_link::Entity::find()
        .filter(db::album_provider_link::Column::AlbumId.eq(suggestion.album_id))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|link| (link.provider, link.provider_album_id))
        .collect();

    let (target_provider, target_external_id, _use_right) = choose_target_pair(
        &linked_pairs,
        suggestion.left_provider,
        &suggestion.left_external_id,
        suggestion.right_provider,
        &suggestion.right_external_id,
    );

    if let Some(existing) = db::album_provider_link::Entity::find()
        .filter(db::album_provider_link::Column::Provider.eq(target_provider))
        .filter(db::album_provider_link::Column::ProviderAlbumId.eq(&target_external_id))
        .one(&state.db)
        .await?
        && existing.album_id != suggestion.album_id
    {
        return Err(AppError::conflict(
            "provider album is already linked to another local album",
        ));
    }

    if !linked_pairs.contains(&(target_provider, target_external_id.clone())) {
        let link = db::album_provider_link::ActiveModel {
            album_id: Set(suggestion.album_id),
            provider: Set(target_provider),
            provider_album_id: Set(target_external_id),
            external_url: Set(suggestion.external_url.clone()),
            external_name: Set(suggestion.external_name.clone()),
            ..db::album_provider_link::ActiveModel::new()
        };
        link.insert(&state.db).await?;
    }

    let album_id = suggestion.album_id;
    let mut model: db::album_match_suggestion::ActiveModel = suggestion.into();
    model.status = Set(MatchStatus::Accepted);
    model.update(&state.db).await?;

    if let Some(artist_id) =
        services::matching::primary_artist_id_for_album(state, album_id).await?
    {
        services::helpers::spawn_recompute_artist_match_suggestions(state, artist_id);
    }

    Ok(())
}

fn choose_target_pair(
    linked_pairs: &HashSet<(Provider, String)>,
    left_provider: Provider,
    left_external_id: &str,
    right_provider: Provider,
    right_external_id: &str,
) -> (Provider, String, bool) {
    let left_linked = linked_pairs.contains(&(left_provider, left_external_id.to_string()));
    let right_linked = linked_pairs.contains(&(right_provider, right_external_id.to_string()));

    if left_linked && !right_linked {
        (right_provider, right_external_id.to_string(), true)
    } else if right_linked && !left_linked {
        (left_provider, left_external_id.to_string(), false)
    } else {
        (right_provider, right_external_id.to_string(), true)
    }
}
