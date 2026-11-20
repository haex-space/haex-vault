//! haex-vault's window may frame a development server only while developer mode is on (spec 017,
//! US12, research R16). The address is not in the fixed policy of `tauri.conf.json`: when haex-vault
//! serves its main document, [`adjust`] adds the loopback origins to `frame-src`. A policy holds
//! from the moment its document loads, so switching developer mode reloads haex-vault's window, and the
//! window reloads itself once after unlocking a vault in developer mode (`served_dev_frames`).
//!
//! Tauri calls the hook only for documents of its own protocol: in the built app, not for the
//! development server of `tauri dev`.

use std::borrow::Cow;

use tauri::http::{HeaderValue, Request, Response};

use crate::extensions::host::ExtensionHost;

/// The origins a development server may have, as `frame-src` sources (`dev::server_url`).
pub const DEV_FRAME_SOURCES: &str = "http://localhost:* http://127.0.0.1:*";

const CSP: &str = "Content-Security-Policy";

/// `csp` with [`DEV_FRAME_SOURCES`] in its `frame-src` directive. A policy without one gets it
/// with the sources frames fell back to (`child-src`, else `default-src`), so other frames keep
/// what they were allowed.
pub fn with_dev_frames(csp: &str) -> String {
    let mut directives: Vec<String> = csp
        .split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
        .collect();
    let named = |directives: &[String], wanted: &str| {
        directives.iter().position(|directive| {
            directive
                .split_whitespace()
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
        })
    };
    if let Some(index) = named(&directives, "frame-src") {
        directives[index] = format!("{} {DEV_FRAME_SOURCES}", directives[index]);
    } else {
        let fallback = named(&directives, "child-src")
            .or_else(|| named(&directives, "default-src"))
            .map(|index| {
                directives[index]
                    .split_whitespace()
                    .skip(1)
                    .map(|source| format!("{source} "))
                    .collect::<String>()
            })
            .unwrap_or_default();
        directives.push(format!("frame-src {fallback}{DEV_FRAME_SOURCES}"));
    }
    directives.join("; ")
}

/// The hook on haex-vault's web resources: a document with a policy gets the development origins while
/// developer mode is on, and the host remembers whether the document now shown has them.
pub fn adjust(
    host: &ExtensionHost,
    _request: Request<Vec<u8>>,
    response: &mut Response<Cow<'static, [u8]>>,
) {
    let Some(policy) = response.headers_mut().get_mut(CSP) else {
        return;
    };
    let dev = host.dev_frames();
    if dev {
        let Ok(text) = policy.to_str() else {
            return;
        };
        match HeaderValue::from_str(&with_dev_frames(text)) {
            Ok(value) => *policy = value,
            Err(error) => {
                log::warn!("extensions: the development origins did not fit the policy: {error}");
                return;
            }
        }
    }
    host.note_served_dev_frames(dev);
}

#[cfg(test)]
#[path = "dev_csp_tests.rs"]
mod tests;
