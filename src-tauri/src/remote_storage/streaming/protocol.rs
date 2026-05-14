//! `haex-stream://` Tauri custom protocol handler.
//!
//! Cross-platform URI shape (Tauri normalises both):
//!   - macOS, iOS, Linux: `haex-stream://localhost/<target>/<...>`
//!   - Windows, Android:  `http://haex-stream.localhost/<target>/<...>`
//!
//! In both cases `request.uri().path()` yields `/<target>/<...>` — that's
//! the only thing we parse.
//!
//! For each request we:
//!   1. parse the path → [`StreamRoute`]
//!   2. construct the matching [`StreamingSource`] adapter
//!   3. parse the `Range:` header (default: full file)
//!   4. respond with `206 Partial Content` (or `200` if no `Range:` header
//!      was present — covers `fetch()` callers; the `<video>` element
//!      always sends `Range:` itself)
//!
//! The handler runs on Tauri's async runtime via `spawn` so S3 I/O does
//! not block the WebView's IPC thread.

use std::sync::Arc;

use tauri::http::{Request, Response};
use tauri::{AppHandle, Manager, UriSchemeResponder};

use super::s3_source::S3StreamingSource;
use super::source::{ByteRange, StreamRoute, StreamingError, StreamingSource};
use crate::AppState;

pub const STREAM_PROTOCOL_NAME: &str = "haex-stream";

/// Entry point wired up in `lib.rs` via `register_asynchronous_uri_scheme_protocol`.
///
/// Tauri hands us a synchronous closure with a `responder` — we spawn the
/// actual async work onto the Tauri runtime and call `responder.respond`
/// from there.
pub fn stream_protocol_handler(
    app_handle: AppHandle,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    tauri::async_runtime::spawn(async move {
        let response = handle(&app_handle, &request).await;
        responder.respond(response);
    });
}

async fn handle(app_handle: &AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    // Browsers send a CORS preflight before the actual GET when the page is
    // served from a different origin than the protocol. Answer it cheaply.
    if request.method() == "OPTIONS" {
        return cors_preflight();
    }

    let route = match parse_route(request.uri().path()) {
        Ok(r) => r,
        Err(e) => return error_response(400, format!("bad URL: {e}")),
    };

    let state = app_handle.state::<AppState>();
    let source: Arc<dyn StreamingSource> = match build_source(&state, route).await {
        Ok(s) => s,
        Err(e) => return streaming_error_response(e),
    };

    let total = match source.size().await {
        Ok(n) => n,
        Err(e) => return streaming_error_response(e),
    };

    let range_header = request
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok());

    // Empty object: skip range parsing entirely. `bytes=0-0` over a 0-byte
    // body is not satisfiable; any `Range:` header is also unsatisfiable.
    if total == 0 {
        if range_header.is_some() {
            return range_not_satisfiable(0);
        }
        let content_type = source
            .content_type()
            .await
            .unwrap_or_else(|| "application/octet-stream".to_string());
        return Response::builder()
            .status(200)
            .header("Content-Type", content_type)
            .header("Content-Length", "0")
            .header("Accept-Ranges", "bytes")
            .header("Cache-Control", "no-store")
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Headers", "Range")
            .body(Vec::new())
            .unwrap_or_else(|_| error_response(500, "failed to build response".into()));
    }

    let (range, status) = match range_header {
        Some(h) => match parse_range_header(h, total) {
            Ok(r) => (r, 206),
            Err(_) => return range_not_satisfiable(total),
        },
        None => match ByteRange::new(0, total - 1) {
            Ok(r) => (r, 200),
            Err(e) => return error_response(500, format!("bad range: {e}")),
        },
    };

    let bytes = match source.read_range(range).await {
        Ok(b) => b,
        Err(e) => return streaming_error_response(e),
    };

    let content_type = source
        .content_type()
        .await
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let builder = Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Content-Length", bytes.len().to_string())
        .header("Accept-Ranges", "bytes")
        .header("Cache-Control", "no-store")
        // Permissive CORS — these URLs only resolve inside the WebView.
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Headers", "Range");

    let builder = if status == 206 {
        builder.header(
            "Content-Range",
            format!("bytes {}-{}/{}", range.start(), range.end(), total),
        )
    } else {
        builder
    };

    builder
        .body(bytes)
        .unwrap_or_else(|_| error_response(500, "failed to build response".into()))
}

