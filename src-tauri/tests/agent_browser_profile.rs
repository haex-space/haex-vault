//! Spec 065 T026 against a real Chromium: a cookie set by a page survives stopping and restarting
//! the agent browser; after stopping, the working directory is gone and only the encrypted folder
//! is left; the browser was steered over the pipe, never a debugging port; another vault on the
//! same device starts without the cookie. Without a Chromium-based browser on the machine the
//! test says so and stops. A browser that does not end in order saves nothing and says so. The
//! profile view (spec 065 FR-046): the cookies are listed, read, changed and deleted through the
//! running browser, a change keeps every other attribute and survives a restart.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use holzi_lib::agent_browser::cdp::Connection;
use holzi_lib::agent_browser::cookies::{self, CookieRef, CookieTarget};
use holzi_lib::agent_browser::dns::DnsSettings;
use holzi_lib::agent_browser::locate::{find, Os, System};
use holzi_lib::agent_browser::profile::Workdir;
use holzi_lib::agent_browser::profile_store::{dir, ProfileStore};
use holzi_lib::agent_browser::{AgentBrowser, StartConfig};
use holzi_lib::identity::{holzi_migration_source, installation_id_path, HolziBootstrap};
use holzi_lib::vault_gate::{VaultDb, VaultGate};

/// A vault with its content key (genesis), as a freshly created instance has.
fn vault(dir: &Path, name: &str) -> VaultDb {
    let id_file = installation_id_path(dir);
    let database = Database::open(DatabaseConfig {
        path: dir.join(format!("{name}.db")),
        key: SqlCipherKey::new(name),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(id_file.clone())),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: holzi_lib::identity::HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    holzi_lib::sync::genesis::run_after_open(&database, &id_file, true);
    VaultGate::new()
        .vault_db(Arc::new(database))
        .expect("open the gate")
}

/// Answers every request with a page that sets the cookie `agent=remembered`.
async fn cookie_server() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request).await;
                let body = "<html><title>cookie</title><body>set</body></html>";
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\
                     Set-Cookie: agent=remembered; Max-Age=86400; Path=/\r\n\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            });
        }
    });
    addr
}

struct Scene {
    os: Os,
    executable: PathBuf,
    data: tempfile::TempDir,
    run_root: tempfile::TempDir,
}

impl Scene {
    fn config(&self, db: &VaultDb, instance: &str) -> StartConfig {
        let db = db.clone();
        StartConfig {
            os: self.os,
            executable: self.executable.clone(),
            workdir: Workdir::of(self.run_root.path(), instance),
            store: ProfileStore::new(
                &dir(self.data.path(), instance),
                Arc::new(move || Ok(db.clone())),
            )
            .expect("store"),
            gate: None,
            // Keep Linux deterministic even if the runner exports a stale DISPLAY value; the
            // integration scenario exercises pipe control and profile persistence, not windows.
            headless: cfg!(target_os = "linux"),
            // These tests must not depend on a network.
            dns: DnsSettings::off(),
        }
    }
}

fn scene() -> Option<Scene> {
    let os = Os::current()?;
    let Some(executable) = find(os, None, &System) else {
        eprintln!("skipped: no Chromium-based browser on this machine");
        return None;
    };
    Some(Scene {
        os,
        executable,
        data: tempfile::tempdir().unwrap(),
        run_root: tempfile::tempdir().unwrap(),
    })
}

