// These tests write registry rows directly to set up a state.
#![allow(clippy::disallowed_methods)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::*;
use crate::extensions::bridge::dispatch::{call, CallContext};
use crate::extensions::registry::effective::effective_bundle;
use crate::extensions::registry::install::install;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

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
    ctx: CallContext,
    recorded: Arc<Recorded>,
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
            .join("tests/fixtures/extension_bundles/good-notes-like.xt"),
    )
    .unwrap();
    let extension = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    let bundle_id = vault
        .read_blocking(move |q| effective_bundle(q, extension).map_err(Into::into))
        .unwrap()
        .unwrap()
        .bundle_id;
    let host = Arc::new(ExtensionHost::default());
    let session = host.frames.open(extension, bundle_id, "tab");
    let recorded = Arc::new(Recorded::default());
    Setup {
        _dir: dir,
        ctx: CallContext {
            db: vault,
            host,
            session,
            device,
            emitter: Arc::clone(&recorded) as Arc<dyn Emit>,
        },
        recorded,
    }
}

fn foreign() -> String {
    format!("{}__cal__events", "b".repeat(64))
}

fn read_foreign(s: &Setup) -> u16 {
    match call(
        &s.ctx,
        "extension_database_query",
        &json!({"sql": format!("SELECT * FROM {}", foreign())}),
    ) {
        Ok(_) => 0,
        Err(e) => e.code.as_u16(),
    }
}

fn resolve_open(s: &Setup, decision: PermissionDecision, remember: bool) {
    let request = s
        .recorded
        .named("extension-permission-request")
        .pop()
        .unwrap();
    resolve(
        &s.ctx.db,
        &s.ctx.host,
        &*s.recorded,
        s.ctx.device,
        PermissionResolveArgs {
            request_id: request["requestId"].as_str().unwrap().to_owned(),
            decision,
            remember,
        },
        2,
    )
    .unwrap();
}

#[test]
fn a_missing_permission_asks_once_and_a_remembered_grant_passes_afterwards() {
    let s = setup();
    for _ in 0..10 {
        assert_eq!(read_foreign(&s), 1004);
    }
    let requests = s.recorded.named("extension-permission-request");
    assert_eq!(requests.len(), 1, "identical questions are one");
    assert_eq!(requests[0]["kind"], "database");
    assert_eq!(requests[0]["target"], foreign());
    assert_eq!(requests[0]["declared"], false);
    assert_eq!(
        requests[0]["targetMissing"], true,
        "that extension is not installed"
    );

    resolve_open(&s, PermissionDecision::Allow, true);
    let resolved = s.recorded.named("extension-frame-event");
    assert_eq!(
        resolved.last().unwrap()["type"],
        "extension:permission-resolved"
    );
    assert_eq!(
        resolved.last().unwrap()["data"],
        json!({"resourceType": "database", "action": "read", "target": foreign(), "decision": "granted"})
    );
    // Granted: the pre-check passes and SQLite answers (the table does not exist here).
    assert_eq!(read_foreign(&s), 2000);
    let views = list(&s.ctx.db, &s.ctx.host, s.ctx.session.extension_id).unwrap();
    assert!(views
        .iter()
        .any(|v| v.target == foreign() && v.status == "granted" && v.all_devices));
}

#[test]
fn a_cancelled_question_is_asked_again_on_the_next_call() {
    let s = setup();
    assert_eq!(read_foreign(&s), 1004);
    let request = s
        .recorded
        .named("extension-permission-request")
        .pop()
        .unwrap();
    cancel(&s.ctx.host, request["requestId"].as_str().unwrap());
    assert_eq!(read_foreign(&s), 1004, "closing cancels, it never denies");
    assert_eq!(
        s.recorded.named("extension-permission-request").len(),
        2,
        "the question is shown again"
    );
}

