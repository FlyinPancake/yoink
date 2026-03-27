use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthStatus {
    pub auth_enabled: bool,
    pub authenticated: bool,
    pub username: Option<String>,
    pub must_change_password: bool,
}
