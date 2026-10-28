//! The registration of an agent holzi does not know (yet) (spec 021 US1, US7, FR-006, FR-007,
//! FR-009): once the agent introduced itself, the window is asked; the answer registers it, renews
//! an expired registration with the rights the user chose again, or declines it, for this vault
//! session or for good. Runs next to the session, so the agent can finish `initialize` meanwhile.

use std::sync::Arc;

use rmcp::service::{Peer, RoleServer};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::connection::now_millis;
use super::grants::{self, default_rights, Rights};
use super::hello::{CloseReason, Control};
use super::live::Link;
use super::manage::{self, Renewal};
use super::prompts::{
    GateTicket, PromptClosed, PromptClosedReason, RegistrationDecision, RegistrationQuestion,
};
use super::registry::{self, AgentKind, AgentMode, Credential, NewAgent, Transport};
use super::server::{Core, Notice};
use super::session::{Session, SessionAgent};
use crate::prompt_park::PromptEnd;

/// An expired registration the dialog starts from (data-model.md: `expired → registered`).
pub(super) struct Previous {
    pub id: Uuid,
    pub rights: Rights,
    pub mode: AgentMode,
}

/// Inputs shared by the registration task.
pub(super) struct RegistrationContext {
    pub(super) core: Arc<Core>,
    pub(super) session: Session,
    pub(super) peer: Peer<RoleServer>,
    pub(super) controls: mpsc::UnboundedSender<Control>,
    pub(super) ticket: GateTicket,
    pub(super) previous: Option<Previous>,
    pub(super) slug: String,
    pub(super) ended: CancellationToken,
    pub(super) link: Arc<Link>,
}

/// Asks the window once the agent introduced itself, and acts on the answer.
pub(super) async fn register(context: RegistrationContext) {
    let RegistrationContext {
        core,
        session,
        peer,
        controls,
        ticket,
        previous,
        slug,
        ended,
        link,
    } = context;
    let _ticket = ticket;
    let mut initialized = session.initialized();
    // The guard `wait_for` returns must not live across the awaits below.
    let introduced = matches!(
        tokio::time::timeout(
            core.limits.initialize_deadline,
            initialized.wait_for(|done| *done),
        )
        .await,
        Ok(Ok(_))
    );
    if !introduced {
        return;
    }
    let (client_name, client_version) = match peer.peer_info() {
        Some(info) => (
            registry::kept_name(&info.client_info.name),
            Some(registry::kept_name(&info.client_info.version)),
        ),
        None => (slug.clone(), None),
    };
    let question = RegistrationQuestion {
        slug: slug.clone(),
        client_name: client_name.clone(),
        client_version: client_version.clone(),
        transport: Transport::Ipc,
        rights: previous
            .as_ref()
            .map_or_else(default_rights, |previous| previous.rights.clone()),
        mode: previous
            .as_ref()
            .map_or_else(AgentMode::default, |previous| previous.mode),
    };
    let mut request_id = String::new();
    let answer = core
        .prompts
        .ask(&question, &ended, |id| request_id = id.to_owned())
        .await;
    let reason = match &answer {
        Ok(_) => PromptClosedReason::Answered,
        Err(PromptEnd::Expired) => PromptClosedReason::Timeout,
        Err(_) => PromptClosedReason::Cancelled,
    };
    (core.notify)(Notice::PromptClosed(PromptClosed { request_id, reason }));
    let answer = match answer {
        Ok(answer) => answer,
        // No window, no answer in time: as if declined once (US1 scenario 4).
        Err(PromptEnd::Expired | PromptEnd::NoWindow) => {
            core.decline(&slug);
            link.end(CloseReason::Denied);
            return;
        }
        Err(PromptEnd::Cancelled) => return,
    };
    let device = core.device;
    if answer.decision == RegistrationDecision::Deny || grants::validate(&answer.rights).is_err() {
        let remember = answer.decision == RegistrationDecision::Deny && answer.remember;
        if remember {
            let now = now_millis();
            let blocked_slug = slug.clone();
            let blocked_name = client_name.clone();
            if let Err(error) = core
                .db
                .write(move |tx| registry::block(tx, device, &blocked_slug, &blocked_name, now))
                .await
            {
                log::warn!("mcp server: could not remember a block: {error}");
            }
        }
        core.decline(&slug);
        (core.notify)(Notice::Changed);
        link.end(CloseReason::Denied);
        return;
    }
    let credential = match Credential::generate() {
        Ok(credential) => credential,
        Err(error) => {
            log::warn!("mcp server: no random source for a credential: {error}");
            link.end(CloseReason::Denied);
            return;
        }
    };
    let digest = registry::hash(credential.as_str());
    let rights = answer.rights.clone();
    let now = now_millis();
    let stored = match previous {
        Some(previous) => {
            let renewal = Renewal {
                client_name,
                client_version,
                mode: answer.mode,
            };
            core.db
                .write(move |tx| {
                    manage::renew(tx, device, previous.id, &renewal, &digest, &rights, now)
                })
                .await
        }
        None => {
            let new = NewAgent {
                kind: AgentKind::External,
                slug,
                client_name,
                client_version,
                transport: Transport::Ipc,
                mode: answer.mode,
            };
            core.db
                .write(move |tx| registry::register(tx, device, &new, &digest, &rights, now))
                .await
                .map(Some)
        }
    };
    let agent = match stored {
        Ok(Some(agent)) => agent,
        // Revoked while the dialog was open.
        Ok(None) => {
            link.end(CloseReason::Revoked);
            return;
        }
        Err(error) => {
            log::warn!("mcp server: could not store a registration: {error}");
            link.end(CloseReason::Denied);
            return;
        }
    };
    session.set_agent(SessionAgent {
        id: agent.id,
        rights: answer.rights,
        mode: agent.mode,
        client_name: agent.client_name,
    });
    // The credential before the new tool list, so the helper has saved it when the agent asks.
    let _ = controls.send(Control::Token(credential.into_string()));
    let _ = peer.notify_tool_list_changed().await;
    (core.notify)(Notice::Changed);
}
