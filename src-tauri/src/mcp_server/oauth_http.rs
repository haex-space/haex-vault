//! The OAuth endpoints of the HTTP way (spec 021 US1, T094, contracts/http-auth.md): metadata,
//! dynamic client registration, the authorization page that waits for the registration dialog in
//! the holzi window, and the token endpoint.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use hyper::body::Bytes;
use hyper::http::{header, HeaderValue, Response, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

use super::http::{json, respond, Body, HttpContext};
use super::oauth::{self, Collected, OAuthError};
use super::oauth_flow;

/// The authorization page asks again this often (seconds) while the dialog is open.
const WAIT_REFRESH_SECS: u32 = 1;

pub(super) fn resource_metadata(base: &str) -> Value {
    json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "bearer_methods_supported": ["header"],
    })
}

pub(super) fn server_metadata(base: &str) -> Value {
    json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
    })
}

fn error(error: OAuthError) -> Response<Body> {
    let code = StatusCode::from_u16(error.status()).unwrap_or(StatusCode::BAD_REQUEST);
    json(code, &json!({ "error": error.code() }))
}

#[derive(Deserialize)]
struct ClientMetadata {
    #[serde(default)]
    redirect_uris: Vec<String>,
    client_name: Option<String>,
}

/// `POST /oauth/register` (RFC 7591): public clients only; no agent yet.
pub(super) fn register(context: &HttpContext, body: &Bytes) -> Response<Body> {
    let Ok(metadata) = serde_json::from_slice::<ClientMetadata>(body) else {
        return error(OAuthError::InvalidClientMetadata);
    };
    match context
        .oauth
        .register_client(metadata.client_name, metadata.redirect_uris)
    {
        Ok(client) => json(
            StatusCode::CREATED,
            &json!({
                "client_id": client.id,
                "client_name": client.name,
                "redirect_uris": client.redirect_uris,
                "token_endpoint_auth_method": "none",
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
            }),
        ),
        Err(failure) => error(failure),
    }
}

/// `GET /oauth/authorize`: checks client and redirect URI (never redirects to an unchecked one),
/// asks the dialog, and shows the page that waits for it.
pub(super) fn authorize(context: &HttpContext, query: &str) -> Response<Body> {
    let begun = oauth::parse_authorize(query)
        .and_then(|request| context.oauth.begin(request, Instant::now()));
    let (pending, client) = match begun {
        Ok(begun) => begun,
        Err(failure) => {
            let code = StatusCode::from_u16(failure.status()).unwrap_or(StatusCode::BAD_REQUEST);
            return page(code, None, failure.code());
        }
    };
    tokio::spawn(oauth_flow::decide(
        Arc::clone(&context.core),
        Arc::clone(&context.oauth),
        pending.clone(),
        client,
    ));
    page(StatusCode::OK, Some(&pending), "")
}

/// `GET /oauth/authorize/wait?id=…`: the page again until the user decided, then the redirect.
pub(super) fn wait(context: &HttpContext, query: &str) -> Response<Body> {
    let fields: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let Some(id) = fields.get("id") else {
        return page(StatusCode::BAD_REQUEST, None, "invalid_request");
    };
    match context.oauth.collect(id, Instant::now()) {
        Ok(Collected::Waiting) => page(StatusCode::OK, Some(id), ""),
        Ok(Collected::Redirect(location)) => {
            let mut response = respond(StatusCode::FOUND, "text/plain; charset=utf-8", "");
            if let Ok(value) = HeaderValue::from_str(&location) {
                response.headers_mut().insert(header::LOCATION, value);
            }
            response
        }
        Ok(Collected::Unknown) => page(StatusCode::BAD_REQUEST, None, "invalid_request"),
        Err(failure) => page(StatusCode::BAD_REQUEST, None, failure.code()),
    }
}

/// `POST /oauth/token`: `authorization_code` with PKCE, or `refresh_token` with rotation.
pub(super) async fn token(context: &HttpContext, body: &Bytes) -> Response<Body> {
    let fields: HashMap<String, String> = url::form_urlencoded::parse(body).into_owned().collect();
    let field = |name: &str| fields.get(name).map(String::as_str).unwrap_or_default();
    let issued = match field("grant_type") {
        "authorization_code" => context
            .oauth
            .exchange(
                field("code"),
                field("client_id"),
                field("redirect_uri"),
                field("code_verifier"),
                Instant::now(),
            )
            .and_then(|exchanged| {
                Ok(oauth_flow::Tokens {
                    access: context
                        .oauth
                        .issue_access(exchanged.agent, Instant::now())?,
                    refresh: exchanged.refresh.into_string(),
                })
            }),
        "refresh_token" => {
            oauth_flow::refresh(
                &context.core,
                &context.oauth,
                field("refresh_token"),
                field("client_id"),
            )
            .await
        }
        _ => Err(OAuthError::UnsupportedGrantType),
    };
    match issued {
        Ok(tokens) => json(
            StatusCode::OK,
            &json!({
                "access_token": tokens.access,
                "token_type": "Bearer",
                "expires_in": oauth::ACCESS_TTL.as_secs(),
                "refresh_token": tokens.refresh,
            }),
        ),
        Err(failure) => error(failure),
    }
}

/// The page in the browser: "In holzi bestätigen" while the dialog is open, refreshing itself;
/// no script, no outside resource. Rust has no localized texts, so the page names both languages.
fn page(code: StatusCode, waiting: Option<&str>, failure: &str) -> Response<Body> {
    let refresh = waiting
        .map(|id| {
            format!(
                "<meta http-equiv=\"refresh\" content=\"{WAIT_REFRESH_SECS};url=/oauth/authorize/wait?id={id}\">"
            )
        })
        .unwrap_or_default();
    let text = if waiting.is_some() {
        "<h1>In holzi bestätigen</h1><p>Confirm in holzi</p>".to_owned()
    } else {
        format!("<h1>holzi</h1><p>{failure}</p>")
    };
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>holzi</title>{refresh}\
         <style>body{{font-family:system-ui,sans-serif;margin:3rem auto;max-width:28rem;\
         text-align:center}}</style></head><body>{text}</body></html>"
    );
    let mut response = respond(code, "text/html; charset=utf-8", html);
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
    );
    response
}
