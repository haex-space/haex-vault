use std::path::Path;

use super::*;
use crate::extensions::bridge::dispatch::call;
use crate::extensions::bridge::events::FRAME_EVENT;
use crate::extensions::commands::permissions::{set, PermissionSetArgs};
use crate::extensions::fake_desktop::FakeDesktop;
use crate::extensions::host::Desktop;
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::{VaultDb, VaultGate};

#[derive(Default)]
struct Recorded(Mutex<Vec<(String, Value)>>);

impl Emit for Recorded {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorded {
    fn named(&self, name: &str) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(e, _)| e == name)
            .map(|(_, p)| p.clone())
            .collect()
    }
}

struct Setup {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    recorded: Arc<Recorded>,
    desktop: Arc<FakeDesktop>,
    notes: CallContext,
    minimal: CallContext,
}

fn installed(vault: &VaultDb, fixture: &str, device: Uuid) -> (Uuid, Uuid) {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles")
            .join(fixture),
    )
    .unwrap();
    let extension = install(vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    let bundle = vault
        .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
        .unwrap()
        .unwrap()
        .bundle_id;
    (extension, bundle)
}

fn setup() -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let host = Arc::new(ExtensionHost::default());
    let desktop = Arc::new(FakeDesktop::default());
    host.set_desktop(Arc::clone(&desktop) as Arc<dyn Desktop>);
    let recorded = Arc::new(Recorded::default());
    let context = |(extension, bundle): (Uuid, Uuid)| CallContext {
        db: vault.clone(),
        host: Arc::clone(&host),
        session: host.frames.open(extension, bundle, "tab"),
        device,
        emitter: Arc::clone(&recorded) as Arc<dyn Emit>,
    };
    let notes = context(installed(&vault, "good-notes-like.xt", device));
    let minimal = context(installed(&vault, "good-minimal.xt", device));
    Setup {
        _dir: dir,
        vault,
        recorded,
        desktop,
        notes,
        minimal,
    }
}

impl Setup {
    fn allow(&self, ctx: &CallContext) {
        set(
            &self.vault,
            ctx.device,
            PermissionSetArgs {
                extension_id: ctx.session.extension_id.to_string(),
                kind: "notifications".into(),
                action: "show".into(),
                target: "*".into(),
                status: "granted".into(),
                replaces: None,
            },
            2,
        )
        .unwrap();
    }
}

fn show_call(ctx: &CallContext, options: Value) -> Result<Value, BridgeError> {
    call(
        ctx,
        "extension_notifications_show",
        &json!({ "options": options }),
    )
}