/// Opens `url` in a new target and waits until it has loaded.
async fn visit(connection: &Connection, url: &str) {
    let target = connection
        .call("Target.createTarget", json!({ "url": url }), None)
        .await
        .expect("a target");
    let target_id = target["targetId"].as_str().unwrap().to_owned();
    for _ in 0..100 {
        let targets = connection
            .call("Target.getTargets", json!({}), None)
            .await
            .unwrap();
        let loaded = targets["targetInfos"].as_array().unwrap().iter().any(|t| {
            t["targetId"] == Value::String(target_id.clone())
                && t["title"] == Value::String("cookie".to_owned())
        });
        if loaded {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the page did not load");
}

async fn cookies(connection: &Connection) -> Vec<String> {
    let all = connection
        .call("Storage.getCookies", json!({}), None)
        .await
        .expect("cookies");
    all["cookies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            format!(
                "{}={}",
                c["name"].as_str().unwrap(),
                c["value"].as_str().unwrap()
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cookie_survives_a_restart_and_nothing_readable_stays_behind() {
    let Some(scene) = scene() else {
        return;
    };
    let db = vault(scene.data.path(), "anna");
    let server = cookie_server().await;
    let browser = AgentBrowser::default();

    let connection = browser
        .ensure(|| Ok(scene.config(&db, "anna")))
        .await
        .expect("the browser starts");
    let product = browser.product().await.expect("running");
    assert!(product.contains('/'), "{product}");
    let workdir = Workdir::of(scene.run_root.path(), "anna");
    assert!(
        !workdir.profile().join("DevToolsActivePort").exists(),
        "steered over the pipe, no debugging port"
    );
    visit(&connection, &format!("http://{server}/")).await;
    assert!(cookies(&connection)
        .await
        .contains(&"agent=remembered".to_owned()));

    browser.stop().await.expect("ends in order");
    assert!(!workdir.dir.exists(), "the working directory is gone");
    let kept: Vec<String> = std::fs::read_dir(dir(scene.data.path(), "anna"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(
        kept[0].ends_with(".hxef"),
        "only the encrypted folder: {kept:?}"
    );

    let connection = browser
        .ensure(|| Ok(scene.config(&db, "anna")))
        .await
        .expect("starts again");
    assert!(
        cookies(&connection)
            .await
            .contains(&"agent=remembered".to_owned()),
        "the cookie came back from the encrypted folder"
    );
    browser.stop().await.unwrap();

    // Another vault on the same device has its own, empty profile.
    let other = vault(scene.data.path(), "ben");
    let connection = browser
        .ensure(|| Ok(scene.config(&other, "ben")))
        .await
        .expect("starts for ben");
    assert!(cookies(&connection).await.is_empty());
    browser.stop().await.unwrap();
}

/// Seeds a cookie of the whole domain and one of exactly one host, as pages would.
async fn seed(connection: &Connection) {
    connection
        .call(
            "Storage.setCookies",
            json!({ "cookies": [
                { "name": "sid", "value": "s3cr3t", "domain": ".shop.example", "path": "/",
                  "secure": true, "httpOnly": true, "sameSite": "Lax", "expires": 4_102_444_800_i64 },
                { "name": "theme", "value": "dark", "url": "http://shop.example/app", "path": "/app" },
                { "name": "other", "value": "x", "domain": ".news.example", "path": "/",
                  "expires": 4_102_444_800_i64 },
            ]}),
            None,
        )
        .await
        .expect("seeded");
}

fn key(name: &str, domain: &str, path: &str) -> CookieRef {
    CookieRef {
        name: name.to_owned(),
        domain: domain.to_owned(),
        path: path.to_owned(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_profile_view_lists_reads_changes_and_deletes_cookies_and_a_change_survives() {
    let Some(scene) = scene() else {
        return;
    };
    let db = vault(scene.data.path(), "anna");
    let browser = AgentBrowser::default();
    let connection = browser
        .ensure(|| Ok(scene.config(&db, "anna")))
        .await
        .expect("the browser starts");
    seed(&connection).await;

    // Listed by website and name; the exact domain tells a host cookie from a domain cookie.
    let listed = cookies::list(&connection).await.expect("listed");
    let names: Vec<_> = listed
        .iter()
        .map(|c| (c.view().site, c.name.clone(), c.domain.clone()))
        .collect();
    assert_eq!(
        names,
        [
            (
                "news.example".to_owned(),
                "other".to_owned(),
                ".news.example".to_owned()
            ),
            (
                "shop.example".to_owned(),
                "sid".to_owned(),
                ".shop.example".to_owned()
            ),
            (
                "shop.example".to_owned(),
                "theme".to_owned(),
                "shop.example".to_owned()
            ),
        ]
    );
    let sid = key("sid", ".shop.example", "/");
    assert_eq!(
        cookies::value_of(&connection, sid.clone()).await.unwrap(),
        "s3cr3t"
    );
    let views =
        serde_json::to_string(&listed.iter().map(|c| c.view()).collect::<Vec<_>>()).unwrap();
    assert!(!views.contains("s3cr3t"), "the list carries no value");

    // A change keeps every other attribute and the kind of cookie.
    // The browser limits an expiry to 400 days, so ask for 30.
    let later = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 30 * 86_400;
    cookies::change(&connection, sid.clone(), "changed".to_owned(), Some(later))
        .await
        .expect("changed");
    let host = key("theme", "shop.example", "/app");
    cookies::change(&connection, host.clone(), "light".to_owned(), None)
        .await
        .expect("a host cookie changes too");
    let after = cookies::list(&connection).await.unwrap();
    let sid_now = after.iter().find(|c| c.name == "sid").unwrap();
    assert_eq!(sid_now.value, "changed");
    assert_eq!(sid_now.domain, ".shop.example");
    assert!(sid_now.secure && sid_now.http_only, "{sid_now:?}");
    assert_eq!(sid_now.same_site.as_deref(), Some("Lax"));
    assert_eq!(sid_now.expires as i64, later);
    let theme = after.iter().find(|c| c.name == "theme").unwrap();
    assert_eq!(theme.value, "light");
    assert_eq!(
        theme.domain, "shop.example",
        "still a cookie of exactly that host"
    );
    assert!(theme.expires <= 0.0, "a cookie of the session");
    assert!(
        cookies::change(&connection, sid.clone(), "x".to_owned(), Some(5))
            .await
            .is_err(),
        "an expiry in the past is refused"
    );
    assert!(
        cookies::change(
            &connection,
            key("nope", "shop.example", "/"),
            "x".to_owned(),
            None
        )
        .await
        .is_err(),
        "a cookie that is gone"
    );

    // One cookie goes.
    let gone = cookies::delete(&connection, CookieTarget::Cookie(host))
        .await
        .unwrap();
    assert_eq!(gone, 1);
    let rest: Vec<_> = cookies::list(&connection)
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(rest, ["other", "sid"]);

    // The change is part of the profile: it comes back after a restart.
    browser.stop().await.expect("ends in order");
    let connection = browser
        .ensure(|| Ok(scene.config(&db, "anna")))
        .await
        .expect("starts again");
    assert_eq!(
        cookies::value_of(&connection, sid.clone()).await.unwrap(),
        "changed"
    );

    // Then every cookie of a website.
    let gone = cookies::delete(&connection, CookieTarget::Site("shop.example".to_owned()))
        .await
        .unwrap();
    assert_eq!(gone, 1);
    let rest: Vec<_> = cookies::list(&connection)
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(rest, ["other"], "only the other website's cookie is left");
    browser.stop().await.unwrap();
}

/// A stand-in browser on the pipe: it answers every call, and ends with an error on
/// `Browser.close`, as a browser does that did not end in order.
#[cfg(unix)]
const ENDS_WITH_AN_ERROR: &str = r#"#!/usr/bin/env bash
while IFS= read -r -d '' message <&3; do
  [[ $message =~ \"id\":([0-9]+) ]] || continue
  [[ $message == *'"Browser.close"'* ]] && exit 3
  printf '{"id":%s,"result":{"product":"Chrome/200.0.0.0"}}\0' "${BASH_REMATCH[1]}" >&4
done
"#;

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_browser_that_does_not_end_in_order_saves_nothing_and_says_so() {
    use std::os::unix::fs::PermissionsExt;

    let bin = tempfile::tempdir().unwrap();
    let executable = bin.path().join("browser");
    std::fs::write(&executable, ENDS_WITH_AN_ERROR).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let scene = Scene {
        os: Os::current().expect("a desktop"),
        executable,
        data: tempfile::tempdir().unwrap(),
        run_root: tempfile::tempdir().unwrap(),
    };
    let db = vault(scene.data.path(), "anna");
    let browser = AgentBrowser::default();
    browser
        .ensure(|| Ok(scene.config(&db, "anna")))
        .await
        .expect("the stand-in starts");

    let error = browser.stop().await.expect_err("not an orderly end");
    assert!(error.to_string().contains("ended with an error"), "{error}");
    assert!(
        !dir(scene.data.path(), "anna").exists(),
        "nothing was saved"
    );
    assert!(!Workdir::of(scene.run_root.path(), "anna").dir.exists());
}
