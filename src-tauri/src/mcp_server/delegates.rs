//! Delegates in the chat (spec 021 US6, FR-024, FR-025, research R7, data-model.md): Claude Code
//! and Codex, which holzi starts itself (specs 007/009), register as external agents once per kind
//! and device. Their row's id comes from device and kind, and they keep no credential: each start
//! gets a pass that lives only in this process, names the kind and the chat thread, and ends with
//! the start. The helper hands it over as `--delegate-token`; a call prompt of that connection
//! names the thread, so the chat shows it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use async_trait::async_trait;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::connection::now_millis;
use super::grants::{self, default_rights};
use super::hello::RejectReason;
use super::manage;
use super::prompts::{
    PromptClosed, PromptClosedReason, RegistrationDecision, RegistrationQuestion,
};
use super::registry::{self, AgentKind, AgentMode, AgentStatus, Credential, Transport};
use super::server::{Core, Notice};
use super::session::SessionAgent;
use crate::adapters::cli_delegate::holzi_mcp::{HolziAccess, HolziServer};
use crate::adapters::cli_delegate::DelegateVendor;
use crate::prompt_park::PromptEnd;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelegatePass {
    pub kind: AgentKind,
    /// The chat thread the delegate answers in; its call prompts show there.
    pub thread: Option<Uuid>,
}

type Passes = Arc<Mutex<HashMap<String, DelegatePass>>>;

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
            .copied()
    }
}

#[async_trait]
impl HolziAccess for DelegateHub {
    async fn server_for(
        &self,
        vendor: DelegateVendor,
        thread: Option<Uuid>,
    ) -> Option<HolziServer> {
        let core = self.core()?;
        let kind = kind_of(vendor);
        if status(&core, kind).await != DelegateStatus::Registered {
            return None;
        }
        let executable = super::helper::executable()
            .inspect_err(|error| log::warn!("mcp server: no path for a delegate: {error}"))
            .ok()?;
        let (credential, guard) = self
            .issue(DelegatePass { kind, thread })
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
    Unregistered,
    /// Not started for 90 days (FR-007): registering again starts from its last rights.
    Expired,
    Registered,
}

/// The status of the delegate `kind`; a registration that ran out is marked expired here.
pub async fn status(core: &Core, kind: AgentKind) -> DelegateStatus {
    let (device, id, now) = (core.device, delegate_id(core.device, kind), now_millis());
    let agent = match core.db.read(move |q| registry::get(q, device, id)).await {
        Ok(agent) => agent,
        Err(error) => {
            log::warn!("mcp server: could not read a delegate: {error}");
            return DelegateStatus::Unregistered;
        }
    };
    match agent {
        Some(agent)
            if agent.status == AgentStatus::Registered && !manage::is_expired(&agent, now) =>
        {
            DelegateStatus::Registered
        }
        Some(agent) if manage::is_expired(&agent, now) => {
            if agent.status == AgentStatus::Registered {
                match core
                    .db
                    .write(move |tx| manage::expire(tx, device, id))
                    .await
                {
                    Ok(()) => (core.notify)(Notice::Changed),
                    Err(error) => log::warn!("mcp server: could not expire a delegate: {error}"),
                }
            }
            DelegateStatus::Expired
        }
        _ => DelegateStatus::Unregistered,
    }
}

/// Admits a delegate's connection by its pass: the delegate must still be registered.
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
            if agent.status != AgentStatus::Registered || manage::is_expired(&agent, now) {
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

/// How "Rechte festlegen" ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum DelegateRegistration {
    Registered,
    /// Declined, closed or not answered in time; nothing is remembered.
    Declined,
    /// A registration dialog of this delegate is open already.
    Busy,
    /// The MCP access is off.
    Off,
}

/// "Rechte festlegen" for the delegate `kind` (FR-025): the registration dialog, preset with an
/// expired registration's rights; the answer registers the delegate. Declining blocks nothing: the
/// user asked for the dialog.
pub async fn register(core: &Arc<Core>, kind: AgentKind) -> DelegateRegistration {
    let Some(slug) = slug_of(kind) else {
        return DelegateRegistration::Declined;
    };
    let Some(_ticket) = core.gate.try_open(slug) else {
        return DelegateRegistration::Busy;
    };
    let (device, id) = (core.device, delegate_id(core.device, kind));
    let previous = core
        .db
        .read(move |q| {
            let Some(agent) = registry::get(q, device, id)? else {
                return Ok(None);
            };
            Ok(Some((agent.mode, registry::load_rights(q, device, id)?)))
        })
        .await
        .unwrap_or_else(|error| {
            log::warn!("mcp server: could not read a delegate: {error}");
            None
        });
    let (mode, rights) = previous.unwrap_or_else(|| (AgentMode::default(), default_rights()));
    let question = RegistrationQuestion {
        slug: slug.to_owned(),
        client_name: name_of(kind).to_owned(),
        client_version: None,
        transport: Transport::Ipc,
        rights,
        mode,
        delegate: true,
    };
    let mut request_id = String::new();
    let answer = core
        .prompts
        .ask(&question, &core.token, |id| request_id = id.to_owned())
        .await;
    let reason = match &answer {
        Ok(_) => PromptClosedReason::Answered,
        Err(PromptEnd::Expired) => PromptClosedReason::Timeout,
        Err(_) => PromptClosedReason::Cancelled,
    };
    (core.notify)(Notice::PromptClosed(PromptClosed { request_id, reason }));
    let Ok(answer) = answer else {
        return DelegateRegistration::Declined;
    };
    if answer.decision == RegistrationDecision::Deny || grants::validate(&answer.rights).is_err() {
        return DelegateRegistration::Declined;
    }
    let guard = core.live.synchronize(id).await;
    let now = now_millis();
    let stored = core
        .db
        .write(move |tx| {
            registry::register_delegate(
                tx,
                device,
                kind,
                name_of(kind),
                &answer.rights,
                answer.mode,
                now,
            )
        })
        .await;
    drop(guard);
    match stored {
        Ok(_) => {
            (core.notify)(Notice::Changed);
            DelegateRegistration::Registered
        }
        Err(error) => {
            log::warn!("mcp server: could not register a delegate: {error}");
            DelegateRegistration::Declined
        }
    }
}
