use api_types::StandardErrorResponse;
use reqwest::blocking::Response;

/// Human-readable stderr line: HTTP status, `error.code`, `error.message` (see docs/CLIENT_EXPERIENCE.md §2.2).
pub fn format_http_api_error(resp: Response, url: &str) -> String {
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if let Ok(err) = serde_json::from_str::<StandardErrorResponse>(&text) {
        format!(
            "HTTP {} {} — {}: {}",
            status.as_u16(),
            url,
            err.error.code,
            err.error.message
        )
    } else if text.is_empty() {
        format!("HTTP {} {}", status.as_u16(), url)
    } else {
        format!("HTTP {} {} — {}", status.as_u16(), url, text)
    }
}