/// Construct the right [`StreamingSource`] for a parsed route.
///
/// New targets: add an arm here (and a variant on `StreamRoute`).
async fn build_source(
    state: &tauri::State<'_, AppState>,
    route: StreamRoute,
) -> Result<Arc<dyn StreamingSource>, StreamingError> {
    match route {
        StreamRoute::S3 { backend_id, key } => {
            let source = S3StreamingSource::from_backend_id(&state.db, &backend_id, &key).await?;
            Ok(Arc::new(source))
        }
    }
}

/// Parse `/<target>/<rest…>` into a [`StreamRoute`].
///
/// The `<key>` part for S3 may itself contain `/` — we therefore do a
/// `splitn(2, '/')` after the backend id and treat everything from there
/// as the key. Percent-encoded bytes inside the key are decoded so that
/// e.g. an `+` or space round-trips correctly.
fn parse_route(path: &str) -> Result<StreamRoute, String> {
    let trimmed = path.trim_start_matches('/');
    let mut top = trimmed.splitn(2, '/');
    let target = top.next().unwrap_or("");
    let rest = top.next().unwrap_or("");

    match target {
        "s3" => {
            let mut s3 = rest.splitn(2, '/');
            let backend_id = s3.next().unwrap_or("");
            let raw_key = s3.next().unwrap_or("");
            if backend_id.is_empty() || raw_key.is_empty() {
                return Err("s3 route requires /s3/<backendId>/<key>".into());
            }
            let key = percent_decode(raw_key);
            Ok(StreamRoute::S3 {
                backend_id: backend_id.to_string(),
                key,
            })
        }
        "" => Err("missing target".into()),
        other => Err(format!("unknown target: {other}")),
    }
}

/// Parse a single-range `Range: bytes=N-M` / `bytes=N-` header.
///
/// Multi-range requests (`bytes=0-99,200-299`) are valid HTTP but rare
/// — browsers don't send them for media. Reject with `RangeNotSatisfiable`
/// rather than silently returning the first range.
fn parse_range_header(header: &str, total: u64) -> Result<ByteRange, ()> {
    let bytes = header.strip_prefix("bytes=").ok_or(())?;
    if bytes.contains(',') {
        return Err(());
    }

    let (start_s, end_s) = bytes.split_once('-').ok_or(())?;
    let start: u64 = start_s.trim().parse().map_err(|_| ())?;
    let end: u64 = if end_s.trim().is_empty() {
        total.saturating_sub(1)
    } else {
        end_s.trim().parse().map_err(|_| ())?
    };

    if start > end || end >= total {
        return Err(());
    }

    ByteRange::new(start, end).map_err(|_| ())
}

fn percent_decode(s: &str) -> String {
    // Tiny inline impl to avoid a direct dep on `percent-encoding` while
    // the only user is this route parser. Decodes `%xx` triplets; leaves
    // anything else alone. Invalid escapes pass through literally — same
    // behaviour as `decode_utf8_lossy` for our use.
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn cors_preflight() -> Response<Vec<u8>> {
    Response::builder()
        .status(204)
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "GET, OPTIONS")
        .header("Access-Control-Allow-Headers", "Range")
        .header("Access-Control-Max-Age", "86400")
        .body(Vec::new())
        .unwrap()
}

