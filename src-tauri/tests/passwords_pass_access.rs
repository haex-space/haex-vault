//! Spec 092 (cards and tickets), FR-024, FR-027, FR-027a, SC-004, SC-005: with a pass in the vault
//! no credential reader of any caller from outside returns it, however well its title, address and
//! tag match a login, a pass is not readable by id as a secret, the agent search asks for the type,
//! and an entry of a type this build does not know is not offered either. The user's own overview
//! lists both, and says what each is.

// These tests write raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use url::Url;

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::{Caller, Grant, GrantAction, Scope};
use holzi_lib::passwords::kind::{EntryType, Kinds};
use holzi_lib::passwords::model::{ItemInput, KeyValueInput};
use holzi_lib::passwords::service::{Headers, PasswordsService};
use holzi_lib::vault_gate::VaultGate;
use holzi_lib::HolziError;

const MARKER: &str = "SECRET-MARKER-pass-code";

struct Fixture {
    _dir: tempfile::TempDir,
    db: Database,
    service: PasswordsService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-pass-access"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let vault_db = VaultGate::new()
        .vault_db(Arc::new(db.clone()))
        .expect("open the gate");
    Fixture {
        _dir: dir,
        db,
        service: PasswordsService::new(vault_db),
    }
}

fn entry(title: &str) -> ItemInput {
    ItemInput {
        title: Some(title.to_string()),
        username: Some("alice".to_string()),
        url: Some("https://shop.example.invalid".to_string()),
        password: Some("login-password".to_string()),
        otp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
        tags: vec!["Shop".to_string()],
        key_values: vec![KeyValueInput {
            key: "Seat".to_string(),
            value: Some("14C".to_string()),
        }],
        ..ItemInput::default()
    }
}

/// A login and a twin with the same title, address and tag that is turned into `kind` with a detail
/// row and a code carrying the marker (the pass service comes with the next slice).
async fn seed(f: &Fixture, kind: &str) -> (String, String) {
    let login = f
        .service
        .create_item(&Caller::User, &[], entry("Shop"), None)
        .await
        .expect("login");
    let twin = f
        .service
        .create_item(&Caller::User, &[], entry("Shop"), None)
        .await
        .expect("twin");
    f.db.with_connection(|conn| {
        conn.execute(
            "UPDATE haex_passwords_item_details SET kind = ?2, password = NULL, username = NULL, \
             otp_secret = NULL WHERE id = ?1",
            haex_crdt::rusqlite::params![twin, kind],
        )?;
        conn.execute(
            "INSERT INTO haex_passwords_pass_details (item_id, pass_kind, start_date) \
             VALUES (?1, 'ticket', '2030-05-01')",
            haex_crdt::rusqlite::params![twin],
        )?;
        conn.execute(
            "INSERT INTO haex_passwords_pass_barcodes (id, item_id, format, value) \
             VALUES ('code-1', ?1, 'qr', ?2)",
            haex_crdt::rusqlite::params![twin, MARKER],
        )?;
        Ok(())
    })
    .expect("turn the twin into a pass");
    (login, twin)
}

fn outside_callers() -> Vec<Caller> {
    vec![
        Caller::Extension {
            id: "ext".to_string(),
        },
        Caller::ExternalAgent {
            id: "agent".to_string(),
        },
        Caller::Internal { feature: "test" },
        Caller::BrowserClient {
            id: "3f1c0000000000000000000000000000".to_string(),
        },
    ]
}

fn all_grants() -> Vec<Grant> {
    vec![Grant::new(GrantAction::ReadWrite, Scope::All)]
}

fn ids(headers: Headers) -> Vec<String> {
    match headers {
        Headers::Items(items) => items.into_iter().map(|h| h.id).collect(),
        Headers::Agent(items) => items.into_iter().map(|h| h.id).collect(),
    }
}

#[tokio::test]
async fn no_credential_reader_returns_a_pass_that_matches_a_login() {
    let f = fixture();
    let (login, pass) = seed(&f, "pass").await;
    let page = Url::parse("https://shop.example.invalid/signin").expect("url");
    let mut callers = outside_callers();
    callers.push(Caller::BuiltinAgent);
    callers.push(Caller::User);

    for caller in &callers {
        let grants = all_grants();
        let listed = ids(f.service.list_headers(caller, &grants).await.expect("list"));
        assert!(listed.contains(&login), "{caller:?} still sees the login");
        assert!(!listed.contains(&pass), "{caller:?} must not list a pass");

        // The user's own window must be served, so a failing reader cannot pass unnoticed; a
        // caller a reader is closed to is allowed to fail.
        let by_site = f.service.headers_for_page(caller, &grants, &page).await;
        if matches!(caller, Caller::User) {
            let found: Vec<_> = by_site
                .as_ref()
                .expect("autofill by site serves the user")
                .iter()
                .map(|h| h.id.clone())
                .collect();
            assert!(found.contains(&login), "the user finds the login by site");
        }
        if let Ok(headers) = by_site {
            let found: Vec<_> = headers.into_iter().map(|h| h.id).collect();
            assert!(!found.contains(&pass), "{caller:?}: autofill by site");
        }
        let shapes = f.service.list_autofill_shapes(caller, &grants).await;
        if matches!(caller, Caller::User) {
            let shapes = shapes.as_ref().expect("autofill shapes serve the user");
            assert!(
                shapes.iter().any(|s| s.id == login),
                "the user gets the shape of the login"
            );
        }
        if let Ok(shapes) = shapes {
            assert!(
                shapes.iter().all(|s| s.id != pass),
                "{caller:?}: autofill shapes"
            );
        }
    }
}

