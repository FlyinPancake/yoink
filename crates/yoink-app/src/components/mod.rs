mod album_card;
mod auth_credentials_form;
mod badge;
mod breadcrumb;
mod button;
mod card;
pub(crate) mod confirm_dialog;
mod dialog_shell;
mod edit_artist_dialog;
mod error_panel;
pub(crate) mod link_provider_dialog;
mod page_header;
mod page_shell;
mod panel;
mod quality_select;
mod resolve_artist_dialog;
mod sidebar;
mod sleeve_badge;
pub mod toast;

pub use album_card::{AlbumCard, MonitorToggle};
pub use auth_credentials_form::AuthCredentialsForm;
pub use badge::{Badge, BadgeSize, BadgeSurface, BadgeVariant, download_status_badge_variant};
pub use breadcrumb::{Breadcrumb, BreadcrumbItem};
pub use button::{Button, ButtonSize, ButtonVariant};
pub use card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
pub use confirm_dialog::ConfirmDialog;
pub use dialog_shell::{
    ArtistAvatar, DialogResultRow, DialogSectionLabel, DialogShell, DialogSize,
};
pub use edit_artist_dialog::EditArtistDialog;
pub use error_panel::ErrorPanel;
pub use link_provider_dialog::LinkProviderDialog;
pub use page_header::PageHeader;
pub use page_shell::PageShell;
pub use panel::{Panel, PanelBody, PanelHeader, PanelTitle};
pub use quality_select::{QualitySelect, quality_label};
pub use resolve_artist_dialog::ResolveArtistDialog;
pub use sidebar::{MobileMenuButton, Sidebar, SidebarProvider};
pub use sleeve_badge::{SleeveBadge, SleeveBadgeView};

/// Extract the first character of a name as an uppercase initial, or "?" if empty.
pub fn fallback_initial(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}
