//! What the agent profile of a turn's model lets the local agent use (spec 083 US5, FR-017, research
//! R6). `chat/action_commands.rs` registers every built-in tool into the shared registry; the turn
//! decides per step what is offered and per call what runs, with the same decisions
//! `mcp_server::grants` makes for an external agent: the action tools by scope, `run_command` by the
//! right "Befehle ausführen", an extension's tools by its right. `find_actions` and `ask_user` are
//! always there. The approval mode is a separate matter and stays as it is (FR-017).

use serde::Serialize;

use super::action_tool::ACTION_SOURCE;
use super::extension_offer::EXTENSION_SOURCE;
use super::{Tool, ToolRegistry};
use crate::adapters::types::ToolSpec;
use crate::mcp_server::grants::{
    covers_extension_tool, covers_local, OperateLevel, Rights, BROWSER_SCOPE, PASSWORD_SCOPES,
};

/// The registry `source` and name of the host command tool (`chat::tools::cli`).
const COMMAND_SOURCE: &str = "cli";
const COMMAND_TOOL: &str = "run_command";

/// What a profile does not give, for the notice in the chat. Names no level: it never claims the
/// model may only read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Missing {
    /// The browser group is off.
    Browser,
    /// The password manager is off.
    Passwords,
    /// A topic of "holzi bedienen" with no access (`topic`) or only reading (`topicWrite`).
    Topic {
        topic: String,
    },
    TopicWrite {
        topic: String,
    },
    /// "Befehle ausführen".
    Commands,
    /// An extension's tools; its name.
    Extension {
        name: String,
    },
}

/// `Ok` when `rights` let the local agent use `tool`, else what is missing.
pub fn permits(rights: &Rights, tool: &dyn Tool) -> Result<(), Missing> {
    match tool.source() {
        ACTION_SOURCE => {
            let Some(def) = tool.action_definition() else {
                // `find_actions`, `ask_user`: no right is needed to look or to ask.
                return Ok(());
            };
            if matches!(
                tool.name(),
                "browser.credentials.fill" | "browser.entry.fill"
            ) && rights.passwords.is_none()
            {
                return Err(Missing::Passwords);
            }
            if covers_local(rights, def) {
                return Ok(());
            }
            Err(missing_of(rights, &def.scope))
        }
        EXTENSION_SOURCE => {
            if covers_extension_tool(rights, tool) {
                Ok(())
            } else {
                Err(Missing::Extension {
                    name: tool
                        .origin()
                        .map(|origin| origin.extension_name)
                        .unwrap_or_default(),
                })
            }
        }
        COMMAND_SOURCE if tool.name() == COMMAND_TOOL && !rights.commands => Err(Missing::Commands),
        // Tools of MCP servers the user connected to the chat keep their own switch.
        _ => Ok(()),
    }
}

fn missing_of(rights: &Rights, scope: &str) -> Missing {
    if scope == BROWSER_SCOPE {
        return Missing::Browser;
    }
    if PASSWORD_SCOPES.contains(&scope) {
        return Missing::Passwords;
    }
    match rights.operate.get(scope) {
        Some(OperateLevel::Read) => Missing::TopicWrite {
            topic: scope.to_owned(),
        },
        _ => Missing::Topic {
            topic: scope.to_owned(),
        },
    }
}

/// What the system prompt says about the extensions whose tools `rights` let the local agent use:
/// one summary per permitted tool, so an extension the profile does not give is never named.
pub fn permitted_summaries(
    rights: &Rights,
    registry: &ToolRegistry,
) -> Vec<super::extension_offer::ExtensionSummary> {
    registry
        .iter()
        .filter(|tool| tool.source() == EXTENSION_SOURCE && permits(rights, tool.as_ref()).is_ok())
        .filter_map(|tool| tool.extension_summary())
        .collect()
}

/// Drops from `tools` what `rights` do not let the local agent use; the first thing dropped is
/// returned. A tool the registry does not know stays: it is no registered tool's name.
pub fn retain_permitted(
    rights: &Rights,
    registry: &ToolRegistry,
    tools: &mut Vec<ToolSpec>,
) -> Option<Missing> {
    let mut first = None;
    tools.retain(|spec| {
        let Some(tool) = registry.get(&spec.name) else {
            return true;
        };
        match permits(rights, tool.as_ref()) {
            Ok(()) => true,
            Err(missing) => {
                first.get_or_insert(missing);
                false
            }
        }
    });
    first
}

/// What the agent profile of the model of `session` gives, read fresh. A vault that cannot be read
/// gives nothing: less, never more.
pub async fn rights_of_session(
    db: &crate::vault_gate::VaultDb,
    session: &crate::chat::session::ActiveSession,
) -> Rights {
    let subject = crate::agent_profiles::local::subject_of_session(session);
    match db
        .read(move |q| crate::agent_profiles::local::rights_for(q, subject.as_ref()))
        .await
    {
        Ok(rights) => rights,
        Err(error) => {
            log::warn!("chat: could not read the agent profile of the model: {error}");
            Rights::default()
        }
    }
}

/// The answer to a call the profile does not permit: a fixed marker, then what to do next.
pub fn refusal(missing: &Missing) -> String {
    let what = match missing {
        Missing::Browser => "the browser".to_owned(),
        Missing::Passwords => "the password manager".to_owned(),
        Missing::Topic { topic } | Missing::TopicWrite { topic } => format!("the area {topic}"),
        Missing::Commands => "running commands".to_owned(),
        Missing::Extension { name } => format!("the extension {name}"),
    };
    format!(
        "not_allowed_by_profile: the agent profile of this model does not give {what}. Do not \
         try it again; tell the user that the agent profile has to allow it."
    )
}

#[cfg(test)]
#[path = "profile_gate_tests.rs"]
mod tests;