#[tokio::test]
async fn a_pass_is_not_a_secret_by_id_for_anyone_from_outside() {
    let f = fixture();
    let (login, pass) = seed(&f, "pass").await;
    for caller in outside_callers() {
        let grants = all_grants();
        assert!(
            f.service
                .read_secret_item(&caller, &grants, login.clone())
                .await
                .is_ok(),
            "{caller:?} reads the login"
        );
        let secret = f
            .service
            .read_secret_item(&caller, &grants, pass.clone())
            .await;
        assert!(
            matches!(secret, Err(HolziError::PasswordsNotFound)),
            "{caller:?}: a pass is not found by id"
        );
        let code = f.service.totp_code(&caller, &grants, pass.clone()).await;
        assert!(
            matches!(code, Err(HolziError::PasswordsNotFound)),
            "{caller:?}: no totp for a pass"
        );
        let delete = f.service.delete_item(&caller, &grants, pass.clone()).await;
        assert!(
            matches!(delete, Err(HolziError::PasswordsNotFound)),
            "{caller:?}: a pass is not deleted from outside"
        );
    }
}

#[tokio::test]
async fn the_agent_search_asks_for_the_type_and_a_row_carries_its_type_and_no_code() {
    let f = fixture();
    let (login, pass) = seed(&f, "pass").await;
    let agent = Caller::BuiltinAgent;
    let external = Caller::ExternalAgent {
        id: "agent".to_string(),
    };

    let default = f
        .service
        .agent_search(&agent, &[], None, None, None)
        .await
        .expect("search");
    let default_ids: Vec<_> = default.iter().map(|h| h.id.clone()).collect();
    assert_eq!(default_ids, vec![login.clone()], "logins are the default");
    assert_eq!(default[0].kind, EntryType::Login);

    let passes = f
        .service
        .agent_search_of(&agent, &[], None, None, None, Kinds::Passes)
        .await
        .expect("search passes");
    assert_eq!(passes.len(), 1);
    assert_eq!(passes[0].id, pass);
    assert_eq!(passes[0].kind, EntryType::Pass);
    assert_eq!(passes[0].pass_kind.as_deref(), Some("ticket"));
    assert_eq!(passes[0].start_date.as_deref(), Some("2030-05-01"));
    assert!(!passes[0].has_totp);
    assert!(!format!("{passes:?}").contains(MARKER), "no code in a row");

    let any = f
        .service
        .agent_search_of(&agent, &[], None, None, None, Kinds::Any)
        .await
        .expect("search any");
    assert_eq!(any.len(), 2);

    // An external agent with a tag grant sees the pass only because the tag is covered.
    let covered = vec![Grant::new(GrantAction::Read, Scope::tags(["Shop"]))];
    let seen = f
        .service
        .agent_search_of(&external, &covered, None, None, None, Kinds::Passes)
        .await
        .expect("covered");
    assert_eq!(seen.len(), 1);
    let other = vec![Grant::new(GrantAction::Read, Scope::tags(["Other"]))];
    let none = f
        .service
        .agent_search_of(&external, &other, None, None, None, Kinds::Passes)
        .await
        .expect("not covered");
    assert!(none.is_empty(), "a tag outside the grant hides the pass");
}

#[tokio::test]
async fn the_overview_of_the_user_names_the_type_of_each_entry_and_an_unknown_type_is_carried() {
    let f = fixture();
    let (login, pass) = seed(&f, "pass").await;
    let overview = f
        .service
        .load_overview(&Caller::User)
        .await
        .expect("overview");
    let kind_of = |id: &str| {
        overview
            .headers
            .iter()
            .find(|h| h.id == id)
            .map(|h| h.kind.clone())
    };
    assert_eq!(kind_of(&login), Some(EntryType::Login));
    assert_eq!(kind_of(&pass), Some(EntryType::Pass));
    let header = overview
        .headers
        .iter()
        .find(|h| h.id == pass)
        .expect("pass");
    assert_eq!(header.pass_kind.as_deref(), Some("ticket"));
    assert!(
        !format!("{overview:?}").contains(MARKER),
        "no code in the overview"
    );

    let newer = fixture();
    let (_, twin) = seed(&newer, "payment_card").await;
    let overview = newer
        .service
        .load_overview(&Caller::User)
        .await
        .expect("overview");
    let header = overview
        .headers
        .iter()
        .find(|h| h.id == twin)
        .expect("twin");
    assert_eq!(header.kind, EntryType::Unknown("payment_card".to_string()));
    for caller in outside_callers() {
        let listed = ids(newer
            .service
            .list_headers(&caller, &all_grants())
            .await
            .expect("list"));
        assert!(
            !listed.contains(&twin),
            "{caller:?}: an unknown type is not offered"
        );
    }
}
