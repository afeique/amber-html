//! Default output naming: `"<safe-url> <YYYY-MM-DD> <HH-MM-SS>"` in local time,
//! used when the CLI `-n/--name` is not given. See `Plans.md`.

use url::Url;

/// Build the default base filename (no extension) for a capture of `url` at `now`.
///
/// Time uses `HH-MM-SS` (not `HH:MM:SS`) because colons are illegal in Windows
/// filenames.
pub fn default_name(url: &Url, now: chrono::DateTime<chrono::Local>) -> String {
    format!("{} {}", safe_url(url), now.format("%Y-%m-%d %H-%M-%S"))
}

/// Filesystem-safe rendering of a URL: scheme dropped; characters outside
/// `[A-Za-z0-9._-]` replaced with `-`; runs collapsed; leading/trailing `-`
/// trimmed; truncated to ~120 chars.
pub fn safe_url(url: &Url) -> String {
    let host = url.host_str().unwrap_or("");
    let path = url.path().trim_end_matches('/');
    let mut raw = String::new();
    raw.push_str(host);
    raw.push_str(path);
    if let Some(query) = url.query() {
        raw.push('-');
        raw.push_str(query);
    }
    sanitize(&raw)
}

fn sanitize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch);
            prev_dash = ch == '-';
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let truncated: String = out.trim_matches('-').chars().take(120).collect();
    let trimmed = truncated.trim_matches('-');
    if trimmed.is_empty() {
        "page".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn safe_url_basic() {
        let u = Url::parse("https://example.com/blog/intro-to-rust").unwrap();
        assert_eq!(safe_url(&u), "example.com-blog-intro-to-rust");
    }

    #[test]
    fn safe_url_query_and_trailing_slash() {
        let u = Url::parse("https://example.com/path/?q=1&x=2").unwrap();
        assert_eq!(safe_url(&u), "example.com-path-q-1-x-2");
    }

    #[test]
    fn safe_url_root_only() {
        let u = Url::parse("https://example.com").unwrap();
        assert_eq!(safe_url(&u), "example.com");
    }

    #[test]
    fn default_name_format() {
        let u = Url::parse("https://example.com").unwrap();
        let dt = chrono::Local.with_ymd_and_hms(2026, 5, 21, 14, 30, 5).unwrap();
        assert_eq!(default_name(&u, dt), "example.com 2026-05-21 14-30-05");
    }
}
