//! The contact actions as native actions (spec 051 US6, spec 021 US5): the input of
//! `contacts.update` as the service's patch, and each agent reaching the address book as itself
//! with its own rows of `agent_file_permissions`.

use std::time::Duration;

use serde_json::json;

use super::*;
use crate::chat::tools::permission::PermissionMode;
use crate::contacts::service::test_support::{person, test_vault, value, TestVault};
use crate::files::permissions::{self, AgentFileKind, AgentFilePermission, AgentFileStatus};
use crate::patch::Patch;

struct Env {
    db: VaultDb,
    prompt: PermissionPrompt,
}

impl ContactsAgentEnv for Env {
    fn database(&self) -> Result<VaultDb, HolziError> {
        Ok(self.db.clone())
    }

    fn prompt(&self) -> PermissionPrompt {
        self.prompt.clone()
    }
}

/// The executor over `vault` with nobody to ask.
fn actions(vault: &TestVault) -> ContactsActions<Env> {
    ContactsActions::new(Env {
        db: vault.db.clone(),
        prompt: PermissionPrompt::with_timeout(Duration::from_secs(5)),
    })
}

fn external(id: &str) -> NativeCall {
    NativeCall {
        caller: Caller::ExternalAgent { id: id.to_owned() },
        mode: PermissionMode::Auto,
    }
}

fn grant(vault: &TestVault, agent_id: &str, status: AgentFileStatus) {
    let row = AgentFilePermission {
        agent_id: agent_id.to_owned(),
        kind: AgentFileKind::Contacts,
        target: "*".to_owned(),
        status,
        updated_at: 1,
        reach: None,
    };
    vault
        .db
        .write_blocking(move |tx| Ok(permissions::set(tx, &row)?))
        .expect("stored");
}

#[test]
fn the_input_of_an_update_becomes_the_patch() {
    let patch = patch_of(&json!({
        "id": "c1",
        "givenName": "Ben",
        "note": "  ",
        "values": [
            { "id": "v1", "kind": "phone", "label": "work", "value": "0170 2" },
            { "kind": "email", "value": "ben@example.de" },
            { "kind": "phone" },
        ],
        "remove": ["v2", 7],
    }))
    .expect("patch");
    assert!(patch.given_name == Patch::Set("Ben".to_owned()));
    assert!(patch.note == Patch::Clear, "an empty text clears");
    assert!(patch.family_name == Patch::Keep, "an absent field stays");
    assert!(patch.groups.is_none(), "agents never change groups");
    let values = patch.values.expect("values");
    assert_eq!(values.remove, ["v2"]);
    let upsert: Vec<_> = values
        .upsert
        .iter()
        .map(|row| (row.id.as_deref(), row.label.as_str(), row.value.as_str()))
        .collect();
    assert_eq!(
        upsert,
        [
            (Some("v1"), "work", "0170 2"),
            (None, "other", "ben@example.de")
        ]
    );
    assert!(patch_of(&json!({ "id": "c1" }))
        .expect("patch")
        .values
        .is_none());
}

/// An external agent reads with its own grant; another external agent's grant does not count for
/// it, and with nobody to ask it gets nothing, which looks like a missing contact (051 FR-031).
#[tokio::test]
async fn an_external_agent_reaches_contacts_only_with_its_own_grant() {
    let vault = test_vault();
    vault
        .service
        .create(
            &Caller::User,
            &[],
            person("Ben", "Berg", vec![value("phone", "mobile", "0170 1")]),
        )
        .await
        .expect("created");
    grant(&vault, "agent-a", AgentFileStatus::Read);
    let actions = actions(&vault);
    let cancel = CancellationToken::new();

    let found = actions
        .run(
            "contacts.search",
            json!({ "query": "ben" }),
            &external("agent-a"),
            &cancel,
        )
        .await
        .expect("found");
    assert_eq!(found.result["contacts"][0]["displayName"], "Ben Berg");

    let refused = actions
        .run(
            "contacts.search",
            json!({ "query": "ben" }),
            &external("agent-b"),
            &cancel,
        )
        .await
        .expect_err("no grant of its own");
    assert_eq!(refused.code, "not_found");

    let id = found.result["contacts"][0]["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let unchanged = actions
        .run(
            "contacts.update",
            json!({ "id": id, "note": "x" }),
            &external("agent-a"),
            &cancel,
        )
        .await
        .expect_err("reading only");
    assert_eq!(unchanged.code, "not_found");
}

#[tokio::test]
async fn a_caller_that_is_no_agent_gets_nothing() {
    let vault = test_vault();
    let call = NativeCall {
        caller: Caller::Extension { id: "x".to_owned() },
        mode: PermissionMode::Auto,
    };
    let refused = actions(&vault)
        .run(
            "contacts.search",
            json!({ "query": "" }),
            &call,
            &CancellationToken::new(),
        )
        .await
        .expect_err("refused");
    assert_eq!(refused.code, "forbidden_for_agents");
}
