//! The contact actions as native actions (spec 051 US6, spec 021 US5): the input of
//! `contacts.update` as the service's patch, and each agent reaching the address book as itself
//! with the contacts group of its agent profile (spec 083 FR-032).

use std::time::Duration;

use serde_json::json;

use super::*;
use crate::chat::tools::permission::PermissionMode;
use crate::contacts::service::test_support::{person, test_vault, value};
use crate::contacts::service::ContactsService;
use crate::patch::Patch;

#[test]
fn internal_failures_do_not_expose_contact_storage_details() {
    for error in [
        HolziError::CrdtSqlite {
            reason: "private database detail".to_owned(),
        },
        HolziError::CrdtIo {
            reason: "private storage detail".to_owned(),
        },
        HolziError::PathResolution {
            reason: "private path detail".to_owned(),
        },
    ] {
        let error = contacts_error(error);
        assert_eq!(error.code, "failed");
        assert_eq!(error.message, "the contact action failed");
    }
}

struct Env {
    db: VaultDb,
    device: uuid::Uuid,
    prompt: PermissionPrompt,
}

impl ContactsAgentEnv for Env {
    fn database(&self) -> Result<VaultDb, HolziError> {
        Ok(self.db.clone())
    }

    fn device(&self) -> BoxFuture<'_, Result<uuid::Uuid, HolziError>> {
        Box::pin(async move { Ok(self.device) })
    }

    fn prompt(&self) -> PermissionPrompt {
        self.prompt.clone()
    }
}

fn external(id: &str) -> NativeCall {
    NativeCall {
        caller: Caller::ExternalAgent { id: id.to_owned() },
        mode: PermissionMode::Auto,
    }
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

/// An external agent reads with the grant of its profile; an agent in another profile does not get
/// it, and with nobody to ask it gets nothing, which looks like a missing contact (051 FR-031).
#[tokio::test]
async fn an_external_agent_reaches_contacts_only_with_the_grant_of_its_profile() {
    use crate::agent_profiles::{store, test_support, Subject};
    use crate::mcp_server::grants::{AccessLevel, Rights};

    let v = test_support::vault();
    let service = ContactsService::new(v.db.clone());
    service
        .create(
            &Caller::User,
            &[],
            person("Ben", "Berg", vec![value("phone", "mobile", "0170 1")]),
        )
        .await
        .expect("created");
    let (agent_a, agent_b) = (
        test_support::registered(&v, "agent-a").await,
        test_support::registered(&v, "agent-b").await,
    );
    let profile = uuid::Uuid::new_v4();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Kontakte lesen")?;
        let mut rights = Rights::default();
        rights.contacts.all = Some(AccessLevel::Read);
        store::set_rights(tx, profile, &rights)?;
        store::add_member(tx, &Subject::Agent(agent_a), profile)?;
        store::set_active(tx, profile)?;
        Ok(())
    })
    .await
    .expect("profile");
    let actions = ContactsActions::new(Env {
        db: v.db.clone(),
        device: v.device,
        prompt: PermissionPrompt::with_timeout(Duration::from_secs(5)),
    });
    let cancel = CancellationToken::new();

    let found = actions
        .run(
            "contacts.search",
            json!({ "query": "ben" }),
            &external(&agent_a.to_string()),
            &cancel,
        )
        .await
        .expect("found");
    assert_eq!(found.result["contacts"][0]["displayName"], "Ben Berg");

    let refused = actions
        .run(
            "contacts.search",
            json!({ "query": "ben" }),
            &external(&agent_b.to_string()),
            &cancel,
        )
        .await
        .expect_err("not in the profile that grants it");
    assert_eq!(refused.code, "not_found");

    let id = found.result["contacts"][0]["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let unchanged = actions
        .run(
            "contacts.update",
            json!({ "id": id, "note": "x" }),
            &external(&agent_a.to_string()),
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
    let refused = ContactsActions::new(Env {
        db: vault.db.clone(),
        device: uuid::Uuid::nil(),
        prompt: PermissionPrompt::with_timeout(Duration::from_secs(5)),
    })
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
