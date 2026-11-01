//! Spec 021 T091 (US1, US8, contracts/http-auth.md): the HTTP way end to end. rmcp's own OAuth
//! client goes 401 → metadata → dynamic registration → authorization (answered in the registration
//! dialog) → token → `tools/list`; a hand-written client checks refresh rotation, revocation and the
//! defences in front of every request.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::collections::HashMap;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rmcp::ServiceExt;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::auth::{AuthClient, AuthorizationRequest, OAuthState as ClientOAuth};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use holzi_lib::mcp_server::grants::{OperateLevel, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::manage;
use holzi_lib::mcp_server::registry::{self, AgentMode, Transport};
use holzi_lib::mcp_server::server::Running;
use mcp_fixture::*;

const REDIRECT: &str = "http://127.0.0.1:1/callback";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const WRITE_TOOL: &str = "settings_appearance_setColorScheme";

fn settings_rights() -> Rights {
    Rights {
        operate: [("settings.device".to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        ..Default::default()
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("a client")
}

async fn http_server(fx: &mut Fixture) -> (Running, String) {
    fx.http_port = Some(0);
    let server = fx.server().await;
    let address = server.http_address().expect("the HTTP way runs");
    (server, format!("http://{address}"))
}

/// An `application/x-www-form-urlencoded` body.
fn form(fields: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish()
}

fn query_of(location: &str) -> HashMap<String, String> {
    url::Url::parse(location)
        .expect("a url")
        .query_pairs()
        .into_owned()
        .collect()
}

/// Opens the authorization page like a browser and follows it until holzi redirects.
async fn browse(fx: &mut Fixture, auth_url: &str, answer: impl FnOnce(&Fixture, &str)) -> String {
    let page = http().get(auth_url).send().await.expect("the page");
    assert_eq!(page.status(), 200);
    let html = page.text().await.expect("html");
    assert!(html.contains("In holzi bestätigen"), "{html}");
    let wait = html
        .split("url=")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the page refreshes")
        .to_owned();
    let base = url::Url::parse(auth_url).expect("a url");
    let wait = base.join(&wait).expect("the wait url");
    let (id, question) = fx.next_question().await;
    assert_eq!(question.transport, Transport::Http);
    answer(fx, &id);
    for _ in 0..100 {
        let response = http().get(wait.clone()).send().await.expect("waiting");
        if response.status() == 302 {
            return response.headers()["location"]
                .to_str()
                .expect("a location")
                .to_owned();
        }
        assert_eq!(response.status(), 200);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("holzi never redirected");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_rmcp_oauth_client_registers_and_lists_its_tools() {
    let mut fx = fixture();
    let (server, base) = http_server(&mut fx).await;

    let refused = http()
        .post(format!("{base}/mcp"))
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }))
        .send()
        .await
        .expect("a response");
    assert_eq!(refused.status(), 401);
    let challenge = refused.headers()["www-authenticate"]
        .to_str()
        .expect("text")
        .to_owned();
    assert!(
        challenge.contains("/.well-known/oauth-protected-resource"),
        "{challenge}"
    );

    let mut oauth = ClientOAuth::new(format!("{base}/mcp"), None)
        .await
        .expect("an OAuth client");
    oauth
        .start_authorization(AuthorizationRequest::new(REDIRECT).with_client_name("Test Agent"))
        .await
        .expect("discovery and registration");
    fx.no_question().await;
    let auth_url = oauth.get_authorization_url().await.expect("an auth url");
    let location = browse(&mut fx, &auth_url, |fx, id| {
        fx.register_as(id, settings_rights(), AgentMode::Auto)
    })
    .await;
    let callback = query_of(&location);
    oauth
        .handle_callback(&callback["code"], &callback["state"])
        .await
        .expect("the token");
    let manager = oauth.into_authorization_manager().expect("authorized");

    let transport = StreamableHttpClientTransport::with_client(
        AuthClient::new(reqwest::Client::default(), manager),
        StreamableHttpClientTransportConfig::with_uri(format!("{base}/mcp")),
    );
    let client = ().serve(transport).await.expect("an MCP session");
    let names: Vec<String> = client
        .list_all_tools()
        .await
        .expect("tools/list")
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    assert!(names.contains(&WRITE_TOOL.to_owned()), "{names:?}");
    assert!(!fx.agent_id("test-agent").await.is_nil());
    drop(client);
    server.stop(CloseReason::Disabled).await;
}

/// Registers a client, authorizes it through the dialog and exchanges the code by hand.
async fn tokens_by_hand(fx: &mut Fixture, base: &str) -> (String, Value) {
    let registered: Value = http()
        .post(format!("{base}/oauth/register"))
        .json(&json!({ "client_name": "Hand Agent", "redirect_uris": [REDIRECT] }))
        .send()
        .await
        .expect("registered")
        .json()
        .await
        .expect("json");
    let client_id = registered["client_id"].as_str().expect("an id").to_owned();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(VERIFIER.as_bytes()));
    let auth_url = format!(
        "{base}/oauth/authorize?response_type=code&client_id={client_id}\
         &redirect_uri={}&code_challenge={challenge}&code_challenge_method=S256&state=s1",
        url::form_urlencoded::byte_serialize(REDIRECT.as_bytes()).collect::<String>()
    );
    let location = browse(fx, &auth_url, |fx, id| fx.register(id, settings_rights())).await;
    let callback = query_of(&location);
    assert_eq!(callback["state"], "s1");
    let tokens: Value = http()
        .post(format!("{base}/oauth/token"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form(&[
            ("grant_type", "authorization_code"),
            ("code", callback["code"].as_str()),
            ("client_id", client_id.as_str()),
            ("redirect_uri", REDIRECT),
            ("code_verifier", VERIFIER),
        ]))
        .send()
        .await
        .expect("tokens")
        .json()
        .await
        .expect("json");
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], 3600);
    (client_id, tokens)
}

