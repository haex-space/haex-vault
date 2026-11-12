//! Spec 092, SC-004 (T014, T031): the value of a code appears only where the user asks for it
//! (`passwords_pass_codes`, one pass at a time, and a revealed state of the history). Every other
//! read path (the overview, the lists, the entry itself, the agents' search, the history list and
//! the view of a state, the drawing as text, the error of a refused call, the `Debug` of an input)
//! and the log of the whole run hold none. The test binary has one capturing logger, so it holds
//! only this test. The bridge, the MCP server and the extension SDK serve logins by the shared
//! default, which `passwords_pass_access.rs` covers.

use std::sync::{Arc, Mutex};

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::{Caller, Grant, GrantAction, Scope};
use holzi_lib::passwords::kind::Kinds;
use holzi_lib::passwords::model::HistorySecret;
use holzi_lib::passwords::pass::model::{CodeInput, PassInput, PassKind, PassPatch, Start};
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::vault_gate::VaultGate;

const MARKER: &str = "SECRET-MARKER-pass-code";
const WRONG_MARKER: &str = "SECRET-MARKER-wrong-ean";

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        LINES.lock().expect("lines").push(format!(
            "{} {}: {}",
            record.level(),
            record.target(),
            record.args()
        ));
    }

    fn flush(&self) {}
}

static CAPTURE: Capture = Capture;

fn code(format: &str, value: &str, position: i64) -> CodeInput {
    serde_json::from_value(serde_json::json!({
        "format": format, "value": value, "position": position
    }))
    .expect("a code")
}

fn assert_clean(what: &str, text: &str) {
    for marker in [MARKER, WRONG_MARKER] {
        assert!(!text.contains(marker), "{what} holds a code value: {text}");
    }
}