fn error_response(status: u16, msg: String) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Access-Control-Allow-Origin", "*")
        .body(msg.into_bytes())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn range_not_satisfiable(total: u64) -> Response<Vec<u8>> {
    Response::builder()
        .status(416)
        .header("Content-Range", format!("bytes */{total}"))
        .header("Access-Control-Allow-Origin", "*")
        .body(Vec::new())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn streaming_error_response(e: StreamingError) -> Response<Vec<u8>> {
    let (status, msg) = match &e {
        StreamingError::NotFound(_) => (404, e.to_string()),
        StreamingError::BadRequest(_) => (400, e.to_string()),
        StreamingError::Backend(_) => (500, e.to_string()),
    };
    error_response(status, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_s3_route() {
        let route = parse_route("/s3/abc-123/folder/file.mp4").unwrap();
        let StreamRoute::S3 { backend_id, key } = route;
        assert_eq!(backend_id, "abc-123");
        assert_eq!(key, "folder/file.mp4");
    }

    #[test]
    fn parses_s3_route_with_percent_encoded_key() {
        let route = parse_route("/s3/abc/folder/space%20file.mp4").unwrap();
        let StreamRoute::S3 { key, .. } = route;
        assert_eq!(key, "folder/space file.mp4");
    }

    #[test]
    fn rejects_missing_key() {
        assert!(parse_route("/s3/abc").is_err());
        assert!(parse_route("/s3/").is_err());
        assert!(parse_route("/").is_err());
        assert!(parse_route("/unknown/foo").is_err());
    }

    #[test]
    fn parses_closed_range() {
        let r = parse_range_header("bytes=0-99", 1000).unwrap();
        assert_eq!(r.start(), 0);
        assert_eq!(r.end(), 99);
    }

    #[test]
    fn parses_open_ended_range() {
        let r = parse_range_header("bytes=500-", 1000).unwrap();
        assert_eq!(r.start(), 500);
        assert_eq!(r.end(), 999);
    }

    #[test]
    fn rejects_invalid_range() {
        assert!(parse_range_header("bytes=500-499", 1000).is_err());
        assert!(parse_range_header("bytes=0-1000", 1000).is_err());
        assert!(parse_range_header("bytes=0-99,200-299", 1000).is_err());
        assert!(parse_range_header("range=0-99", 1000).is_err());
        // Non-numeric bounds.
        assert!(parse_range_header("bytes=abc-99", 1000).is_err());
        assert!(parse_range_header("bytes=0-xyz", 1000).is_err());
        // Empty start without prefix.
        assert!(parse_range_header("bytes=", 1000).is_err());
    }

    #[test]
    fn accepts_full_range() {
        // First and last byte of a small file.
        let r = parse_range_header("bytes=0-0", 1).unwrap();
        assert_eq!(r.start(), 0);
        assert_eq!(r.end(), 0);
        let r = parse_range_header("bytes=0-999", 1000).unwrap();
        assert_eq!(r.start(), 0);
        assert_eq!(r.end(), 999);
    }

    #[test]
    fn accepts_open_ended_at_start() {
        let r = parse_range_header("bytes=0-", 1000).unwrap();
        assert_eq!(r.start(), 0);
        assert_eq!(r.end(), 999);
    }

    #[test]
    fn byte_range_rejects_inverted() {
        assert!(ByteRange::new(50, 49).is_err());
    }

    #[test]
    fn route_rejects_empty_target_and_backend() {
        assert!(parse_route("").is_err());
        // Empty backend id between the two slashes.
        assert!(parse_route("/s3//key").is_err());
    }

    #[test]
    fn percent_decode_passes_through_plain_chars() {
        assert_eq!(percent_decode("plain/path.txt"), "plain/path.txt");
        assert_eq!(percent_decode(""), "");
    }

    #[test]
    fn percent_decode_handles_escapes() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%2F"), "/");
        // Unicode (UTF-8 round-trip): "ä" = 0xC3 0xA4
        assert_eq!(percent_decode("%C3%A4"), "ä");
    }

    #[test]
    fn percent_decode_leaves_invalid_escapes() {
        // Truncated triplet at end of string: pass through unchanged.
        assert_eq!(percent_decode("foo%"), "foo%");
        // Non-hex chars after `%`: pass through literally.
        assert_eq!(percent_decode("foo%ZZbar"), "foo%ZZbar");
    }

    #[test]
    fn route_decodes_percent_encoded_backend_segment() {
        // Backend ids are UUIDs in practice but the parser should still
        // accept percent-encoded path bytes — that's `encodeURIComponent`
        // behaviour for the segment delimiter.
        let route = parse_route("/s3/abc-123/with%20space.mp4").unwrap();
        let StreamRoute::S3 { backend_id, key } = route;
        assert_eq!(backend_id, "abc-123");
        assert_eq!(key, "with space.mp4");
    }
}
