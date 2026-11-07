//! Delegates in the chat (spec 021 US6, FR-024, FR-025, research R7, data-model.md; spec 083 PR J):
//! Claude Code and Codex, which holzi starts itself (specs 007/009). Their row in `haex_mcp_agents`
//! (one per kind and device, its id from both) is a technical identity only: it is written by the
//! start itself, asks nothing and carries no rights. What a delegate may do is the profile of the
//! model the start runs with, like for any model (FR-027, FR-028). Each start gets a pass that
//! lives only in this process, names the kind, the model and the chat thread, and ends with the
//! start. The helper hands it over as `--delegate-token`; a call prompt of that connection names
//! the thread, so the chat shows it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use async_trait::async_trait;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::connection::now_millis;
use super::hello::RejectReason;
use super::registry::{self, AgentKind, AgentMode, AgentStatus, Credential};
use super::server::Core;
use super::session::SessionAgent;
use crate::adapters::cli_delegate::holzi_mcp::{HolziAccess, HolziServer};
use crate::adapters::cli_delegate::DelegateVendor;
use crate::agent_profiles::STANDARD_PROFILE_ID;

/// Namespace of the row ids of delegates in `haex_mcp_agents`.
const NS_MCP_DELEGATE: Uuid = Uuid::from_u128(0x6d63_7021_de1e_4a7e_8c3b_5f0e_2d9a_c021);

/// The delegate of `vendor`.
pub fn kind_of(vendor: DelegateVendor) -> AgentKind {
    match vendor {
        DelegateVendor::Claude => AgentKind::ClaudeCodeDelegate,
        DelegateVendor::Codex => AgentKind::CodexDelegate,
    }
}

/// The slug a delegate's helper introduces itself with; `None` for an agent that is no delegate.
pub fn slug_of(kind: AgentKind) -> Option<&'static str> {
    match kind {
        AgentKind::ClaudeCodeDelegate => Some("claude-code-delegate"),
        AgentKind::CodexDelegate => Some("codex-delegate"),
        AgentKind::External => None,
    }
}

/// The name the registration dialog shows; holzi starts the delegate, so it is no claim.
fn name_of(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::CodexDelegate => "Codex",
        _ => "Claude Code",
    }
}

/// The row id of the delegate `kind` on `device`: two devices never collide (data-model.md).
pub fn delegate_id(device: Uuid, kind: AgentKind) -> Uuid {
    Uuid::new_v5(
        &NS_MCP_DELEGATE,
        format!("{device}:{}", kind.as_str()).as_bytes(),
    )
}

/// What a pass stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatePass {
    pub kind: AgentKind,
    /// The chat thread the delegate answers in; its call prompts show there.
    pub thread: Option<Uuid>,
    /// The model the start runs with (`<provider id>:<remote id>`): its profile decides what the
    /// delegate may do (FR-028).
    pub model: String,
}

type Passes = Arc<Mutex<HashMap<String, DelegatePass>>>;

/// The passes of this process in the order they were issued, by kind: what the rights of a delegate
/// are looked up by (`registry::load_rights`), which has no session to ask. Until rights are resolved
/// from the session's own pass, a kind must have at most one live pass; otherwise concurrent models
/// of the same provider could borrow each other's profile rights.
static LIVE: Mutex<Vec<(String, AgentKind, String)>> = Mutex::new(Vec::new());

/// The model the delegate `kind` runs with now, while a start of it lives.
pub fn live_model(kind: AgentKind) -> Option<String> {
    LIVE.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .rev()
        .find(|(_, live, _)| *live == kind)
        .map(|(_, _, model)| model.clone())
}

/// Ends a pass when the delegate's start ends.
pub struct PassGuard {
    passes: Passes,
    key: String,
}

impl Drop for PassGuard {
    fn drop(&mut self) {
        self.passes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.key);
        LIVE.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(key, _, _)| *key != self.key);
    }
}

/// The passes of this process and the running server they lead to; outlives a server restart.
#[derive(Default)]
pub struct DelegateHub {
    /// Keyed by the pass's hash, like a stored credential.
    passes: Passes,
    core: Mutex<Weak<Core>>,
}

impl DelegateHub {
    /// The server that answers delegates now.
    pub(super) fn attach(&self, core: &Arc<Core>) {
        *self.core.lock().unwrap_or_else(PoisonError::into_inner) = Arc::downgrade(core);
    }

    fn core(&self) -> Option<Arc<Core>> {
        self.core
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .upgrade()
            .filter(|core| !core.token.is_cancelled())
    }

