//! Spec 021 US4 (T066): the tools of extensions for external agents. The test extension
//! `agent-tools.xt` is installed and its tools are published as the chat registered them; a second
//! extension "Mail" offers a copy of one tool. Only granted extensions reach an agent, with the
//! same name, description and schema as for the built-in agent; a tool the user set to "ask" asks
//! even in Auto; a removed extension's tool is `unavailable` after `tools/list_changed`.

#[path = "common/mcp_fixture.rs"]
mod mcp_fixture;

#[path = "common/extension_frame.rs"]
mod extension_frame;

use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use extension_frame::{choice, install_agent_tools, InMemoryFrame};
use holzi_lib::chat::tools::{Tool, ToolRegistry};
use holzi_lib::extensions::agent_tools::defs::{load, ExtensionToolDef};
use holzi_lib::extensions::agent_tools::registration::register_extension_tools;
use holzi_lib::extensions::agent_tools::tool::Links;
use holzi_lib::extensions::bridge::frames::FrameRegistry;
use holzi_lib::mcp_server::grants::{default_rights, Rights};
use holzi_lib::mcp_server::hello::CloseReason;
use holzi_lib::mcp_server::manage_commands::set_rights;
use holzi_lib::mcp_server::prompts::CallDecision;
use mcp_fixture::*;

const COUNT: &str = "x_agenttools_count_entries";
const ADD: &str = "x_agenttools_add_entry";
const MAIL_COUNT: &str = "x_mail_count_entries";

/// The published tools of both extensions, and the id of the installed one.
struct Extensions {
    installed: Uuid,
    tools: Vec<Arc<dyn Tool>>,
}

/// Installs the test extension with `count_entries` confirmed and `add_entry` on "ask", and
/// publishes its tools and those of "Mail" to the server the way the chat registers them.
async fn publish(fx: &Fixture) -> Extensions {
    install_agent_tools(
        &fx.db,
        fx.device,
        vec![
            choice("agentTool", "read", "count_entries", true),
            choice("agentTool", "change", "add_entry", false),
            choice("agentTool", "read", "slow_echo", true),
            choice("notifications", "show", "*", false),
        ],
    )
    .await;
    let device = fx.device;
    let mut defs = fx
        .db
        .read(move |q| load(q, device).map_err(Into::into))
        .await
        .expect("the extension tools");
    let installed = defs
        .first()
        .expect("the test extension offers tools")
        .extension_id;
    let mail = ExtensionToolDef {
        extension_id: Uuid::from_u128(0x4d41_494c),
        extension_title: "Mail".to_owned(),
        model_name: MAIL_COUNT.to_owned(),
        ..defs[0].clone()
    };
    defs.push(mail);
    let frames = FrameRegistry::default();
    let session = frames.open(installed, Uuid::new_v4(), "tab-1");
    let links: Arc<dyn Links> = Arc::new(InMemoryFrame {
        session,
        _frames: frames,
    });
    let tools = register_extension_tools(&mut ToolRegistry::new(), defs, &links);
    fx.extension_tools.replace(tools.clone());
    Extensions { installed, tools }
}

fn granted(extension: Uuid) -> Rights {
    Rights {
        extensions: [extension].into_iter().collect(),
        ..default_rights()
    }
}

fn extension_names(names: &[String]) -> Vec<&str> {
    names
        .iter()
        .map(String::as_str)
        .filter(|name| name.starts_with("x_"))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_granted_extension_offers_its_tools_like_to_the_built_in_agent_and_others_stay_hidden() {
    let mut fx = fixture();
    let extensions = publish(&fx).await;
    let server = fx.server().await;
    let (agent, _helper) = fx
        .registered("claude-code", granted(extensions.installed))
        .await;

    let listed = agent.list_all_tools().await.expect("tools/list");
    let names: Vec<String> = listed.iter().map(|tool| tool.name.to_string()).collect();
    let mut offered = extension_names(&names);
    offered.sort_unstable();
    assert_eq!(offered, [ADD, COUNT, "x_agenttools_slow_echo"]);
    for tool in listed.iter().filter(|tool| tool.name.starts_with("x_")) {
        let built_in = extensions
            .tools
            .iter()
            .find(|known| known.name() == tool.name)
            .expect("published");
        assert_eq!(tool.description.as_deref(), Some(built_in.description()));
        assert_eq!(
            Value::Object((*tool.input_schema).clone()),
            built_in.input_schema()
        );
    }

    // A confirmed reading tool runs in Auto and answers with the extension's data.
    let result = call(agent.peer(), COUNT).await;
    assert_eq!(error_code(&result), None);
    assert_eq!(
        result.structured_content,
        Some(json!({ "extension": "Test-Erweiterung", "data": r#"{"entries":2}"# }))
    );

    // "ask" in the extension is stricter than Auto (FR-018); the prompt names the extension.
    let asking = tokio::spawn({
        let peer = agent.peer().clone();
        async move { call_with(&peer, ADD, json!({ "text": "Milch" })).await }
    });
    let (id, question) = fx.next_call().await;
    assert_eq!(question.action_id, ADD);
    assert_eq!(question.titles.de, "Test-Erweiterung: Eintrag hinzufügen");
    fx.answer_call(&id, CallDecision::Deny);
    let declined = asking.await.expect("the call");
    assert_eq!(error_code(&declined).as_deref(), Some("user_declined"));

    // An extension that is not granted is neither listed nor callable.
    assert_eq!(
        error_code(&call(agent.peer(), MAIL_COUNT).await).as_deref(),
        Some("not_allowed")
    );
    server.stop(CloseReason::Disabled).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_extension_appears_once_granted_and_a_removed_one_is_unavailable() {
    let mut fx = fixture();
    let extensions = publish(&fx).await;
    let server = fx.server().await;
    let (agent, _helper) = fx.registered("claude-code", default_rights()).await;
    assert!(extension_names(&tool_names(&agent).await).is_empty());
    let id = fx.agent_id("claude-code").await;

    // Granting it sends `tools/list_changed`, and the tools are there.
    let mut changes = agent.service().list_changes();
    set_rights(
        &fx.db,
        fx.device,
        Some(server.core()),
        id,
        granted(extensions.installed),
    )
    .await
    .expect("rights");
    next_list_change(&mut changes).await;
    assert!(extension_names(&tool_names(&agent).await).contains(&COUNT));

    // Removing the extension: the list changes and a call of a tool the agent saw is unavailable,
    // while one it never saw stays not allowed.
    let remaining: Vec<_> = extensions
        .tools
        .iter()
        .filter(|tool| tool.name() == MAIL_COUNT)
        .cloned()
        .collect();
    fx.extension_tools.replace(remaining);
    next_list_change(&mut changes).await;
    assert!(extension_names(&tool_names(&agent).await).is_empty());
    assert_eq!(
        error_code(&call(agent.peer(), COUNT).await).as_deref(),
        Some("unavailable")
    );
    assert_eq!(
        error_code(&call(agent.peer(), "x_agenttools_never_seen").await).as_deref(),
        Some("not_allowed")
    );
    server.stop(CloseReason::Disabled).await;
}
