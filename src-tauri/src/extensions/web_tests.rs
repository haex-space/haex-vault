// The tests change limits rows directly to set up a state.
#![allow(clippy::disallowed_methods)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use haex_crdt::rusqlite::params;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::extensions::bridge::dispatch::{call, Emit};
use crate::extensions::commands::permissions::{set, PermissionSetArgs};
use crate::extensions::host::{Desktop, ExtensionHost};
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

struct Silent;

impl Emit for Silent {
    fn emit(&self, _event: &str, _payload: Value) {}
}

#[derive(Default)]
struct Opened(Mutex<Vec<String>>);

impl Desktop for Opened {
    fn open_url(&self, url: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(url.to_owned());
        Ok(())
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    rt: tokio::runtime::Runtime,
    ctx: Arc<CallContext>,
    opened: Arc<Opened>,
}

fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-minimal.xt"),
    )
    .unwrap();
    let extension = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    let bundle = vault
        .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
        .unwrap()
        .unwrap()
        .bundle_id;
    let host = Arc::new(ExtensionHost::default());
    let opened = Arc::new(Opened::default());
    host.set_desktop(Arc::clone(&opened) as Arc<dyn Desktop>);
    Setup {
        _dir: dir,
        rt: tokio::runtime::Runtime::new().unwrap(),
        ctx: Arc::new(CallContext {
            db: vault,
            session: host.frames.open(extension, bundle, "tab"),
            host,
            device,
            emitter: Arc::new(Silent),
        }),
        opened,
    }
}

impl Setup {
    fn server(&self) -> MockServer {
        self.rt.block_on(MockServer::start())
    }

    fn mount(&self, server: &MockServer, mock: Mock) {
        self.rt.block_on(mock.mount(server));
    }

    /// Calls `method` as the frame would: on a blocking thread of the runtime.
    fn call(&self, method: &str, params: Value) -> Result<Value, BridgeError> {
        let ctx = Arc::clone(&self.ctx);
        let method = method.to_owned();
        self.rt
            .block_on(self.rt.spawn_blocking(move || call(&ctx, &method, &params)))
            .unwrap()
    }

    fn fetch(&self, params: Value) -> Result<Value, BridgeError> {
        self.call("extension_web_fetch", params)
    }

    fn grant(&self, action: &str, target: &str, status: &str) {
        set(
            &self.ctx.db,
            self.ctx.device,
            PermissionSetArgs {
                extension_id: self.ctx.session.extension_id.to_string(),
                kind: "web".into(),
                action: action.into(),
                target: target.into(),
                status: status.into(),
                replaces: None,
            },
            2,
        )
        .unwrap();
    }

    /// Grants every method on everything below `server`.
    fn grant_server(&self, server: &MockServer) {
        self.grant("*", &format!("{}/*", server.uri()), "granted");
    }

    fn received(&self, server: &MockServer) -> Vec<wiremock::Request> {
        self.rt
            .block_on(server.received_requests())
            .unwrap_or_default()
    }
}

fn decoded(answer: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(answer["body"].as_str().unwrap())
        .unwrap()
}

fn code(result: Result<Value, BridgeError>) -> u16 {
    result.map_or_else(|e| e.code.as_u16(), |_| 0)
}

fn redirect(to: &str, status: u16) -> ResponseTemplate {
    ResponseTemplate::new(status).insert_header("location", to)
}

#[test]
fn a_granted_address_answers_with_status_headers_body_and_final_address() {
    let s = setup();
    let server = s.server();
    s.mount(
        &server,
        Mock::given(method("POST"))
            .and(path("/dav/cal"))
            .respond_with(
                ResponseTemplate::new(207)
                    .insert_header("x-test", "yes")
                    .set_body_bytes(b"<multistatus/>".to_vec()),
            ),
    );
    s.grant("POST", &format!("{}/dav/*", server.uri()), "granted");
    let answer = s
        .fetch(json!({
            "url": format!("{}/dav/cal", server.uri()),
            "method": "post",
            "headers": {"Content-Type": "text/xml", "Depth": "1"},
            "body": base64::engine::general_purpose::STANDARD.encode("<propfind/>"),
        }))
        .unwrap();
    assert_eq!(answer["status"], 207);
    assert_eq!(answer["statusText"], "Multi-Status");
    assert_eq!(answer["headers"]["x-test"], "yes");
    assert_eq!(decoded(&answer), b"<multistatus/>");
    assert_eq!(answer["url"], format!("{}/dav/cal", server.uri()));
    let received = s.received(&server);
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].body, b"<propfind/>");
    assert_eq!(received[0].headers["depth"], "1");
}

