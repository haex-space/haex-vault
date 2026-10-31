//! Spec 021 US5 (T071, T077): files and contacts for external agents. Their tools follow the
//! agent's own rows of `agent_file_permissions`: none without a row, the reading ones with a read
//! row, and a change of the rows sends `tools/list_changed`. The contact actions run through the
//! real executor as the agent; a native action whose executor serves only the built-in agent is
//! never offered.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use holzi_lib::caller::Caller;
use holzi_lib::chat::tools::action_tool::AgentActionDef;
use holzi_lib::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use holzi_lib::contacts::agent_actions::{ContactsActions, ContactsAgentEnv};
use holzi_lib::contacts::model::{ContactInput, Head};
use holzi_lib::contacts::service::ContactsService;
use holzi_lib::error::HolziError;
use holzi_lib::files::agent::prompt::PermissionPrompt;
use holzi_lib::files::permissions::{self, AgentFileKind, AgentFilePermission, AgentFileStatus};
use holzi_lib::mcp_server::grants::default_rights;
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::vault_gate::VaultDb;
use mcp_fixture::*;

/// A file executor that serves external agents and records who called.
#[derive(Default)]
struct Files {
    callers: Mutex<Vec<Caller>>,
}

#[async_trait]
impl NativeExecutor for Files {
    fn action_ids(&self) -> &'static [&'static str] {
        &["files.list", "files.delete"]
    }

    fn serves_external_agents(&self) -> bool {
        true
    }

    async fn run(
        &self,
        action_id: &str,
        _input: Value,
        call: &NativeCall,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        self.callers
            .lock()
            .expect("callers")
            .push(call.caller.clone());
        Ok(json!({ "ran": action_id }).into())
    }
}

/// An executor of the built-in agent only, like the contact share.
struct BuiltinOnly;

#[async_trait]
impl NativeExecutor for BuiltinOnly {
    fn action_ids(&self) -> &'static [&'static str] {
        &["contacts.share"]
    }

    async fn run(
        &self,
        _action_id: &str,
        _input: Value,
        _call: &NativeCall,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        Ok(json!({}).into())
    }
}

/// The open vault and nobody to ask: a call without a row is refused.
struct Env(VaultDb);

impl ContactsAgentEnv for Env {
    fn database(&self) -> Result<VaultDb, HolziError> {
        Ok(self.0.clone())
    }

    fn prompt(&self) -> PermissionPrompt {
        PermissionPrompt::default()
    }
}

fn native(id: &str, scope: &str, effect: &str) -> AgentActionDef {
    serde_json::from_value(json!({
        "toolName": id.replace('.', "_"),
        "actionId": id,
        "description": "A native action.",
        "inputSchema": { "type": "object" },
        "effect": effect,
        "scope": scope,
        "runner": "native",
    }))
    .expect("definition")
}

const DATA_TOOLS: [&str; 6] = [
    "files_list",
    "files_delete",
    "contacts_search",
    "contacts_get",
    "contacts_update",
    "contacts_share",
];

fn setup(fx: &Fixture) -> Arc<Files> {
    let files = Arc::new(Files::default());
    fx.native.add(files.clone());
    fx.native
        .add(Arc::new(ContactsActions::new(Env(fx.db.clone()))));
    fx.native.add(Arc::new(BuiltinOnly));
    let mut catalog = test_catalog();
    catalog.extend([
        native("files.list", "files.read", "read"),
        native("files.delete", "files.write", "destructive"),
        native("contacts.search", "contacts.read", "read"),
        native("contacts.get", "contacts.read", "read"),
        native("contacts.update", "contacts.write", "write"),
        native("contacts.share", "contacts.read", "destructive"),
    ]);
    fx.catalog.replace(catalog);
    files
}

async fn grant(
    fx: &Fixture,
    agent_id: String,
    kind: AgentFileKind,
    target: &str,
    status: AgentFileStatus,
) {
    let row = AgentFilePermission {
        agent_id,
        kind,
        target: target.to_owned(),
        status,
        updated_at: 1,
        reach: None,
    };
    fx.db
        .write(move |tx| Ok(permissions::set(tx, &row)?))
        .await
        .expect("stored");
    // In the app the vault's observer announces the commit.
    let _ = fx
        .vault_changes
        .send(Arc::new(vec!["agent_file_permissions".to_owned()]));
}

fn data_tools(names: &[String]) -> Vec<&str> {
    names
        .iter()
        .map(String::as_str)
        .filter(|name| DATA_TOOLS.contains(name))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_row_no_file_or_contact_tool_is_offered_or_runs() {
    let mut fx = fixture();
    let files = setup(&fx);
    let server = fx.server().await;
    let (agent, _helper) = fx.registered("claude-code", default_rights()).await;

    assert!(data_tools(&tool_names(&agent).await).is_empty());
    for tool in DATA_TOOLS {
        assert_eq!(
            error_code(&call(agent.peer(), tool).await).as_deref(),
            Some("not_allowed"),
            "{tool}"
        );
    }
    assert!(files.callers.lock().expect("callers").is_empty());
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn its_own_rows_open_the_tools_and_the_calls_run_as_the_agent() {
    let mut fx = fixture();
    let files = setup(&fx);
    ContactsService::new(fx.db.clone())
        .create(
            &Caller::User,
            &[],
            ContactInput {
                head: Head {
                    given_name: Some("Ben".to_owned()),
                    ..Head::default()
                },
                ..ContactInput::default()
            },
        )
        .await
        .expect("a contact");
    let server = fx.server().await;
    let (agent, _helper) = fx.registered("claude-code", default_rights()).await;
    let id = fx.agent_id("claude-code").await;
    // Another agent's rows open nothing for this one.
    grant(
        &fx,
        "builtin".to_owned(),
        AgentFileKind::Device,
        "",
        AgentFileStatus::Granted,
    )
    .await;

    // "Dateien des Geräts": the file tools appear without a restart and run as the agent.
    let mut changes = agent.service().list_changes();
    grant(
        &fx,
        id.to_string(),
        AgentFileKind::Device,
        "",
        AgentFileStatus::Granted,
    )
    .await;
    next_list_change(&mut changes).await;
    assert_eq!(
        data_tools(&tool_names(&agent).await),
        ["files_delete", "files_list"]
    );
    assert_eq!(error_code(&call(agent.peer(), "files_list").await), None);
    assert_eq!(
        files.callers.lock().expect("callers").as_slice(),
        [Caller::ExternalAgent { id: id.to_string() }]
    );

    // Reading contacts: search and get, never update, and never the built-in agent's share.
    grant(
        &fx,
        id.to_string(),
        AgentFileKind::Contacts,
        "*",
        AgentFileStatus::Read,
    )
    .await;
    next_list_change(&mut changes).await;
    assert_eq!(
        data_tools(&tool_names(&agent).await),
        [
            "contacts_get",
            "contacts_search",
            "files_delete",
            "files_list"
        ]
    );
    let found = call_with(agent.peer(), "contacts_search", json!({ "query": "ben" })).await;
    assert_eq!(error_code(&found), None);
    let found = found.structured_content.expect("contacts");
    assert_eq!(found["contacts"][0]["displayName"], "Ben");
    for tool in ["contacts_update", "contacts_share"] {
        assert_eq!(
            error_code(&call(agent.peer(), tool).await).as_deref(),
            Some("not_allowed"),
            "{tool}"
        );
    }
    server.stop(CloseReason::Disabled).await;
}
