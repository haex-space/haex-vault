//! The password actions of agents (spec 034 FR-027, spec 021 US5, research R13): the search per
//! caller, the single secret only for an external agent within its grant, and a refusal that reads
//! like a missing entry (FR-029).

use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::chat::tools::permission::PermissionMode;
use crate::passwords::access::{GrantAction, Scope};
use crate::passwords::model::{ItemInput, KeyValueInput};
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::{VaultDb, VaultGate};

struct Env {
    db: VaultDb,
    grants: Vec<Grant>,
}

#[async_trait]
impl PasswordsAgentEnv for Env {
    fn service(&self) -> Result<PasswordsService, HolziError> {
        Ok(PasswordsService::new(self.db.clone()))
    }

    async fn grants(&self, agent_id: &str) -> Vec<Grant> {
        if agent_id == "claude" {
            self.grants.clone()
        } else {
            Vec::new()
        }
    }
}

struct Vault {
    _dir: tempfile::TempDir,
    db: VaultDb,
    dev: String,
    bank: String,
}

/// One entry tagged `dev` with a password, a TOTP and a custom field, one tagged `bank`.
async fn vault() -> Vault {
    let (dir, db) = open_test_vault();
    let db = VaultGate::new().vault_db(Arc::new(db)).expect("vault");
    let service = PasswordsService::new(db.clone());
    let dev = service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Git".to_owned()),
                username: Some("ada".to_owned()),
                password: Some("hunter2".to_owned()),
                otp_secret: Some("JBSWY3DPEHPK3PXP".to_owned()),
                tags: vec!["dev".to_owned()],
                key_values: vec![KeyValueInput {
                    key: "pin".to_owned(),
                    value: Some("1234".to_owned()),
                }],
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("dev entry");
    let bank = service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Bank".to_owned()),
                password: Some("money".to_owned()),
                tags: vec!["bank".to_owned()],
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("bank entry");
    Vault {
        _dir: dir,
        db,
        dev,
        bank,
    }
}

fn actions(vault: &Vault) -> PasswordsActions<Env> {
    PasswordsActions::new(Env {
        db: vault.db.clone(),
        grants: vec![Grant::new(GrantAction::Read, Scope::tags(["dev"]))],
    })
}

fn call(caller: Caller) -> NativeCall {
    NativeCall {
        caller,
        mode: PermissionMode::Auto,
    }
}

fn claude() -> NativeCall {
    call(Caller::ExternalAgent {
        id: "claude".to_owned(),
    })
}

async fn run(
    actions: &PasswordsActions<Env>,
    action: &str,
    input: Value,
    call: &NativeCall,
) -> Result<Value, NativeError> {
    actions
        .run(action, input, call, &CancellationToken::new())
        .await
        .map(|reply| reply.result)
}

#[test]
fn the_field_names_a_password_a_code_or_a_custom_field() {
    assert_eq!(SecretField::parse("password"), Some(SecretField::Password));
    assert_eq!(SecretField::parse("totp"), Some(SecretField::Totp));
    assert_eq!(
        SecretField::parse("custom:pin"),
        Some(SecretField::Custom("pin".to_owned()))
    );
    for wrong in ["", "custom:", "custom: ", "username", "note"] {
        assert_eq!(SecretField::parse(wrong), None, "{wrong}");
    }
}

#[test]
fn internal_password_failures_do_not_reveal_vault_details() {
    for error in [
        HolziError::CrdtIo {
            reason: "private-vault.db could not be read".to_owned(),
        },
        HolziError::CrdtSqlite {
            reason: "SELECT secret FROM private_table".to_owned(),
        },
    ] {
        let failure = passwords_error(error);
        assert_eq!(failure.code, "failed");
        assert_eq!(failure.message, "The password action failed.");
        assert!(failure.field.is_none());
    }
    for error in [
        HolziError::PasswordsNotFound,
        HolziError::PasswordsForbidden,
    ] {
        let failure = passwords_error(error);
        assert_eq!(failure.code, "not_found");
        assert_eq!(failure.message, no_entry().message);
    }
}

/// US5 scenario 2: the grant "Lesen, Tag dev" lists only the dev entries, with its fields (spec 092: and the kind).
#[tokio::test]
async fn an_external_agent_searches_within_its_tags_and_the_built_in_agent_everything() {
    let vault = vault().await;
    let actions = actions(&vault);
    let found = run(&actions, "passwords.items.search", json!({}), &claude())
        .await
        .expect("found");
    assert_eq!(
        found,
        json!({ "items": [{ "id": vault.dev, "title": "Git", "tags": ["dev"], "hasTotp": true, "kind": "login" }] })
    );
    let builtin = run(
        &actions,
        "passwords.items.search",
        json!({}),
        &call(Caller::BuiltinAgent),
    )
    .await
    .expect("all");
    assert_eq!(builtin["items"].as_array().map(Vec::len), Some(2));
    let stranger = run(
        &actions,
        "passwords.items.search",
        json!({}),
        &call(Caller::ExternalAgent {
            id: "other".to_owned(),
        }),
    )
    .await
    .expect_err("no grant");
    assert_eq!(stranger.code, "not_found");
}

