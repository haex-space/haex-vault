//! The `holzi-ext` URI scheme (research R12): `<base>/<extId>/<path>` serves a file of the bundle
//! an open frame of that extension runs, read from the vault and hashed again.
//!
//! The extension is chosen by the path alone, never by Origin, Referer or a "last extension" cache
//! (the gap in haex-vault 2.x (Legacy) `protocol.rs:21-24, 498-560`). An HTML document is served only with the
//! start token of an open frame of that extension (R13), gets the frame shim and, like every
//! response, the CSP of its bundle. Paths without a file extension fall back to the entry page
//! (SPA routes). Nothing is served while no frame of the extension is open.

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use super::shim;
use crate::extensions::bundle::store::read_verified_file;
use crate::extensions::host::ExtensionHost;
use crate::extensions::mime;
use crate::state::AppState;
use crate::vault_gate::VaultDb;

/// A response before it becomes an HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    pub status: StatusCode,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub csp: Option<String>,
}

impl Served {
    fn error(status: StatusCode) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: Vec::new(),
            csp: None,
        }
    }
}

/// Strict percent-decoding of one URL path: `%XX` pairs only, the result must be UTF-8.
fn percent_decode(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = path.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn start_token(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|pair| pair.strip_prefix("hf="))
}

/// `/<extId>/<path>` → the extension and the decoded path inside its bundle (empty for the root).
fn route(path: &str) -> Option<(Uuid, String)> {
    let (id, rest) = path
        .strip_prefix('/')?
        .split_once('/')
        .unwrap_or((path.strip_prefix('/')?, ""));
    let extension_id = Uuid::try_parse(id).ok()?;
    Some((extension_id, percent_decode(rest)?))
}

fn has_file_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'))
}

/// Answers one request. Blocking (reads the vault).
pub fn serve(
    db: &VaultDb,
    host: &ExtensionHost,
    method: &str,
    path: &str,
    query: Option<&str>,
) -> Served {
    if method != "GET" {
        return Served::error(StatusCode::METHOD_NOT_ALLOWED);
    }
    let Some((extension_id, mut file)) = route(path) else {
        return Served::error(StatusCode::NOT_FOUND);
    };
    let token = start_token(query).and_then(|t| host.frames.by_token(extension_id, t));
    // With a token, the bundle of that frame. Without one (scripts, styles, lazy chunks) the
    // request cannot name its frame: the bundles of the open frames are tried newest first, so a
    // frame still on an older version finds its own (content-hashed) chunks after an update.
    let mut bundles: Vec<Uuid> = Vec::new();
    match &token {
        Some(session) => bundles.extend(session.source.bundle()),
        None => {
            for bundle in host
                .frames
                .of_extension(extension_id)
                .into_iter()
                .filter_map(|session| session.source.bundle())
            {
                if !bundles.contains(&bundle) {
                    bundles.push(bundle);
                }
            }
        }
    }
    let started: Vec<_> = bundles
        .into_iter()
        .filter_map(|b| host.started(b))
        .collect();
    if started.is_empty() {
        return Served::error(StatusCode::FORBIDDEN);
    }

    let read = |bundle_id: Uuid, path: String| {
        db.read_blocking(move |q| read_verified_file(q, bundle_id, &path).map_err(Into::into))
    };
    let mut found = None;
    for bundle in &started {
        match read(bundle.bundle_id, file.clone()) {
            Ok(Some(data)) => {
                found = Some((bundle, data));
                break;
            }
            Ok(None) => {}
            Err(_) => return Served::error(StatusCode::SERVICE_UNAVAILABLE),
        }
    }
    if found.is_none() && !has_file_extension(&file) {
        let bundle = &started[0];
        file = bundle.entry.clone();
        match read(bundle.bundle_id, file.clone()) {
            Ok(data) => found = data.map(|data| (bundle, data)),
            Err(_) => return Served::error(StatusCode::SERVICE_UNAVAILABLE),
        }
    }
    let Some((started, data)) = found else {
        return Served::error(StatusCode::NOT_FOUND);
    };

    let content_type = mime::for_path(&file);
    let body = if content_type.starts_with("text/html") {
        if token.is_none() {
            return Served::error(StatusCode::FORBIDDEN);
        }
        shim::inject(&String::from_utf8_lossy(&data)).into_bytes()
    } else {
        data
    };
    Served {
        status: StatusCode::OK,
        content_type,
        body,
        csp: Some(started.csp.clone()),
    }
}

fn into_response(served: Served) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(served.status)
        .header(header::CONTENT_TYPE, served.content_type)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "no-store");
    if let Some(csp) = served.csp {
        builder = builder.header(header::CONTENT_SECURITY_POLICY, csp);
    }
    builder.body(served.body).unwrap_or_else(|error| {
        log::error!("holzi-ext response: {error}");
        let mut response = Response::new(Vec::new());
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        response
    })
}

fn respond<R: Runtime>(app: &AppHandle<R>, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let state = app.state::<AppState>();
    let Ok(db) = state.database() else {
        return into_response(Served::error(StatusCode::SERVICE_UNAVAILABLE));
    };
    into_response(serve(
        &db,
        &state.extensions(),
        request.method().as_str(),
        request.uri().path(),
        request.uri().query(),
    ))
}

/// Registers the `holzi-ext` scheme on the app builder. Each request is answered off the async
/// executor, because it reads the vault.
pub fn register<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(super::SCHEME, |ctx, request, responder| {
        let app = ctx.app_handle().clone();
        tauri::async_runtime::spawn_blocking(move || responder.respond(respond(&app, &request)));
    })
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