#[test]
fn a_setting_never_replaces_a_row_of_another_extension() {
    let s = setup();
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles/good-minimal.xt"),
    )
    .unwrap();
    let other = install(&s.ctx.db, &bytes, vec![], false, s.ctx.device, 1)
        .unwrap()
        .ids
        .extension_id;
    assert_ne!(other, s.ctx.session.extension_id);
    let foreign_row = s
        .ctx
        .db
        .write_blocking(move |tx| {
            permission_store::put(
                tx,
                other,
                &NewPermission {
                    kind: "database",
                    action: "read",
                    target: &foreign(),
                    status: "granted",
                    declared: false,
                    vault_device_uuid: VAULT_WIDE,
                },
                1,
            )
            .map_err(Into::into)
        })
        .unwrap();
    set(
        &s.ctx.db,
        s.ctx.device,
        PermissionSetArgs {
            extension_id: s.ctx.session.extension_id.to_string(),
            kind: "database".into(),
            action: "read".into(),
            target: foreign(),
            status: "denied".into(),
            replaces: Some(foreign_row.to_string()),
        },
        2,
    )
    .unwrap();
    let rows = s
        .ctx
        .db
        .read_blocking(move |q| {
            permission_store::rows_of(q, other)
                .map_err(|e| haex_crdt::Error::consumer(e.to_string()))
        })
        .unwrap();
    assert!(rows.iter().any(|r| r.id == foreign_row), "kept");
}

#[test]
fn a_setting_keeps_a_fitting_scope_and_moves_an_old_one_to_the_scope_of_its_kind() {
    let s = setup();
    let ext = s.ctx.session.extension_id;
    let other_device = Uuid::new_v4();
    s.ctx
        .db
        .write_blocking(move |tx| {
            tx.execute(
                "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
                 VALUES (?1, ?2, 'phone', 1)",
                haex_crdt::rusqlite::params![Uuid::new_v4().to_string(), other_device.to_string()],
            )
            .map(drop)
        })
        .unwrap();
    let me = s.ctx.device;
    let put = |kind: &'static str, action: &'static str, target: &'static str, scope: Uuid| {
        s.ctx
            .db
            .write_blocking(move |tx| {
                permission_store::put(
                    tx,
                    ext,
                    &NewPermission {
                        kind,
                        action,
                        target,
                        status: "granted",
                        declared: false,
                        vault_device_uuid: scope,
                    },
                    1,
                )
                .map_err(Into::into)
            })
            .unwrap()
    };
    let change = |kind: &str, action: &str, target: &str, row: Uuid| {
        set(
            &s.ctx.db,
            me,
            PermissionSetArgs {
                extension_id: ext.to_string(),
                kind: kind.into(),
                action: action.into(),
                target: target.into(),
                status: "denied".into(),
                replaces: Some(row.to_string()),
            },
            2,
        )
        .unwrap();
    };
    let shell_elsewhere = put("shell", "execute", "/usr/bin/git", other_device);
    let old_shell = put("shell", "execute", "/usr/bin/make", VAULT_WIDE);
    let old_file = put("filesystem", "read", "/home/a/docs", me);
    change("shell", "execute", "/usr/bin/git", shell_elsewhere);
    change("shell", "execute", "/usr/bin/make", old_shell);
    change("filesystem", "read", "/home/a/docs", old_file);

    let mut rows: Vec<(String, Uuid, String)> = s
        .ctx
        .db
        .read_blocking(move |q| {
            permission_store::rows_of(q, ext).map_err(|e| haex_crdt::Error::consumer(e.to_string()))
        })
        .unwrap()
        .into_iter()
        .filter(|r| !r.declared)
        .map(|r| (r.target, r.vault_device_uuid, r.status))
        .collect();
    rows.sort();
    assert_eq!(
        rows,
        vec![
            ("/home/a/docs".into(), VAULT_WIDE, "denied".into()),
            ("/usr/bin/git".into(), other_device, "denied".into()),
            ("/usr/bin/make".into(), me, "denied".into()),
        ],
        "a shell row of another device stays there; older rows take the scope of their kind"
    );
}

#[test]
fn a_decision_without_remember_is_held_and_a_denial_answers_1002() {
    let s = setup();
    read_foreign(&s);
    resolve_open(&s, PermissionDecision::Deny, false);
    assert_eq!(read_foreign(&s), 1002);
    let views = list(&s.ctx.db, &s.ctx.host, s.ctx.session.extension_id).unwrap();
    assert!(views.iter().any(|v| v.temporary && v.status == "denied"));
    assert!(views.iter().all(|v| v.temporary), "nothing stored");
}

