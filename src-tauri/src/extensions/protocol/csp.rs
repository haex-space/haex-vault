//! The Content-Security-Policy of extension frames (research R12), sent as a response header.
//!
//! Scripts run only from the extension's own URL prefix or as inline scripts whose SHA-256 holzi
//! computed after verifying the bundle (Nuxt configuration, import maps, SDK polyfills) plus the
//! frame shim. WebAssembly may be compiled (`'wasm-unsafe-eval'`), JavaScript never evaluated
//! (`'unsafe-eval'`). Never `ipc:` or `http://ipc.localhost`, no network.

use std::collections::BTreeSet;

use base64::Engine;
use sha2::{Digest, Sha256};

use super::shim::{find_ascii_case_insensitive, tag_end, SHIM};
use crate::extensions::bundle::VerifiedBundle;
use crate::extensions::mime;

/// Origins of holzi's own window, the only allowed parents of a frame.
fn holzi_origins() -> &'static str {
    // The dev server origin is `build.devUrl` of tauri.conf.json.
    match (
        cfg!(any(windows, target_os = "android")),
        cfg!(debug_assertions),
    ) {
        (true, true) => "http://tauri.localhost http://localhost:3030",
        (true, false) => "http://tauri.localhost",
        (false, true) => "tauri://localhost http://localhost:3030",
        (false, false) => "tauri://localhost",
    }
}

fn source_hash(script: &str) -> String {
    format!(
        "'sha256-{}'",
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(script.as_bytes()))
    )
}

/// The `'sha256-…'` sources of every inline `<script>` (one without `src`) in `html`.
pub fn inline_script_hashes(html: &str) -> Vec<String> {
    let mut hashes = Vec::new();
    let mut pos = 0;
    while let Some(start) = find_ascii_case_insensitive(html, "<script", pos) {
        let after_name = html.as_bytes().get(start + 7).copied();
        if !after_name.is_some_and(|b| b.is_ascii_whitespace() || b == b'>' || b == b'/') {
            pos = start + 7;
            continue;
        }
        let Some(open_end) = tag_end(html, start) else {
            break;
        };
        let Some(close) = find_ascii_case_insensitive(html, "</script", open_end) else {
            break;
        };
        if !has_src_attribute(&html[start + 7..open_end - 1]) {
            hashes.push(source_hash(&html[open_end..close]));
        }
        pos = close + 8;
    }
    hashes
}

/// Whether the attributes of a start tag (without `<script` and `>`) contain `src`.
fn has_src_attribute(attributes: &str) -> bool {
    let mut rest = attributes;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        if rest.is_empty() {
            return false;
        }
        let name_end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/')
            .unwrap_or(rest.len());
        if rest[..name_end].eq_ignore_ascii_case("src") {
            return true;
        }
        rest = rest[name_end..].trim_start();
        let Some(value) = rest.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        rest = match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].find(q).map_or("", |end| &value[end + 2..]),
            _ => value
                .find(|c: char| c.is_ascii_whitespace())
                .map_or("", |end| &value[end..]),
        };
    }
}

/// The policy for frames of one extension: `prefix` is its URL prefix (with a trailing `/`),
/// `hashes` the allowed inline scripts.
pub fn policy(prefix: &str, hashes: &BTreeSet<String>) -> String {
    let hashes = hashes.iter().cloned().collect::<Vec<_>>().join(" ");
    let p = prefix;
    format!(
        "default-src 'none'; script-src {p} 'wasm-unsafe-eval' {hashes}; style-src {p} 'unsafe-inline'; \
         img-src {p} data: blob:; font-src {p} data:; media-src {p} data: blob:; \
         connect-src {p}; worker-src {p} blob:; frame-src 'none'; object-src 'none'; \
         base-uri 'none'; form-action 'none'; frame-ancestors {}",
        holzi_origins()
    )
}

/// The policy of a verified bundle: the hashes of all inline scripts of its HTML files and of the
/// shim.
pub fn for_bundle(bundle: &VerifiedBundle, prefix: &str) -> String {
    let mut hashes: BTreeSet<String> = bundle
        .entries
        .iter()
        .filter(|e| mime::for_path(&e.path).starts_with("text/html"))
        .filter_map(|e| std::str::from_utf8(&e.data).ok())
        .flat_map(inline_script_hashes)
        .collect();
    hashes.insert(source_hash(SHIM));
    policy(prefix, &hashes)
}

#[cfg(test)]
#[path = "csp_tests.rs"]
mod tests;
