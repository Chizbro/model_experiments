//! Shared CLI helpers: config resolution, HTTP error formatting, SSE parsing (task 20).

pub mod config_file;
pub mod http_util;
pub mod log_ops;
pub mod resolved;
pub mod sse;

pub use config_file::{default_config_path, load_config_file, FileConfig};
pub use http_util::format_http_api_error;
pub use resolved::{resolve_api_key, resolve_control_plane_url, ConfigSource, ResolvedApiKey};
pub use sse::{SseEvent, SseReader};