#[test]
fn without_a_permission_haex_asks_for_the_origin_and_sends_nothing() {
    let s = setup();
    let server = s.server();
    let url = format!("{}/icons/a.ico", server.uri());
    let asked = s.fetch(json!({ "url": url })).unwrap_err();
    assert_eq!(asked.code.as_u16(), 1004);
    assert_eq!(
        asked.details,
        Some(
            json!({"resourceType": "web", "action": "GET", "target": format!("{}/*", server.uri())})
        )
    );

    s.grant("GET", &format!("{}/*", server.uri()), "granted");
    assert_eq!(
        code(s.fetch(json!({ "url": url, "method": "DELETE" }))),
        1004,
        "the method counts"
    );
    s.grant("DELETE", &format!("{}/*", server.uri()), "denied");
    assert_eq!(
        code(s.fetch(json!({ "url": url, "method": "DELETE" }))),
        1002
    );
    assert_eq!(s.received(&server).len(), 0);
}

#[test]
fn a_redirect_to_an_address_without_permission_asks_for_that_address() {
    let s = setup();
    let (first, second) = (s.server(), s.server());
    s.mount(
        &first,
        Mock::given(path("/go")).respond_with(redirect(&format!("{}/there", second.uri()), 302)),
    );
    s.grant_server(&first);
    let asked = s
        .fetch(json!({ "url": format!("{}/go", first.uri()) }))
        .unwrap_err();
    assert_eq!(asked.code.as_u16(), 1004);
    assert_eq!(
        asked.details.unwrap()["target"],
        format!("{}/*", second.uri())
    );
    assert_eq!(s.received(&second).len(), 0, "never sent to the new target");

    s.mount(
        &second,
        Mock::given(path("/there")).respond_with(ResponseTemplate::new(200).set_body_string("ok")),
    );
    s.grant_server(&second);
    let answer = s
        .fetch(json!({ "url": format!("{}/go", first.uri()) }))
        .unwrap();
    assert_eq!(answer["url"], format!("{}/there", second.uri()));
    assert_eq!(decoded(&answer), b"ok");
}

#[test]
fn a_post_redirected_to_an_address_without_permission_answers_with_the_redirect() {
    let s = setup();
    let (first, second) = (s.server(), s.server());
    s.mount(
        &first,
        Mock::given(path("/create"))
            .respond_with(redirect(&format!("{}/created", second.uri()), 303)),
    );
    s.grant_server(&first);
    let body = base64::engine::general_purpose::STANDARD.encode("{}");
    // Asking would make the SDK send the POST again from the start: the extension gets the
    // redirect and follows it itself.
    let answer = s
        .fetch(json!({ "url": format!("{}/create", first.uri()), "method": "POST", "body": body }))
        .unwrap();
    assert_eq!(answer["status"], 303);
    assert_eq!(
        answer["headers"]["location"],
        format!("{}/created", second.uri())
    );
    assert_eq!(answer["url"], format!("{}/create", first.uri()));
    assert_eq!(s.received(&first).len(), 1, "sent once");
    assert!(s.received(&second).is_empty());
}

#[test]
fn a_patch_kept_by_a_temporary_redirect_answers_with_the_redirect() {
    let s = setup();
    let (first, second) = (s.server(), s.server());
    s.mount(
        &first,
        Mock::given(path("/item")).respond_with(redirect(&format!("{}/item", second.uri()), 307)),
    );
    s.grant_server(&first);
    let answer = s
        .fetch(json!({ "url": format!("{}/item", first.uri()), "method": "PATCH" }))
        .unwrap();
    assert_eq!(answer["status"], 307);
    assert_eq!(s.received(&first).len(), 1);
    assert!(s.received(&second).is_empty());
}