    /// A pass for one start; it ends when the guard is dropped.
    pub fn issue(&self, pass: DelegatePass) -> std::io::Result<(Credential, PassGuard)> {
        let credential = Credential::generate()?;
        let key = registry::hash(credential.as_str());
        let mut live = LIVE.lock().unwrap_or_else(PoisonError::into_inner);
        if live.iter().any(|(_, kind, _)| *kind == pass.kind) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "a delegate of this provider already has a live session",
            ));
        }
        live.push((key.clone(), pass.kind, pass.model.clone()));
        drop(live);
        self.passes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone(), pass);
        let guard = PassGuard {
            passes: Arc::clone(&self.passes),
            key,
        };
        Ok((credential, guard))
    }

    /// What `credential` stands for, while its start runs.
    pub fn find(&self, credential: &str) -> Option<DelegatePass> {
        self.passes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&registry::hash(credential))
            .cloned()
    }
}

/// The approval mode of the chat (`chat.permission_mode`, the control of the composer) as a
/// delegate's calls ask: a delegate has no mode of its own any more.
fn chat_mode(q: &mut impl crate::storage::query::Query) -> haex_crdt::Result<AgentMode> {
    use crate::storage::preferences::{get, PrefScope};
    Ok(
        match get(q, PrefScope::Vault, "chat.permission_mode")?.as_deref() {
            Some("auto") => AgentMode::Auto,
            Some("plan") => AgentMode::Plan,
            _ => AgentMode::Manual,
        },
    )
}

#[async_trait]
impl HolziAccess for DelegateHub {
    async fn server_for(
        &self,
        vendor: DelegateVendor,
        thread: Option<Uuid>,
        model: &str,
    ) -> Option<HolziServer> {
        let core = self.core()?;
        let kind = kind_of(vendor);
        let (device, now, name) = (core.device, now_millis(), name_of(kind));
        // The start itself writes the technical row, and only for a model the active profile
        // offers (FR-006b); the approval mode is the chat's.
        let model_owned = model.to_owned();
        let written = core
            .db
            .write(move |tx| {
                if !crate::agent_profiles::resolve::model_available(tx, &model_owned)? {
                    return Ok(false);
                }
                let mode = chat_mode(tx)?;
                registry::register_delegate(
                    tx,
                    device,
                    kind,
                    name,
                    STANDARD_PROFILE_ID,
                    mode,
                    now,
                )?;
                Ok(true)
            })
            .await;
        match written {
            Ok(true) => {}
            Ok(false) => return None,
            Err(error) => {
                log::warn!("mcp server: could not write a delegate: {error}");
                return None;
            }
        }
        let executable = super::helper::executable()
            .inspect_err(|error| log::warn!("mcp server: no path for a delegate: {error}"))
            .ok()?;
        let (credential, guard) = self
            .issue(DelegatePass {
                kind,
                thread,
                model: model.to_owned(),
            })
            .inspect_err(|error| log::warn!("mcp server: no pass for a delegate: {error}"))
            .ok()?;
        let args = vec![
            super::helper::FLAG.to_owned(),
            slug_of(kind)?.to_owned(),
            "--vault".to_owned(),
            core.device.to_string(),
            "--delegate-token".to_owned(),
            credential.into_string(),
        ];
        Some(HolziServer::new(executable, args, guard))
    }
}

/// Whether a delegate has holzi's tools, as the chat shows it (FR-025).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DelegateStatus {
    /// The MCP access is off; no delegate gets holzi's tools.
    Off,
    /// The delegate gets holzi's tools as far as the profile of its model allows (FR-029).
    Available,
}

/// The status of the delegates: off while the MCP access is, else available.
pub fn status() -> DelegateStatus {
    DelegateStatus::Available
}

/// Admits a delegate's connection by its pass: the technical row must be there and not blocked.
pub(super) async fn admit(
    core: &Core,
    credential: Option<&str>,
) -> Result<SessionAgent, RejectReason> {
    let pass = credential
        .and_then(|credential| core.delegates.find(credential))
        .ok_or(RejectReason::NotRegistered)?;
    let (device, id, now) = (
        core.device,
        delegate_id(core.device, pass.kind),
        now_millis(),
    );
    let found = core
        .db
        .write(move |tx| {
            let Some(agent) = registry::get(tx, device, id)? else {
                return Ok(None);
            };
            if agent.status != AgentStatus::Registered {
                return Ok(None);
            }
            registry::touch(tx, device, id, now)?;
            Ok(Some((agent, registry::load_rights(tx, device, id)?)))
        })
        .await;
    match found {
        Ok(Some((agent, rights))) => Ok(SessionAgent {
            id,
            rights,
            active_profile: crate::agent_profiles::store::Active::Unset,
            mode: agent.mode,
            client_name: agent.client_name,
            delegate_thread: pass.thread,
        }),
        Ok(None) => Err(RejectReason::NotRegistered),
        Err(error) => {
            log::warn!("mcp server: could not admit a delegate: {error}");
            Err(RejectReason::Disabled)
        }
    }
}
