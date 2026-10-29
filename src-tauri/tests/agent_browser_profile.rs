//! Spec 065 T026 against a real Chromium: a cookie set by a page survives stopping and restarting
//! the agent browser; after stopping, the working directory is gone and only the encrypted folder
//! is left; the browser was steered over the pipe, never a debugging port; another vault on the
//! same device starts without the cookie. Without a Chromium-based browser on the machine the
//! test says so and stops.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use holzi_lib::agent_browser::cdp::Connection;
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
            // CI runners for Linux have no display; macOS and Windows runners have a desktop.
            headless: cfg!(target_os = "linux")
                && std::env::var_os("DISPLAY").is_none()
                && std::env::var_os("WAYLAND_DISPLAY").is_none(),
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