#[test]
fn denied_beats_granted_and_a_change_applies_to_the_next_call() {
    let s = setup();
    let ext = s.ctx.session.extension_id.to_string();
    let prefix = format!("{}__cal__*", "b".repeat(64));
    let set_status = |target: &str, status: &str| {
        set(
            &s.ctx.db,
            s.ctx.device,
            PermissionSetArgs {
                extension_id: ext.clone(),
                kind: "database".into(),
                action: "read".into(),
                target: target.into(),
                status: status.into(),
                replaces: None,
            },
            3,
        )
        .unwrap();
    };
    set_status(&prefix, "granted");
    assert_eq!(read_foreign(&s), 2000);
    set_status(&foreign(), "denied");
    assert_eq!(read_foreign(&s), 1002, "a matching denial wins");

    let views = list(&s.ctx.db, &s.ctx.host, s.ctx.session.extension_id).unwrap();
    let denial = views.iter().find(|v| v.status == "denied").unwrap();
    remove(
        &s.ctx.db,
        &s.ctx.host,
        s.ctx.device,
        PermissionRemoveArgs {
            extension_id: ext.clone(),
            permission_id: denial.id.clone(),
            temporary_key: None,
        },
    )
    .unwrap();
    assert_eq!(read_foreign(&s), 2000, "the revocation applies at once");
}

#[test]
fn an_unknown_stored_permission_is_absent_and_nothing_can_be_granted_over_the_bridge() {
    let s = setup();
    let ext = s.ctx.session.extension_id;
    s.ctx
        .db
        .write_blocking(move |tx| {
            permission_store::put(
                tx,
                ext,
                &NewPermission {
                    kind: "teleport",
                    action: "read",
                    target: "*",
                    status: "granted",
                    declared: false,
                    vault_device_uuid: VAULT_WIDE,
                },
                1,
            )
            .map(drop)
            .map_err(Into::into)
        })
        .unwrap();
    let views = list(&s.ctx.db, &s.ctx.host, ext).unwrap();
    assert!(views.iter().any(|v| v.kind == "teleport" && v.unreadable));
    assert_eq!(read_foreign(&s), 1004);
    for method in [
        "extension_permission_resolve",
        "extension_permission_cancel",
        "extension_permission_set",
        "extension_permissions_grant",
        "extension_limits_set",
    ] {
        assert_eq!(
            call(&s.ctx, method, &json!({})).unwrap_err().code.as_u16(),
            8000,
            "{method}"
        );
    }
}

#[test]
fn checking_tells_the_state_and_grants_nothing() {
    let s = setup();
    let check = |resource: &str| {
        call(
            &s.ctx,
            "extension_permissions_check_database",
            &json!({"resource": resource, "operation": "read"}),
        )
        .unwrap()["status"]
            .clone()
    };
    assert_eq!(check(&foreign()), "ask");
    assert_eq!(check("chat_threads"), "denied");
    assert_eq!(
        check(&format!(
            "{}__notes-like__notes",
            "3614253f84ba66a8faa168d317a6979992979a3e7ad15ae83ca2823f5ca97d34"
        )),
        "granted"
    );
    assert!(s.recorded.named("extension-permission-request").is_empty());
}

/// Spec 018 (research R11): a question the extension raises while the agent's call of one of its
/// tools runs names the agent; the same question otherwise does not.
#[test]
fn a_question_raised_during_an_agent_call_names_the_agent() {
    let s = setup();
    assert_eq!(read_foreign(&s), 1004);
    let requests = s.recorded.named("extension-permission-request");
    assert_eq!(requests[0]["byAgent"], false);

    let s = setup();
    let call = s.ctx.session.mcp.begin_call();
    assert_eq!(read_foreign(&s), 1004);
    drop(call);
    let requests = s.recorded.named("extension-permission-request");
    assert_eq!(requests[0]["byAgent"], true);
}
