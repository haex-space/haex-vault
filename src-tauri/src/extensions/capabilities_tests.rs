//! Research R25: wry injects Tauri's main-frame-only scripts, with the invoke key, into a main frame
//! of any origin (Android's origin rules stay `"*"`; Linux and macOS behave the same). What keeps a
//! foreign top-level page from using the key is Tauri's ACL: a request from a non-local origin is
//! refused unless a capability lists that origin under `remote`
//! (`tauri-2.12.1/src/webview/mod.rs:2075-2108`). No capability of haex-vault may do so.

use std::path::Path;

#[test]
fn no_capability_opens_the_ipc_to_a_remote_origin() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("capabilities directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read capability");
        let capability: serde_json::Value = serde_json::from_str(&text).expect("capability JSON");
        assert!(
            capability.get("remote").is_none(),
            "{} grants IPC to remote origins; extension frames and foreign main frames rely on \
             there being none (research R25)",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 0, "no capability files in {}", dir.display());
}
