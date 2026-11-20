//! The little HTTP the media server speaks (contracts/media-server.md): the head of a request, and
//! the head of an answer with the headers every answer carries. Pure, so it is tested without a
//! socket.

/// The most a request head may take; a client that sends more gets `431`.
pub const MAX_HEAD: usize = 16 * 1024;

/// What the server needs of a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub path: String,
    pub range: Option<String>,
}

/// The head of `bytes` up to the empty line; `None` while it is incomplete or not HTTP.
pub fn parse_head(bytes: &[u8]) -> Option<Head> {
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n")?;
    let text = std::str::from_utf8(&bytes[..end]).ok()?;
    let mut lines = text.split("\r\n");
    let mut request = lines.next()?.split_whitespace();
    let method = request.next()?.to_owned();
    let path = request.next()?.to_owned();
    request.next()?.strip_prefix("HTTP/")?;
    let range = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("range")
            .then(|| value.trim().to_owned())
    });
    Some(Head {
        method,
        path,
        range,
    })
}

/// The token of a request path (`/<token>`, query ignored); `None` for `/` or a deeper path.
pub fn token_of(path: &str) -> Option<&str> {
    let path = path.split('?').next().unwrap_or_default();
    let token = path.strip_prefix('/')?;
    (!token.is_empty() && !token.contains('/')).then_some(token)
}

/// Headers on every answer: no caching, any origin may read (only haex-vault's window knows a token),
/// and pdf.js may see the range headers.
const COMMON: &str = "Cache-Control: no-store\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Headers: Range\r\n\
Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n\
Access-Control-Allow-Private-Network: true\r\n\
Access-Control-Expose-Headers: Accept-Ranges, Content-Range, Content-Length\r\n\
Connection: close\r\n";

/// A head without a body (`404`, `204`, `405`, `431`, …).
pub fn empty(status: u16, reason: &str) -> String {
    format!("HTTP/1.1 {status} {reason}\r\n{COMMON}Content-Length: 0\r\n\r\n")
}

/// `416` with the real size, so the client learns it.
pub fn unsatisfiable(total: u64) -> String {
    format!(
        "HTTP/1.1 416 Range Not Satisfiable\r\n{COMMON}Content-Range: bytes */{total}\r\nContent-Length: 0\r\n\r\n"
    )
}

/// The head of a body of `length` bytes: `200` for the whole file, `206` with `Content-Range` for a
/// part. Never a `Content-Encoding` (pdf.js turns ranges off for one).
pub fn body(content_type: &str, length: u64, part: Option<(u64, u64, u64)>) -> String {
    let (status, range) = match part {
        Some((start, end, total)) => (
            "206 Partial Content",
            format!("Content-Range: bytes {start}-{end}/{total}\r\n"),
        ),
        None => ("200 OK", String::new()),
    };
    format!(
        "HTTP/1.1 {status}\r\n{COMMON}Content-Type: {content_type}\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\n{range}\r\n"
    )
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