fn shown_id(ctx: &CallContext, options: Value) -> String {
    show_call(ctx, options).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn code(result: Result<Value, BridgeError>) -> u16 {
    result.map_or_else(|e| e.code.as_u16(), |_| 0)
}

#[test]
fn without_a_permission_holzi_asks_and_shows_nothing() {
    let s = setup();
    let asked = show_call(&s.notes, json!({ "title": "Termin" })).unwrap_err();
    assert_eq!(asked.code.as_u16(), 1004);
    assert_eq!(
        asked.details,
        Some(json!({"resourceType": "notifications", "action": "show", "target": "*"}))
    );
    assert_eq!(s.desktop.count(), 0);
}

#[test]
fn a_click_reaches_the_frames_and_brings_the_tab_forward() {
    let s = setup();
    s.allow(&s.notes);
    let id = shown_id(
        &s.notes,
        json!({
            "title": "Standup",
            "body": "in 5 Minuten",
            "primary": { "path": "/event/1" },
            "actions": [
                { "id": "snooze", "label": "Später", "deepLink": { "path": "/event/1/snooze" } },
                { "id": "default", "label": "Fertig", "deepLink": { "path": "/done" } },
            ],
        }),
    );
    let spec = s.desktop.shown.lock().unwrap()[0].0.clone();
    assert_eq!(spec.title, "Standup");
    assert_eq!(spec.body.as_deref(), Some("in 5 Minuten"));
    assert_eq!(
        spec.buttons
            .iter()
            .map(|b| b.id.as_str())
            .collect::<Vec<_>>(),
        ["snooze", "default"]
    );

    s.desktop
        .answer(0, NotificationResponse::Button("snooze".into()));
    let frames = s.recorded.named(FRAME_EVENT);
    assert_eq!(frames.len(), 1, "only the frame of the notifying extension");
    assert_eq!(frames[0]["frame"], s.notes.session.frame);
    assert_eq!(frames[0]["type"], CLICK);
    assert_eq!(
        frames[0]["data"],
        json!({"notificationId": id, "actionId": "snooze", "path": "/event/1/snooze"})
    );
    assert_eq!(
        s.recorded.named(CLICKED),
        [
            json!({"extensionId": s.notes.session.extension_id.to_string(), "path": "/event/1/snooze"})
        ]
    );
    assert_eq!(
        code(call(
            &s.notes,
            "extension_notifications_dismiss",
            &json!({ "id": id })
        )),
        1001,
        "a clicked notification is gone"
    );

    shown_id(
        &s.notes,
        json!({ "title": "Zweite", "primary": { "path": "/p" } }),
    );
    s.desktop.answer(1, NotificationResponse::Body);
    assert_eq!(
        s.recorded.named(FRAME_EVENT)[1]["data"]["path"],
        "/p",
        "a click on the body leads to the primary link"
    );
}

#[test]
fn closing_without_a_click_tells_nobody() {
    let s = setup();
    s.allow(&s.notes);
    let id = shown_id(&s.notes, json!({ "title": "Weg" }));
    s.desktop.answer(0, NotificationResponse::Closed);
    assert!(s.recorded.named(FRAME_EVENT).is_empty());
    assert!(s.recorded.named(CLICKED).is_empty());
    assert_eq!(
        code(call(
            &s.notes,
            "extension_notifications_dismiss",
            &json!({ "id": id })
        )),
        1001
    );
}

#[test]
fn an_extension_dismisses_only_its_own_notifications() {
    let s = setup();
    s.allow(&s.notes);
    let id = shown_id(&s.notes, json!({ "title": "Meins" }));
    for missing in [id.as_str(), "nothing"] {
        assert_eq!(
            code(call(
                &s.minimal,
                "extension_notifications_dismiss",
                &json!({ "id": missing })
            )),
            1001
        );
    }
    assert!(s.desktop.closed().is_empty());
    call(
        &s.notes,
        "extension_notifications_dismiss",
        &json!({ "id": id }),
    )
    .unwrap();
    assert_eq!(s.desktop.closed(), [0]);
}

#[test]
fn the_same_tag_replaces_and_too_many_close_the_oldest() {
    let s = setup();
    s.allow(&s.notes);
    shown_id(&s.notes, json!({ "title": "1", "tag": "reminder" }));
    shown_id(&s.notes, json!({ "title": "2", "tag": "reminder" }));
    assert_eq!(s.desktop.closed(), [0]);

    for i in 0..MAX_OPEN {
        shown_id(&s.notes, json!({ "title": format!("n{i}") }));
    }
    assert_eq!(s.desktop.closed(), [0, 1], "the oldest open one");
    assert_eq!(
        s.notes
            .host
            .notifications
            .of_extension(s.notes.session.extension_id)
            .len(),
        MAX_OPEN
    );
}

#[test]
fn disabling_or_removing_an_extension_closes_its_notifications() {
    let s = setup();
    s.allow(&s.notes);
    shown_id(&s.notes, json!({ "title": "1" }));
    shown_id(&s.notes, json!({ "title": "2" }));
    let extension_id = s.notes.session.extension_id;
    s.notes.host.notifications.close_all(extension_id);
    assert_eq!(s.desktop.closed(), [0, 1]);
    assert!(s
        .notes
        .host
        .notifications
        .of_extension(extension_id)
        .is_empty());
}

#[test]
fn malformed_notifications_are_refused_before_anything_shows() {
    let s = setup();
    s.allow(&s.notes);
    let four = (0..4)
        .map(|i| json!({ "id": format!("a{i}"), "label": "x" }))
        .collect::<Vec<_>>();
    let big = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(vec![0u8; MAX_ICON_BYTES + 1])
    );
    for (options, expected) in [
        (json!({}), 3001),
        (json!({ "title": "" }), 3001),
        (json!({ "title": "x".repeat(MAX_TITLE_CHARS + 1) }), 3001),
        (json!({ "title": "t", "actions": four }), 3001),
        (
            json!({ "title": "t", "actions": [{ "id": "a", "label": "1" }, { "id": "a", "label": "2" }] }),
            3001,
        ),
        (
            json!({ "title": "t", "icon": "https://example.org/i.png" }),
            3001,
        ),
        (
            json!({ "title": "t", "icon": "data:text/html;base64,PGI+" }),
            3001,
        ),
        (json!({ "title": "t", "icon": "missing.png" }), 3001),
        (json!({ "title": "t", "icon": "../secret.png" }), 3001),
        (json!({ "title": "t", "icon": big }), 7000),
    ] {
        assert_eq!(
            code(show_call(&s.notes, options.clone())),
            expected,
            "{options}"
        );
    }
    assert_eq!(s.desktop.count(), 0);
    shown_id(
        &s.notes,
        json!({ "title": "t", "icon": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"png")) }),
    );
    assert_eq!(
        s.desktop.shown.lock().unwrap()[0].0.icon,
        Some((b"png".to_vec(), "png"))
    );
}

#[test]
fn a_refused_system_permission_keeps_the_message_inside_holzi() {
    let s = setup();
    s.allow(&s.notes);
    s.desktop
        .refused
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let answer = show_call(
        &s.notes,
        json!({ "title": "Standup", "body": "in 5 Minuten", "tag": "standup" }),
    );

    assert!(
        answer.unwrap()["id"].is_string(),
        "the extension is not told off"
    );
    assert!(
        s.desktop.shown.lock().unwrap().is_empty(),
        "the system shows nothing"
    );
    assert_eq!(
        s.recorded.named(IN_APP),
        vec![json!({
            "extensionId": s.notes.session.extension_id.to_string(),
            "title": "Standup",
            "body": "in 5 Minuten",
            "tag": "standup",
        })]
    );
}

#[test]
fn without_a_desktop_notifications_are_not_available() {
    let s = setup();
    s.allow(&s.notes);
    let host = Arc::new(ExtensionHost::default());
    let ctx = CallContext {
        db: s.vault.clone(),
        session: host.frames.open(
            s.notes.session.extension_id,
            s.notes.session.source.bundle().unwrap(),
            "tab",
        ),
        host,
        device: s.notes.device,
        emitter: Arc::clone(&s.recorded) as Arc<dyn Emit>,
    };
    assert_eq!(code(show_call(&ctx, json!({ "title": "t" }))), 8001);
}
