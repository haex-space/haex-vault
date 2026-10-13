//! Encrypted folders on the device through the bridge (spec 048 US7, PR K2): the storage id is
//! `device:/`, and a folder is offered and reached only where the extension has file access to the
//! folder that holds it; without, 1002 and nobody is asked. The folder's own permission is asked
//! for as everywhere.

// The setup writes permissions directly.
#![allow(clippy::disallowed_methods)]

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::*;
use crate::extensions::bridge::dispatch::{call as bridge_call, Emit};
use crate::extensions::encrypted_folder_choose::{ChooseAnswer, ChooseDecision, CHOOSE_REQUEST};
use crate::extensions::fs::test_support::{setup, Setup};
use crate::files::encrypted::device::DeviceRoot;
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::test_support::give_key;

/// Answers the chooser with the first folder offered, as `decision`; records what was offered.
struct Window {
    host: Arc<crate::extensions::host::ExtensionHost>,
    decision: Mutex<Option<ChooseDecision>>,
    offered: Mutex<Vec<Value>>,
}

impl Emit for Window {
    fn emit(&self, event: &str, payload: Value) {
        if event != CHOOSE_REQUEST {
            return;
        }
        self.offered
            .lock()
            .unwrap()
            .push(payload["folders"].clone());
        let first = payload["folders"][0]["folderId"]
            .as_str()
            .map(str::to_owned);
        let answer = match (first, *self.decision.lock().unwrap()) {
            (Some(folder_id), Some(decision)) => ChooseAnswer::Decide {
                folder_id,
                decision,
                remember: false,
            },
            _ => ChooseAnswer::Cancel,
        };
        self.host
            .encrypted
            .resolve(payload["requestId"].as_str().unwrap(), answer);
    }
}

struct Scene {
    s: Setup,
    ctx: CallContext,
    window: Arc<Window>,
}

/// The granted area holds `Dokumente/Unterlagen` with `brief.txt`; outside it lies `Geheim`.
fn scene() -> Scene {
    let s = setup();
    give_key(&s.vault, 1);
    let plain = s.frame("good-notes-like.xt");
    let window = Arc::new(Window {
        host: Arc::clone(&plain.host),
        decision: Mutex::new(None),
        offered: Mutex::new(Vec::new()),
    });
    let ctx = CallContext {
        emitter: window.clone(),
        ..plain
    };
    let documents = s.root.join("Dokumente");
    std::fs::create_dir_all(&documents).unwrap();
    let root = DeviceRoot::of(&documents).unwrap();
    let files = files(&ctx, &root.storage_id).unwrap();
    block_on(async {
        create_encrypted_folder(&files, &root.view_of(&documents), "Unterlagen")
            .await
            .unwrap();
        create_encrypted_folder(&files, &root.view_of(&s.outside), "Geheim")
            .await
            .unwrap();
        let note = root.view_of(&documents.join("Unterlagen/brief.txt"));
        let Route::Encrypted(at) = route(&files, &note).await.unwrap() else {
            panic!("inside the folder");
        };
        transfer::put_file(&files, &at, b"Strom 2026")
            .await
            .unwrap();
    });
    Scene { s, ctx, window }
}

impl Scene {
    fn call(&self, method: &str, request: Value) -> Result<Value, BridgeError> {
        bridge_call(&self.ctx, method, &json!({ "request": request }))
    }

    fn choose(&self, decision: ChooseDecision, access: &str) -> Result<Value, BridgeError> {
        *self.window.decision.lock().unwrap() = Some(decision);
        self.call(
            "extension_encrypted_folder_choose",
            json!({ "storageId": "device:/", "access": access }),
        )
    }

    fn offered(&self) -> Vec<String> {
        self.window
            .offered
            .lock()
            .unwrap()
            .last()
            .and_then(Value::as_array)
            .map(|folders| {
                folders
                    .iter()
                    .filter_map(|folder| folder["name"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn at(&self, folder: &str, path: &str) -> Value {
        json!({ "storageId": "device:/", "folderId": folder, "path": path })
    }
}

#[test]
fn a_folder_on_the_device_needs_file_access_to_its_place() {
    let s = scene();
    // Without file access: 1002, and no chooser opens.
    assert_eq!(
        s.choose(ChooseDecision::Read, "read")
            .unwrap_err()
            .code
            .as_u16(),
        1002
    );
    assert!(s.window.offered.lock().unwrap().is_empty());

    // With read access to the granted area: only the folder there, never "Geheim", and writing
    // asks for more than reading offers.
    s.s.grant(&s.ctx, "read", &s.s.root);
    assert_eq!(
        s.choose(ChooseDecision::ReadWrite, "readWrite")
            .unwrap_err()
            .code
            .as_u16(),
        1002
    );
    assert!(s.window.offered.lock().unwrap().is_empty());
    let chosen = s.choose(ChooseDecision::Read, "read").unwrap();
    assert_eq!(s.offered(), ["Unterlagen"]);
    let folder = chosen["folderId"].as_str().unwrap().to_owned();
    let listed = s
        .call("extension_encrypted_folder_list", s.at(&folder, ""))
        .unwrap();
    assert_eq!(listed["entries"][0]["name"], "brief.txt");
    let file = s
        .call(
            "extension_encrypted_folder_download",
            s.at(&folder, "brief.txt"),
        )
        .unwrap();
    assert_eq!(file, json!("U3Ryb20gMjAyNg=="));

    // Writing needs both: the folder's grant for writing and file access for writing to its place.
    s.s.grant(&s.ctx, "readWrite", &s.s.root);
    s.choose(ChooseDecision::ReadWrite, "readWrite").unwrap();
    let mut upload = s.at(&folder, "neu.txt");
    upload["data"] = json!("aGFsbG8=");
    s.call("extension_encrypted_folder_upload", upload.clone())
        .unwrap();
    s.s.remember(&s.ctx, "readWrite", &s.s.root, "denied");
    assert_eq!(
        s.call("extension_encrypted_folder_upload", upload)
            .unwrap_err()
            .code
            .as_u16(),
        1002
    );

    // Once the file access goes, so does the folder: 1002, and nobody is asked.
    s.s.remember(&s.ctx, "read", &s.s.root, "denied");
    let error = s
        .call("extension_encrypted_folder_list", s.at(&folder, ""))
        .unwrap_err();
    assert_eq!(error.code.as_u16(), 1002);
}
