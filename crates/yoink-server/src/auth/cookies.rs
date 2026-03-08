use axum::http::{HeaderMap, header};
use cookie::{Cookie, SameSite};

const COOKIE_NAME: &str = "yoink_session";
const SESSION_MAX_AGE_SECS: i64 = 24 * 60 * 60;

pub(crate) fn session_cookie_header(value: &str, secure: bool) -> String {
    Cookie::build((COOKIE_NAME, value.to_string()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(cookie::time::Duration::seconds(SESSION_MAX_AGE_SECS))
        .secure(secure)
        .build()
        .to_string()
}

pub(crate) fn clear_session_cookie_header(secure: bool) -> String {
    Cookie::build((COOKIE_NAME, String::new()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(cookie::time::Duration::seconds(0))
        .secure(secure)
        .build()
        .to_string()
}

pub(crate) fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    let header_value = headers.get(header::COOKIE)?.to_str().ok()?;
    header_value
        .split(';')
        .filter_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            if name == COOKIE_NAME {
                Some(value.to_string())
            } else {
                None
            }
        })
        .next()
}

pub(crate) fn is_secure_request(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(|value| !value.eq_ignore_ascii_case("http"))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::is_secure_request;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn secure_request_defaults_to_true_without_forwarded_proto() {
        let headers = HeaderMap::new();

        assert!(is_secure_request(&headers));
    }

    #[test]
    fn secure_request_is_false_when_forwarded_proto_is_http() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("http"));

        assert!(!is_secure_request(&headers));
    }

    #[test]
    fn secure_request_is_true_when_forwarded_proto_is_https() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));

        assert!(is_secure_request(&headers));
    }
}