#[test]
fn a_post_redirected_to_a_denied_address_is_refused() {
    let s = setup();
    let (first, second) = (s.server(), s.server());
    s.mount(
        &first,
        Mock::given(path("/create")).respond_with(redirect(&format!("{}/x", second.uri()), 308)),
    );
    s.grant_server(&first);
    s.grant("POST", &format!("{}/*", second.uri()), "denied");
    let refused = s
        .fetch(json!({ "url": format!("{}/create", first.uri()), "method": "POST" }))
        .unwrap_err();
    assert_eq!(refused.code.as_u16(), 1002);
    assert!(s.received(&second).is_empty());
}

#[test]
fn credentials_stay_with_their_origin() {
    let s = setup();
    let (first, second) = (s.server(), s.server());
    s.mount(
        &first,
        Mock::given(path("/a")).respond_with(redirect("/b", 307)),
    );
    s.mount(
        &first,
        Mock::given(path("/b")).respond_with(redirect(&format!("{}/c", second.uri()), 307)),
    );
    s.mount(
        &second,
        Mock::given(path("/c")).respond_with(ResponseTemplate::new(200)),
    );
    s.grant_server(&first);
    s.grant_server(&second);
    s.fetch(json!({
        "url": format!("{}/a", first.uri()),
        "headers": {"Authorization": "Bearer secret", "Cookie": "s=1", "X-Keep": "1"},
    }))
    .unwrap();
    let same = s.received(&first);
    assert_eq!(same.len(), 2);
    assert!(same.iter().all(|r| r.headers.contains_key("authorization")));
    let other = s.received(&second);
    assert_eq!(other.len(), 1);
    assert!(!other[0].headers.contains_key("authorization"));
    assert!(!other[0].headers.contains_key("cookie"));
    assert_eq!(other[0].headers["x-keep"], "1");
}

#[test]
fn a_see_other_redirect_turns_a_post_into_a_get_without_body() {
    let s = setup();
    let server = s.server();
    s.mount(
        &server,
        Mock::given(path("/form")).respond_with(redirect("/done", 303)),
    );
    s.mount(
        &server,
        Mock::given(method("GET"))
            .and(path("/done"))
            .respond_with(ResponseTemplate::new(200)),
    );
    s.grant("POST", &format!("{}/*", server.uri()), "granted");
    let first = s
        .fetch(json!({
            "url": format!("{}/form", server.uri()),
            "method": "POST",
            "body": base64::engine::general_purpose::STANDARD.encode("a=1"),
        }))
        .unwrap();
    assert_eq!(
        first["status"], 303,
        "the GET after the redirect needs its own permission; asking would send the POST again, \
         so the redirect is the answer"
    );
    assert_eq!(s.received(&server).len(), 1, "the POST went out once");
    s.grant("GET", &format!("{}/*", server.uri()), "granted");
    s.fetch(json!({
        "url": format!("{}/form", server.uri()),
        "method": "POST",
        "body": base64::engine::general_purpose::STANDARD.encode("a=1"),
    }))
    .unwrap();
    let last = s.received(&server).pop().unwrap();
    assert_eq!(last.method.as_str(), "GET");
    assert!(last.body.is_empty());
}

#[test]
fn ten_redirects_are_followed_and_the_eleventh_fails() {
    let s = setup();
    let server = s.server();
    for hop in 0..11 {
        s.mount(
            &server,
            Mock::given(path(format!("/r{hop}")))
                .respond_with(redirect(&format!("/r{}", hop + 1), 302)),
        );
    }
    s.mount(
        &server,
        Mock::given(path("/r11")).respond_with(ResponseTemplate::new(200)),
    );
    s.grant_server(&server);
    let ten = s
        .fetch(json!({ "url": format!("{}/r1", server.uri()) }))
        .unwrap();
    assert_eq!(ten["url"], format!("{}/r11", server.uri()));
    let eleven = s
        .fetch(json!({ "url": format!("{}/r0", server.uri()) }))
        .unwrap_err();
    assert_eq!(eleven.code.as_u16(), 2005);
}