async fn refresh(base: &str, client_id: &str, token: &str) -> reqwest::Response {
    http()
        .post(format!("{base}/oauth/token"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", token),
            ("client_id", client_id),
        ]))
        .send()
        .await
        .expect("a response")
}

async fn mcp_status(base: &str, access: &str) -> u16 {
    http()
        .post(format!("{base}/mcp"))
        .bearer_auth(access)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "hand", "version": "1" }
            }
        }))
        .send()
        .await
        .expect("a response")
        .status()
        .as_u16()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_refresh_token_rotates_and_dies_with_a_revocation() {
    let mut fx = fixture();
    let (server, base) = http_server(&mut fx).await;
    let (client_id, tokens) = tokens_by_hand(&mut fx, &base).await;
    let first = tokens["refresh_token"].as_str().expect("a refresh token");
    assert_eq!(
        mcp_status(&base, tokens["access_token"].as_str().unwrap()).await,
        200
    );

    let rotated: Value = refresh(&base, &client_id, first)
        .await
        .json()
        .await
        .expect("json");
    let second = rotated["refresh_token"].as_str().expect("rotated");
    assert_ne!(first, second);
    assert_eq!(
        refresh(&base, &client_id, first).await.status(),
        400,
        "used once"
    );
    assert_eq!(
        refresh(&base, "another-client", second).await.status(),
        400,
        "bound to its client"
    );

    let agent = fx.agent_id("hand-agent").await;
    let device = fx.device;
    fx.db
        .write(move |tx| manage::revoke(tx, device, agent).map(|_| ()))
        .await
        .expect("revoked");
    assert_eq!(refresh(&base, &client_id, second).await.status(), 400);
    assert_eq!(
        mcp_status(&base, rotated["access_token"].as_str().unwrap()).await,
        401,
        "the access token dies with the agent"
    );
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requests_from_web_pages_and_oversized_bodies_are_refused() {
    let mut fx = fixture();
    let (server, base) = http_server(&mut fx).await;
    let port = base.rsplit(':').next().expect("a port").to_owned();

    let rebound = http()
        .get(format!("{base}/.well-known/oauth-authorization-server"))
        .header("host", format!("evil.example:{port}"))
        .send()
        .await
        .expect("a response");
    assert_eq!(rebound.status(), 421);

    for path in ["/mcp", "/oauth/token"] {
        let scripted = http()
            .post(format!("{base}{path}"))
            .header("origin", "https://evil.example")
            .body("{}")
            .send()
            .await
            .expect("a response");
        assert_eq!(scripted.status(), 403, "{path}");
    }

    // Announce an oversized body without sending it: the server should reject the headers
    // immediately, and the test client must not race that response by writing a megabyte.
    let mut oversized = TcpStream::connect(("127.0.0.1", port.parse::<u16>().expect("port")))
        .await
        .expect("connect to HTTP server");
    oversized
        .write_all(
            format!(
                "POST /oauth/register HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                1024 * 1024 + 1
            )
            .as_bytes(),
        )
        .await
        .expect("send oversized content length");
    let mut response = Vec::new();
    oversized
        .read_to_end(&mut response)
        .await
        .expect("read oversized response");
    assert!(
        String::from_utf8_lossy(&response)
            .lines()
            .next()
            .is_some_and(|status| status.contains(" 413 "))
    );

    let metadata: Value = http()
        .get(format!("{base}/.well-known/oauth-authorization-server"))
        .send()
        .await
        .expect("metadata")
        .json()
        .await
        .expect("json");
    assert_eq!(metadata["issuer"], base);
    assert_eq!(
        metadata["code_challenge_methods_supported"],
        json!(["S256"])
    );
    fx.no_question().await;
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declined_authorization_registers_nobody() {
    let mut fx = fixture();
    let (server, base) = http_server(&mut fx).await;
    let mut oauth = ClientOAuth::new(format!("{base}/mcp"), None)
        .await
        .expect("an OAuth client");
    oauth
        .start_authorization(AuthorizationRequest::new(REDIRECT).with_client_name("Nosy Agent"))
        .await
        .expect("registered");
    let auth_url = oauth.get_authorization_url().await.expect("an auth url");
    let location = browse(&mut fx, &auth_url, |fx, id| fx.deny(id, false)).await;
    assert_eq!(query_of(&location)["error"], "access_denied");
    let device = fx.device;
    let agents = fx
        .db
        .read(move |q| registry::list(q, device))
        .await
        .expect("the agents");
    assert!(agents.is_empty(), "{agents:?}");
    server.stop(CloseReason::Disabled).await;
}
