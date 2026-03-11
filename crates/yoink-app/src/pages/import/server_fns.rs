use leptos::prelude::*;

use yoink_shared::{
    BrowseEntry, ExternalImportConfirmation, ImportConfirmation, ImportPreviewItem,
    ImportResultSummary,
};

#[server(PreviewImportLibrary, "/leptos")]
pub async fn preview_import_library() -> Result<Vec<ImportPreviewItem>, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    (ctx.preview_import)().await.map_err(ServerFnError::new)
}

#[server(
    name = ConfirmImportAction,
    prefix = "/leptos",
    input = server_fn::codec::Json,
    output = server_fn::codec::Json
)]
pub async fn confirm_import_action(
    items: Vec<ImportConfirmation>,
) -> Result<ImportResultSummary, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    (ctx.confirm_import)(items)
        .await
        .map_err(ServerFnError::new)
}

#[server(
    name = BrowseServerPath,
    prefix = "/leptos",
    input = server_fn::codec::Json,
    output = server_fn::codec::Json
)]
pub async fn browse_server_path(path: String) -> Result<Vec<BrowseEntry>, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    (ctx.browse_path)(path).await.map_err(ServerFnError::new)
}

#[server(
    name = PreviewExternalImportAction,
    prefix = "/leptos",
    input = server_fn::codec::Json,
    output = server_fn::codec::Json
)]
pub async fn preview_external_import_action(
    source_path: String,
) -> Result<Vec<ImportPreviewItem>, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    (ctx.preview_external_import)(source_path)
        .await
        .map_err(ServerFnError::new)
}

#[server(
    name = ConfirmExternalImportAction,
    prefix = "/leptos",
    input = server_fn::codec::Json,
    output = server_fn::codec::Json
)]
pub async fn confirm_external_import_action(
    confirmation: ExternalImportConfirmation,
) -> Result<ImportResultSummary, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    (ctx.confirm_external_import)(confirmation)
        .await
        .map_err(ServerFnError::new)
}
