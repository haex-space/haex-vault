//! Encrypted folders through the bridge against the fake provider (spec 048 US6, T058): no
//! `remoteStorage`, no question; the chooser and its answers, held until the frame ends or stored
//! with "Erlaubnis merken"; `read` does not write; paths are checked; nothing raw leaks; a damaged
//! entry is 3010.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::*;
use crate::extensions::bridge::dispatch::{call as bridge_call, Emit};
use crate::extensions::encrypted_folder_choose::{ChooseAnswer, ChooseDecision, CHOOSE_REQUEST};
use crate::extensions::permissions::store::rows_of;
use crate::extensions::remote_storage_test_support::{setup, Setup};
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::route::{route, Route};
use crate::files::encrypted::test_support::give_key;

/// Records events and answers the chooser with `answer`.
struct Window {
    host: Arc<crate::extensions::host::ExtensionHost>,
    seen: Mutex<Vec<(String, Value)>>,
    answer: Mutex<Option<ChooseAnswer>>,
}

impl Emit for Window {
    fn emit(&self, event: &str, payload: Value) {
        self.seen
            .lock()
            .unwrap()
            .push((event.to_owned(), payload.clone()));
        if event != CHOOSE_REQUEST {
            return;
        }
        let answer = match self.answer.lock().unwrap().clone() {
            Some(ChooseAnswer::Decide {
                folder_id,
                decision,
                remember,
            }) => ChooseAnswer::Decide {
                // `*` stands for the first folder offered.
                folder_id: if folder_id == "*" {
                    payload["folders"][0]["folderId"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                } else {
                    folder_id
                },
                decision,
                remember,
            },
            other => other.unwrap_or(ChooseAnswer::Cancel),
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

/// The probe with a storage "Fotos" holding the encrypted folder "Unterlagen" with
/// `Belege/strom.txt`.
fn scene() -> Scene {
    let s = setup();
    give_key(&s.vault, 1);
    let window = Arc::new(Window {
        host: Arc::clone(&s.ctx.host),
        seen: Mutex::new(Vec::new()),
        answer: Mutex::new(None),
    });
    let ctx = CallContext {
        db: s.ctx.db.clone(),
        host: Arc::clone(&s.ctx.host),
        session: Arc::clone(&s.ctx.session),
        device: s.ctx.device,
        emitter: window.clone(),
    };
    let files = files(&ctx, &s.photos).unwrap();
    block_on(async {
        create_encrypted_folder(&files, "/", "Unterlagen")
            .await
            .unwrap();
        let Route::Encrypted(at) = route(&files, "/Unterlagen/Belege/strom.txt").await.unwrap()
        else {
            panic!("inside the folder");
        };
        transfer::put_file(&files, &at, b"Strom 2026")
            .await
            .unwrap();
    });
    // A fresh session: nothing of the folder is open in memory.
    let host = Arc::new(crate::extensions::host::ExtensionHost::default());
    host.storage.set(
        s.fake.clone(),
        crate::remote_storage::test_support::resolver(&[]),
    );
    let session = host.frames.open(
        s.ctx.session.extension_id,
        s.ctx.session.source.bundle().unwrap(),
        "tab",
    );
    let window = Arc::new(Window {
        host: Arc::clone(&host),
        seen: Mutex::new(Vec::new()),
        answer: Mutex::new(None),
    });
    let ctx = CallContext {
        db: s.ctx.db.clone(),
        host,
        session,
        device: s.ctx.device,
        emitter: window.clone(),
    };
    Scene { s, ctx, window }
}

impl Scene {
    fn call(&self, method: &str, request: Value) -> Result<Value, BridgeError> {
        bridge_call(&self.ctx, method, &json!({ "request": request }))
    }

    fn code(&self, method: &str, request: Value) -> u16 {
        self.call(method, request)
            .map_or_else(|e| e.code.as_u16(), |_| 0)
    }

    fn answer(&self, answer: ChooseAnswer) {
        *self.window.answer.lock().unwrap() = Some(answer);
    }

    fn events(&self, name: &str) -> Vec<Value> {
        self.window
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == name)
            .map(|(_, payload)| payload.clone())
            .collect()
    }

    fn storage(&self, action: &str) {
        self.s.permit(action, &self.s.photos, "granted");
    }

    /// Chooses the folder with `decision`; its id.
    fn choose(&self, decision: ChooseDecision, remember: bool) -> String {
        self.answer(ChooseAnswer::Decide {
            folder_id: "*".into(),
            decision,
            remember,
        });
        let chosen = self
            .call(
                "extension_encrypted_folder_choose",
                json!({ "storageId": self.s.photos, "access": "readWrite" }),
            )
            .expect("chosen");
        chosen["folderId"].as_str().unwrap().to_owned()
    }

    fn at(&self, folder: &str, path: &str) -> Value {
        json!({ "storageId": self.s.photos, "folderId": folder, "path": path })
    }

    fn stored(&self) -> Vec<String> {
        let id = self.ctx.session.extension_id;
        self.s
            .vault
            .read_blocking(move |q| rows_of(q, id).map_err(Into::into))
            .unwrap()
            .into_iter()
            .filter(|r| r.kind == "encryptedFolder")
            .map(|r| format!("{} {}", r.action, r.status))
            .collect()
    }
}

const METHODS: [&str; 4] = [
    "extension_encrypted_folder_list",
    "extension_encrypted_folder_download",
    "extension_encrypted_folder_upload",
    "extension_encrypted_folder_delete",
];

#[test]
fn without_remote_storage_every_function_is_refused_and_nobody_is_asked() {
    let s = scene();
    let folder = crate::files::encrypted::format::encode_id(&[1; 16]);
    for status in [None, Some("ask"), Some("denied")] {
        if let Some(status) = status {
            s.s.permit("readWrite", &s.s.photos, status);
        }
        let choose = json!({ "storageId": s.s.photos, "access": "read" });
        assert_eq!(s.code("extension_encrypted_folder_choose", choose), 1002);
        for method in METHODS {
            let mut request = s.at(&folder, "Belege/strom.txt");
            request["data"] = json!("eA==");
            assert_eq!(s.code(method, request), 1002, "{method} with {status:?}");
        }
    }
    assert!(s.events(CHOOSE_REQUEST).is_empty());
    assert!(s.events("extension-permission-request").is_empty());
}

#[test]
fn the_chooser_names_the_folders_and_an_answer_without_remember_is_held_until_the_frame_ends() {
    let s = scene();
    s.storage("read");
    s.answer(ChooseAnswer::Decide {
        folder_id: "*".into(),
        decision: ChooseDecision::Read,
        remember: false,
    });
    let chosen = s
        .call(
            "extension_encrypted_folder_choose",
            json!({ "storageId": s.s.photos, "access": "read" }),
        )
        .unwrap();
    assert_eq!(chosen["name"], "Unterlagen");
    let asked = s.events(CHOOSE_REQUEST);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["storageName"], "Fotos");
    assert_eq!(asked[0]["folders"][0]["name"], "Unterlagen");
    let folder = chosen["folderId"].as_str().unwrap().to_owned();
    assert!(s.stored().is_empty(), "held, not stored");

    let listed = s
        .call("extension_encrypted_folder_list", s.at(&folder, ""))
        .unwrap();
    assert_eq!(
        listed,
        json!({ "entries": [{ "name": "Belege", "kind": "folder", "modified": listed["entries"][0]["modified"] }] })
    );
    let file = s
        .call(
            "extension_encrypted_folder_download",
            s.at(&folder, "Belege/strom.txt"),
        )
        .unwrap();
    assert_eq!(file, json!("U3Ryb20gMjAyNg=="));

    // The frame reloads: the held answer is gone and holzi asks again.
    s.ctx
        .host
        .permissions
        .frame_ended(s.ctx.session.extension_id);
    assert_eq!(
        s.code("extension_encrypted_folder_list", s.at(&folder, "")),
        1004
    );
}

#[test]
fn with_remember_the_answer_is_stored_and_a_closed_chooser_decides_nothing() {
    let s = scene();
    s.storage("readWrite");
    s.answer(ChooseAnswer::Cancel);
    let closed = s
        .call(
            "extension_encrypted_folder_choose",
            json!({ "storageId": s.s.photos, "access": "read" }),
        )
        .unwrap();
    assert_eq!(closed, Value::Null);
    assert!(s.stored().is_empty());

    let folder = s.choose(ChooseDecision::ReadWrite, true);
    assert_eq!(s.stored(), ["readWrite granted"]);
    s.ctx
        .host
        .permissions
        .frame_ended(s.ctx.session.extension_id);
    let mut upload = s.at(&folder, "neu/a.txt");
    upload["data"] = json!("aGFsbG8=");
    s.call("extension_encrypted_folder_upload", upload).unwrap();
    let got = s
        .call(
            "extension_encrypted_folder_download",
            s.at(&folder, "neu/a.txt"),
        )
        .unwrap();
    assert_eq!(got, json!("aGFsbG8="));
    s.call("extension_encrypted_folder_delete", s.at(&folder, "neu"))
        .unwrap();
    assert_eq!(
        s.code(
            "extension_encrypted_folder_download",
            s.at(&folder, "neu/a.txt")
        ),
        3001
    );
}

#[test]
fn a_refusal_is_1002_and_without_an_answer_the_question_names_the_folder_only_to_the_user() {
    let s = scene();
    s.storage("readWrite");
    s.answer(ChooseAnswer::Decide {
        folder_id: "*".into(),
        decision: ChooseDecision::Deny,
        remember: false,
    });
    let refused = s
        .call(
            "extension_encrypted_folder_choose",
            json!({ "storageId": s.s.photos, "access": "read" }),
        )
        .unwrap_err();
    assert_eq!(refused.code.as_u16(), 1002);
    let folder = refused.details.unwrap()["target"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(
        s.code("extension_encrypted_folder_list", s.at(&folder, "")),
        1002,
        "the refusal covers reading"
    );

    s.ctx
        .host
        .permissions
        .forget_extension(s.ctx.session.extension_id);
    let asked = s
        .call("extension_encrypted_folder_list", s.at(&folder, ""))
        .unwrap_err();
    assert_eq!(asked.code.as_u16(), 1004);
    let told = serde_json::to_string(&asked).unwrap();
    assert!(
        !told.contains("Unterlagen") && !told.contains("Fotos"),
        "{told}"
    );
    let question = s.events("extension-permission-request");
    assert_eq!(question[0]["kind"], "encryptedFolder");
    assert_eq!(question[0]["encryptedFolder"]["storageName"], "Fotos");
    assert_eq!(question[0]["encryptedFolder"]["folderName"], "Unterlagen");

    // A folder that is not there, without a grant: refused, nothing asked.
    let unknown = crate::files::encrypted::format::encode_id(&[3; 16]);
    assert_eq!(
        s.code("extension_encrypted_folder_list", s.at(&unknown, "")),
        1002
    );
    assert_eq!(s.events("extension-permission-request").len(), 1);
}

#[test]
fn read_does_not_write_and_paths_are_checked() {
    let s = scene();
    s.storage("readWrite");
    let folder = s.choose(ChooseDecision::Read, false);
    let before = s.s.fake.keys("photos");
    let mut upload = s.at(&folder, "x.txt");
    upload["data"] = json!("eA==");
    assert_eq!(s.code("extension_encrypted_folder_upload", upload), 1004);
    assert_eq!(
        s.code("extension_encrypted_folder_delete", s.at(&folder, "Belege")),
        1004
    );
    assert_eq!(s.s.fake.keys("photos"), before);

    for path in ["../x", "/Belege", "Belege//strom.txt", "a\\b", ""] {
        assert_eq!(
            s.code("extension_encrypted_folder_download", s.at(&folder, path)),
            3001,
            "{path:?}"
        );
    }
    assert_eq!(
        s.code("extension_encrypted_folder_list", s.at("not-an-id", "")),
        3001
    );
    assert_eq!(
        s.code(
            "extension_encrypted_folder_download",
            s.at(&folder, "Belege/fehlt.txt")
        ),
        3001
    );
}

#[test]
fn nothing_raw_leaves_and_a_damaged_entry_is_3010() {
    let s = scene();
    s.storage("readWrite");
    let folder = s.choose(ChooseDecision::ReadWrite, true);
    let answers = [
        s.call("extension_encrypted_folder_list", s.at(&folder, "")),
        s.call("extension_encrypted_folder_list", s.at(&folder, "Belege")),
        s.call(
            "extension_encrypted_folder_download",
            s.at(&folder, "Belege/strom.txt"),
        ),
    ];
    let keys = s.s.fake.keys("photos");
    for answer in answers {
        let text = serde_json::to_string(&answer.unwrap()).unwrap();
        assert!(!text.contains(".hxef"), "{text}");
        for key in &keys {
            let last = key.rsplit('/').next().unwrap();
            assert!(
                last.len() < 20 || !text.contains(last),
                "{text} names {key}"
            );
        }
    }
    let content = keys.iter().find(|key| key.contains(".hxef/c/")).unwrap();
    let mut body = s.s.fake.object("photos", content).unwrap();
    let last = body.len() - 1;
    body[last] ^= 1;
    s.s.fake.insert("photos", content, &body);
    assert_eq!(
        s.code(
            "extension_encrypted_folder_download",
            s.at(&folder, "Belege/strom.txt")
        ),
        3010
    );
}

#[test]
fn the_settings_name_stored_and_held_grants_and_a_revoke_holds_at_once() {
    let s = scene();
    s.storage("readWrite");
    let folder = s.choose(ChooseDecision::Read, true);
    s.choose(ChooseDecision::ReadWrite, false);
    let ext = s.ctx.session.extension_id;
    let list = || crate::extensions::commands::permissions::list(&s.s.vault, &s.ctx.host, ext);
    let views: Vec<_> = list()
        .unwrap()
        .into_iter()
        .filter(|v| v.kind == "encryptedFolder")
        .collect();
    assert_eq!(views.len(), 2);
    for view in &views {
        let names = view.folder.as_ref().expect("named");
        assert_eq!(
            (names.storage_name.as_str(), names.folder_name.as_str()),
            ("Fotos", "Unterlagen")
        );
    }
    let held = views.iter().find(|v| v.temporary).unwrap();
    crate::extensions::commands::permissions::remove(
        &s.s.vault,
        &s.ctx.host,
        s.ctx.device,
        crate::extensions::commands::permissions::PermissionRemoveArgs {
            extension_id: ext.to_string(),
            permission_id: None,
            temporary_key: Some(format!("{}|{}|{}", held.kind, held.action, held.target)),
        },
    )
    .unwrap();
    let stored = views.iter().find(|v| !v.temporary).unwrap();
    crate::extensions::commands::permissions::remove(
        &s.s.vault,
        &s.ctx.host,
        s.ctx.device,
        crate::extensions::commands::permissions::PermissionRemoveArgs {
            extension_id: ext.to_string(),
            permission_id: stored.id.clone(),
            temporary_key: None,
        },
    )
    .unwrap();
    assert_eq!(
        s.code("extension_encrypted_folder_list", s.at(&folder, "")),
        1004,
        "revoked at once"
    );
}
