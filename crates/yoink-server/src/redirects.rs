use axum::http::Uri;

pub(crate) fn sanitize_relative_target(next: Option<&str>) -> String {
    match next {
        Some(value)
            if value.starts_with('/')
                && !value.starts_with("//")
                && !value.contains('\\')
                && !value.contains("://")
                && !value
                    .chars()
                    .any(|ch| ch.is_ascii_whitespace() || ch.is_control())
                && !contains_percent_encoded_control_chars(value)
                && Uri::try_from(value)
                    .map(|uri| uri.scheme().is_none() && uri.authority().is_none())
                    .unwrap_or(false) =>
        {
            value.to_string()
        }
        _ => "/".to_string(),
    }
}

pub(crate) fn percent_encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn contains_percent_encoded_control_chars(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == b'%'
            && let (Some(high), Some(low)) = (
                decode_hex_digit(bytes[index + 1]),
                decode_hex_digit(bytes[index + 2]),
            )
            && ((high << 4) | low).is_ascii_control()
        {
            return true;
        }
        index += 1;
    }
    false
}

fn decode_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{percent_encode_component, sanitize_relative_target};

    #[test]
    fn sanitize_relative_target_rejects_header_unsafe_targets() {
        assert_eq!(sanitize_relative_target(Some("/library")), "/library");
        assert_eq!(
            sanitize_relative_target(Some("/library?view=grid")),
            "/library?view=grid"
        );
        assert_eq!(sanitize_relative_target(Some("/\\evil.example")), "/");
        assert_eq!(sanitize_relative_target(Some("/\r\nLocation: /admin")), "/");
        assert_eq!(
            sanitize_relative_target(Some("/library%0d%0aLocation:%20/admin")),
            "/"
        );
        assert_eq!(sanitize_relative_target(Some("/library path")), "/");
        assert_eq!(sanitize_relative_target(Some("/library\tpath")), "/");
    }

    #[test]
    fn sanitize_relative_target_rejects_non_relative_targets() {
        assert_eq!(sanitize_relative_target(Some("https://example.com")), "/");
        assert_eq!(sanitize_relative_target(Some("//example.com/path")), "/");
        assert_eq!(sanitize_relative_target(Some("/://example.com")), "/");
        assert_eq!(sanitize_relative_target(Some("library")), "/");
    }

    #[test]
    fn percent_encode_component_preserves_safe_path_bytes() {
        assert_eq!(
            percent_encode_component("/library?view=grid"),
            "/library%3Fview%3Dgrid"
        );
    }
}