#[test]
fn the_answer_size_and_the_run_time_come_from_the_limits() {
    let s = setup();
    let server = s.server();
    s.mount(
        &server,
        Mock::given(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7; 100_000])),
    );
    s.mount(
        &server,
        Mock::given(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(5))),
    );
    s.grant_server(&server);
    let ext = s.ctx.session.extension_id.to_string();
    s.ctx
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "UPDATE extension_limits SET max_response_bytes = 65536, timeout_ms = 300 \
                 WHERE extension_id = ?1",
                params![ext],
            )
            .map(drop)
        })
        .unwrap();
    assert_eq!(
        code(s.fetch(json!({ "url": format!("{}/big", server.uri()) }))),
        7000
    );
    let started = std::time::Instant::now();
    assert_eq!(
        code(s.fetch(json!({ "url": format!("{}/slow", server.uri()), "timeout": 60_000 }))),
        7000
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "the limit, not the request's own timeout"
    );
}

#[test]
fn only_http_and_https_and_no_header_of_the_connection() {
    let s = setup();
    let server = s.server();
    s.grant("*", "*", "granted");
    for url in [
        "file:///etc/passwd",
        "ftp://example.org/x",
        "data:text/plain,x",
        "not a url",
    ] {
        assert_eq!(code(s.fetch(json!({ "url": url }))), 3001, "{url}");
    }
    for header in ["Host", "Content-Length", "Transfer-Encoding", "Connection"] {
        assert_eq!(
            code(s.fetch(json!({ "url": server.uri(), "headers": { header: "x" } }))),
            3001,
            "{header}"
        );
    }
    s.mount(
        &server,
        Mock::given(path("/out")).respond_with(redirect("file:///etc/passwd", 302)),
    );
    assert_eq!(
        code(s.fetch(json!({ "url": format!("{}/out", server.uri()) }))),
        2005
    );
}

#[test]
fn an_address_opens_in_the_browser_only_with_a_permission() {
    let s = setup();
    let url = "https://example.org/help";
    assert_eq!(
        code(s.call("extension_web_open", json!({ "url": url }))),
        1004
    );
    assert!(s.opened.0.lock().unwrap().is_empty());
    assert_eq!(
        code(s.call("extension_web_open", json!({ "url": "file:///etc/passwd" }))),
        3001
    );
    s.grant("GET", "https://example.org/*", "granted");
    s.call("extension_web_open", json!({ "url": url })).unwrap();
    assert_eq!(*s.opened.0.lock().unwrap(), [url]);
}

#[test]
fn checking_tells_the_state_and_grants_nothing() {
    let s = setup();
    let check = |url: &str, method: Option<&str>| {
        s.call(
            "extension_permissions_check_web",
            json!({ "url": url, "method": method }),
        )
        .unwrap()["status"]
            .clone()
    };
    assert_eq!(check("https://example.org/x", None), "ask");
    s.grant("GET", "https://example.org/*", "granted");
    assert_eq!(check("https://example.org/x", None), "granted");
    assert_eq!(check("https://example.org/x", Some("PUT")), "ask");
    assert_eq!(check("file:///etc/passwd", None), "denied");
    // `fetch` refuses `*` as a method, so no grant covers it.
    s.grant("*", "https://example.org/*", "granted");
    assert_eq!(check("https://example.org/x", Some("*")), "denied");
}

#[test]
fn only_redirect_statuses_are_followed() {
    let s = setup();
    let server = s.server();
    s.mount(
        &server,
        Mock::given(path("/cached")).respond_with(redirect("/elsewhere", 304)),
    );
    s.grant_server(&server);
    let answer = s
        .fetch(json!({ "url": format!("{}/cached", server.uri()) }))
        .unwrap();
    assert_eq!(answer["status"], 304);
    assert_eq!(s.received(&server).len(), 1);
}

#[test]
fn without_a_desktop_opening_is_not_available() {
    let s = setup();
    let host = Arc::new(ExtensionHost::default());
    let ctx = CallContext {
        db: s.ctx.db.clone(),
        session: host
            .frames
            .open(s.ctx.session.extension_id, Uuid::nil(), "tab"),
        host,
        device: s.ctx.device,
        emitter: Arc::new(Silent),
    };
    s.grant("GET", "https://example.org/*", "granted");
    assert_eq!(
        code(open(&ctx, &json!({ "url": "https://example.org/" }))),
        8001
    );
}