#[tokio::test]
async fn no_read_path_returns_the_value_of_a_code() {
    log::set_logger(&CAPTURE).expect("the only logger of this binary");
    log::set_max_level(log::LevelFilter::Trace);

    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-pass-leak"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let db = Arc::new(db);
    let service = PasswordsService::new(
        VaultGate::new()
            .vault_db(Arc::clone(&db))
            .expect("open the gate"),
    );
    let user = Caller::User;

    let input = PassInput {
        title: Some("Concert".to_owned()),
        pass_kind: PassKind::Ticket,
        start: Some(Start::Date {
            date: "2030-05-01".to_owned(),
        }),
        codes: vec![code("qr", MARKER, 0)],
        tag_names: vec!["Ticket".to_owned()],
        ..PassInput::default()
    };
    assert_clean("the Debug of an input", &format!("{input:?}"));
    let created = service
        .pass_create(&user, input, None)
        .await
        .expect("create");
    let id = created.header.id.clone();
    assert_clean("the created pass", &format!("{created:?}"));

    // Editing the codes makes a second state; the codes in it are secrets too.
    let codes = service.pass_codes(&user, id.clone()).await.expect("codes");
    assert_eq!(codes[0].value, MARKER, "the one place that returns it");
    let patch = PassPatch {
        title: holzi_lib::passwords::model::Patch::Set("Concert, late".to_owned()),
        codes: Some(vec![
            CodeInput {
                id: Some(codes[0].id.clone()),
                ..code("qr", MARKER, 0)
            },
            code("code128", "ABC-123", 1),
        ]),
        ..PassPatch::default()
    };
    assert_clean("the Debug of a patch", &format!("{patch:?}"));
    let edited = service
        .pass_update(
            &user,
            id.clone(),
            created.header.updated_at.clone().expect("token"),
            patch,
        )
        .await
        .expect("update");

    // The user's own reads.
    assert_clean(
        "the overview",
        &format!(
            "{:?}",
            service.load_overview(&user).await.expect("overview")
        ),
    );
    assert_clean(
        "the list of all types",
        &format!(
            "{:?}",
            service
                .list_headers_of(&user, &[], Kinds::Any)
                .await
                .expect("list")
        ),
    );
    assert_clean(
        "the entry",
        &format!(
            "{:?}",
            service.get_item(&user, id.clone()).await.expect("get")
        ),
    );
    assert_clean("the edited pass", &format!("{edited:?}"));
    let stands = service
        .history_list(&user, id.clone())
        .await
        .expect("history");
    assert_eq!(stands.len(), 2);
    assert_clean("the history list", &format!("{stands:?}"));
    for stand in &stands {
        let view = service
            .history_get(&user, stand.id.clone())
            .await
            .expect("a state");
        assert_clean("the view of a state", &format!("{view:?}"));
        assert_clean(
            "the view of a state as sent to the window",
            &serde_json::to_string(&view).expect("json"),
        );
    }
    // A revealed code is the one thing a state gives, on request.
    let newest = &stands[0];
    let revealed = service
        .history_reveal(&user, newest.id.clone(), HistorySecret::Code { index: 0 })
        .await
        .expect("reveal");
    assert_eq!(revealed.value.as_str(), MARKER);

    // The drawing is not the text.
    let drawn = service
        .pass_code_svg(&user, id.clone(), codes[0].id.clone())
        .await
        .expect("svg");
    assert_clean("the drawing", &format!("{drawn:?}"));

    // A code that cannot be drawn comes back as its text; that text goes to the user's screen only,
    // and `Debug` of the answer prints none of it.
    let flagged = service
        .pass_create(
            &user,
            PassInput {
                codes: vec![code("code128", MARKER, 0)],
                ..PassInput::default()
            },
            None,
        )
        .await
        .expect("a second pass");
    let flagged_codes = service
        .pass_codes(&user, flagged.header.id.clone())
        .await
        .expect("codes");
    db.write(|tx| {
        tx.execute(
            "UPDATE haex_passwords_pass_barcodes SET flag = 'invalid_value' WHERE id = ?1",
            &[&flagged_codes[0].id as &dyn haex_crdt::rusqlite::ToSql],
        )?;
        Ok(())
    })
    .expect("flag it");
    let as_text = service
        .pass_code_svg(
            &user,
            flagged.header.id.clone(),
            flagged_codes[0].id.clone(),
        )
        .await
        .expect("text");
    assert_eq!(as_text.text.as_deref(), Some(MARKER), "the screen gets it");
    assert_clean("the Debug of a code shown as text", &format!("{as_text:?}"));

    // The agents: the built-in one with its narrow rows, an external one with a grant.
    let grants = [Grant::new(GrantAction::Read, Scope::All)];
    for (caller, grants) in [
        (Caller::BuiltinAgent, Vec::new()),
        (
            Caller::ExternalAgent {
                id: "agent".to_owned(),
            },
            grants.to_vec(),
        ),
    ] {
        let found = service
            .agent_search_of(&caller, &grants, None, None, None, Kinds::Any)
            .await
            .expect("search");
        assert_clean("the agent search", &format!("{found:?}"));
        let listed = service.list_headers_of(&caller, &grants, Kinds::Any).await;
        assert_clean("the list of an outside caller", &format!("{listed:?}"));
        let secret = service.read_secret_item(&caller, &grants, id.clone()).await;
        assert!(
            secret.is_err(),
            "an outside caller reads no pass as a secret"
        );
        assert_clean("the error of a secret read", &format!("{secret:?}"));
    }

    // Refused calls: the error holds the position and the reason, never the value.
    let refused = service
        .pass_create(
            &user,
            PassInput {
                codes: vec![code("ean13", WRONG_MARKER, 2)],
                ..PassInput::default()
            },
            None,
        )
        .await
        .expect_err("a code that does not fit");
    assert_clean(
        "the error of a refused code",
        &format!("{refused:?} {refused}"),
    );
    assert_clean(
        "the error as sent to the window",
        &serde_json::to_string(&refused).expect("json"),
    );
    let conflict = service
        .pass_update(
            &user,
            id,
            "stale".to_owned(),
            PassPatch {
                codes: Some(vec![code("qr", MARKER, 0)]),
                ..PassPatch::default()
            },
        )
        .await
        .expect_err("a stale token");
    assert_clean(
        "the error of a conflict",
        &format!("{conflict:?} {conflict}"),
    );

    // Nothing in the log of the whole run.
    let lines = LINES.lock().expect("lines");
    assert_clean("the log", &lines.join("\n"));
}
