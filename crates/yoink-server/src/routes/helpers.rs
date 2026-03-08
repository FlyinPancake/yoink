use axum::response::{IntoResponse, Redirect, Response};

use crate::redirects::{percent_encode_component, sanitize_relative_target};

pub(super) fn redirect_with_error(base: &str, message: &str, next: Option<&str>) -> Response {
    let mut location = format!("{base}?error={}", percent_encode_component(message));
    if let Some(next) = next.filter(|next| *next != "/") {
        location.push_str("&next=");
        location.push_str(&percent_encode_component(next));
    }
    Redirect::to(&location).into_response()
}

pub(super) fn sanitize_next_target(next: Option<&str>) -> String {
    sanitize_relative_target(next)
}
