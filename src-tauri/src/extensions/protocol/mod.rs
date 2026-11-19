//! Serving extension files to sandboxed frames (research R12): the `haex-ext` URI scheme, the
//! start tokens of frames, the frame shim and the Content-Security-Policy.

pub mod csp;
pub mod handler;
pub mod shim;
pub mod token;

use uuid::Uuid;

/// The URI scheme of extension files.
pub const SCHEME: &str = "haex-ext";

/// Where the webview serves the `haex-ext` scheme: WebView2 and Android map custom schemes to
/// `http://<scheme>.localhost`, WebKit keeps `<scheme>://localhost`.
pub fn base_url() -> &'static str {
    if cfg!(any(windows, target_os = "android")) {
        "http://haex-ext.localhost"
    } else {
        "haex-ext://localhost"
    }
}

/// The path segment that names an extension in its URLs.
pub fn url_id(extension_id: Uuid) -> String {
    extension_id.simple().to_string()
}

/// The URL prefix of an extension's files, with a trailing `/`.
pub fn prefix(extension_id: Uuid) -> String {
    format!("{}/{}/", base_url(), url_id(extension_id))
}

/// Percent-encodes a bundle path for a URL; `/` and the unreserved characters stay.
pub fn encode_path(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
