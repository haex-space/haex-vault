//! Spec 021 US5 (T072): passwords for external agents. With the group "Lesen, Tag dev" the search
//! lists only dev entries; `passwords.secret.get` asks in every approval mode, Auto and Plan
//! included, says that the secret goes to the agent and its provider, and returns it only after
//! "allow". Without the group the tools are missing and a forced call says nothing about the
//! entry. The grant comes from the agent's rows in `haex_mcp_agent_grants`, as in the app.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use holzi_lib::caller::Caller;
use holzi_lib::chat::tools::action_tool::AgentActionDef;
use holzi_lib::error::HolziError;
use holzi_lib::mcp_server::grants::{default_rights, PasswordLevel, PasswordRights, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::prompts::CallDecision;
use holzi_lib::mcp_server::registry::{self, AgentMode};
use holzi_lib::passwords::access::Grant;
use holzi_lib::passwords::agent_actions::{PasswordsActions, PasswordsAgentEnv};
use holzi_lib::passwords::model::ItemInput;
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::vault_gate::VaultDb;
use mcp_fixture::*;

/// The vault of the test; the grants of an agent from its rights, as `mcp_server::password_env`.
struct Env {
    db: VaultDb,
    device: Uuid,
}

#[async_trait]
impl PasswordsAgentEnv for Env {
    fn service(&self) -> Result<PasswordsService, HolziError> {
        Ok(PasswordsService::new(self.db.clone()))
    }

    async fn grants(&self, agent_id: &str) -> Vec<Grant> {
        let (Ok(agent), device) = (Uuid::parse_str(agent_id), self.device) else {
            return Vec::new();
        };
        self.db
            .read(move |q| registry::load_rights(q, device, agent))
            .await
            .ok()
            .and_then(|rights| rights.passwords)
            .map(|passwords| passwords.grants())
            .unwrap_or_default()
    }
}

fn native(id: &str, scope: &str) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": "A password action.",
        "inputSchema": { "type": "object" },
        "effect": "read",
        "scope": scope,
        "runner": "native",
        "builtinAgentCallable": false,
    }))
    .expect("definition")
}

/// The ids of a `dev` entry and a `bank` entry.
async fn setup(fx: &Fixture) -> (String, String) {
    fx.native.add(Arc::new(PasswordsActions::new(Env {
        db: fx.db.clone(),
        device: fx.device,
    })));
    let mut catalog = test_catalog();
    catalog.extend([
        native("passwords.items.search", "passwords.read"),
        native("passwords.secret.get", "passwords.secret"),
    ]);
    fx.catalog.replace(catalog);
    let service = PasswordsService::new(fx.db.clone());
    let mut ids = Vec::new();
    for (title, tag) in [("Git", "dev"), ("Bank", "bank")] {
        let id = service
            .create_item(
                &Caller::User,
                &[],
                ItemInput {
                    title: Some(title.to_owned()),
                    password: Some(format!("secret-of-{title}")),
                    tags: vec![tag.to_owned()],
                    ..ItemInput::default()
                },
                None,
            )
            .await
            .expect("entry");
        ids.push(id);
    }
    (ids[0].clone(), ids[1].clone())
}

fn dev_reader() -> Rights {
    Rights {
        passwords: Some(PasswordRights {
            level: PasswordLevel::Read,
            tags: Some(vec!["dev".to_owned()]),
        }),
        ..default_rights()
    }
}

fn secret_of(entry: &str) -> Value {
    json!({ "entryId": entry, "field": "password" })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_search_lists_only_entries_of_the_granted_tag() {
    let mut fx = fixture();
    let (dev, _bank) = setup(&fx).await;
    let server = fx.server().await;
    let (agent, _helper) = fx.registered("claude-code", dev_reader()).await;

    let found = call_with(agent.peer(), "passwords_items_search", json!({})).await;
    assert_eq!(error_code(&found), None);
    assert_eq!(
        found.structured_content,
        Some(
            json!({ "items": [{ "id": dev, "title": "Git", "tags": ["dev"], "hasTotp": false }] })
        )
    );
    server.stop(CloseReason::Disabled).await;
}

/// FR-019, FR-021: every retrieval asks, in Auto and in Plan, and the prompt warns about the
/// provider; only "allow" hands the secret out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_secret_asks_in_every_mode_and_comes_only_after_allow() {
    for (slug, mode) in [
        ("auto-agent", AgentMode::Auto),
        ("plan-agent", AgentMode::Plan),
    ] {
        let mut fx = fixture();
        let (dev, bank) = setup(&fx).await;
        let server = fx.server().await;
        let (agent, _helper) = fx.registered_as(slug, dev_reader(), mode).await;

        let asking = tokio::spawn({
            let peer = agent.peer().clone();
            let input = secret_of(&dev);
            async move { call_with(&peer, "passwords_secret_get", input).await }
        });
        let (id, question) = fx.next_call().await;
        assert!(
            question.secret,
            "{mode:?}: the prompt warns about the provider"
        );
        assert_eq!(question.action_id, "passwords.secret.get");
        fx.answer_call(&id, CallDecision::Allow);
        let allowed = asking.await.expect("the call");
        assert_eq!(error_code(&allowed), None, "{mode:?}");
        assert_eq!(
            allowed.structured_content,
            Some(json!({ "value": "secret-of-Git" }))
        );

        let declining = tokio::spawn({
            let peer = agent.peer().clone();
            let input = secret_of(&dev);
            async move { call_with(&peer, "passwords_secret_get", input).await }
        });
        let (id, _) = fx.next_call().await;
        fx.answer_call(&id, CallDecision::Deny);
        let declined = declining.await.expect("the call");
        assert_eq!(error_code(&declined).as_deref(), Some("user_declined"));

        // Outside the tag: after "allow" the same answer as a missing entry.
        let mut answers = Vec::new();
        for entry in [bank.as_str(), "no-such-entry"] {
            let asking = tokio::spawn({
                let peer = agent.peer().clone();
                let input = secret_of(entry);
                async move { call_with(&peer, "passwords_secret_get", input).await }
            });
            let (id, _) = fx.next_call().await;
            fx.answer_call(&id, CallDecision::Allow);
            let result = asking.await.expect("the call");
            answers.push(result.structured_content);
        }
        assert_eq!(answers[0], answers[1], "{mode:?}: nothing about the entry");
        server.stop(CloseReason::Disabled).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_the_group_nothing_is_offered_and_nothing_is_said_about_an_entry() {
    let mut fx = fixture();
    let (dev, _bank) = setup(&fx).await;
    let server = fx.server().await;
    let (agent, _helper) = fx.registered("claude-code", default_rights()).await;

    let names = tool_names(&agent).await;
    assert!(!names.iter().any(|name| name.starts_with("passwords_")));
    for (tool, input) in [
        ("passwords_items_search", json!({})),
        ("passwords_secret_get", secret_of(&dev)),
        ("passwords_secret_get", secret_of("no-such-entry")),
    ] {
        assert_eq!(
            error_code(&call_with(agent.peer(), tool, input).await).as_deref(),
            Some("not_allowed"),
            "{tool}"
        );
    }
    fx.no_call_prompt().await;
    server.stop(CloseReason::Disabled).await;
}