/// Spec 092, FR-027a: the search asks for the type, logins are the default, a value that is none of
/// the three is refused by field, and a row of a pass carries its kind and start but never a code.
#[tokio::test]
async fn the_search_takes_a_type_and_a_pass_row_carries_its_start_and_no_code() {
    let vault = vault().await;
    let actions = actions(&vault);
    let builtin = call(Caller::BuiltinAgent);
    let refused = run(
        &actions,
        "passwords.items.search",
        json!({ "type": "card" }),
        &builtin,
    )
    .await
    .expect_err("an unknown type");
    assert_eq!(refused.code, "invalid_input");
    assert_eq!(refused.field.as_deref(), Some("type"));
    for asked in ["login", "pass", "any"] {
        let reply = run(
            &actions,
            "passwords.items.search",
            json!({ "type": asked }),
            &builtin,
        )
        .await
        .expect("a known type");
        let count = reply["items"].as_array().map(Vec::len).unwrap_or(0);
        assert_eq!(count, if asked == "pass" { 0 } else { 2 }, "{asked}");
    }

    let header = header_json(AgentHeader {
        id: "p1".to_owned(),
        title: Some("Concert".to_owned()),
        tags: vec!["Ticket".to_owned()],
        folder: Some("Tickets".to_owned()),
        has_totp: false,
        kind: crate::passwords::kind::EntryType::Pass,
        pass_kind: Some("ticket".to_owned()),
        start_date: None,
        start_at: Some("2030-05-01T19:00:00.000Z".to_owned()),
        start_zone: Some("Europe/London".to_owned()),
        expires_at: Some("2030-05-02".to_owned()),
    });
    assert_eq!(
        header,
        json!({
            "id": "p1", "title": "Concert", "tags": ["Ticket"], "folder": "Tickets",
            "hasTotp": false, "kind": "pass", "passKind": "ticket",
            "start": { "at": "2030-05-01T19:00:00.000Z", "zone": "Europe/London" },
            "expiresAt": "2030-05-02"
        })
    );
}

/// FR-019, 034 FR-029: the secret of an entry in the grant; outside it the same answer as for a
/// missing entry.
#[tokio::test]
async fn a_secret_comes_only_from_an_entry_in_the_grant() {
    let vault = vault().await;
    let actions = actions(&vault);
    let secret = |field: &str, entry: &str| json!({ "entryId": entry, "field": field });
    assert_eq!(
        run(
            &actions,
            "passwords.secret.get",
            secret("password", &vault.dev),
            &claude()
        )
        .await
        .expect("password"),
        json!({ "value": "hunter2" })
    );
    assert_eq!(
        run(
            &actions,
            "passwords.secret.get",
            secret("custom:pin", &vault.dev),
            &claude()
        )
        .await
        .expect("custom field"),
        json!({ "value": "1234" })
    );
    let code = run(
        &actions,
        "passwords.secret.get",
        secret("totp", &vault.dev),
        &claude(),
    )
    .await
    .expect("code");
    assert_eq!(code["code"].as_str().map(str::len), Some(6));

    let outside = run(
        &actions,
        "passwords.secret.get",
        secret("password", &vault.bank),
        &claude(),
    )
    .await
    .expect_err("outside the grant");
    let missing = run(
        &actions,
        "passwords.secret.get",
        secret("password", "nope"),
        &claude(),
    )
    .await
    .expect_err("missing");
    assert_eq!(
        (outside.code, outside.message),
        (missing.code, missing.message)
    );
}

#[tokio::test]
async fn the_built_in_agent_and_other_callers_never_get_a_secret() {
    let vault = vault().await;
    let actions = actions(&vault);
    for caller in [
        Caller::BuiltinAgent,
        Caller::User,
        Caller::Extension { id: "x".to_owned() },
    ] {
        let refused = run(
            &actions,
            "passwords.secret.get",
            json!({ "entryId": vault.dev, "field": "password" }),
            &call(caller),
        )
        .await
        .expect_err("refused");
        assert_eq!(refused.code, "forbidden_for_agents");
    }
    let wrong = run(
        &actions,
        "passwords.secret.get",
        json!({ "entryId": vault.dev, "field": "username" }),
        &claude(),
    )
    .await
    .expect_err("no such field kind");
    assert_eq!(wrong.code, "invalid_input");
}
