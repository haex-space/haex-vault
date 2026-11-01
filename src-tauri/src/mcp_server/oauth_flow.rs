//! The OAuth steps of the HTTP way that reach the window and the database (spec 021 US1, T094,
//! contracts/http-auth.md): an authorization asks the registration dialog, as on the program way;
//! a grant registers the agent with a fresh refresh token as its credential, handed out once by the
//! code exchange; a refresh rotates it; an expired, revoked or blocked agent gets no token.

use std::sync::Arc;
use std::time::Instant;

use uuid::Uuid;

use super::connection::now_millis;
use super::grants::{self, default_rights};
use super::manage;
use super::oauth::{Client, Decision, OAuthError, OAuthState};
use super::oauth_rows::{self, Authorized};
use super::prompts::{
    PromptClosed, PromptClosedReason, RegistrationDecision, RegistrationQuestion,
};
use super::registry::{self, AgentMode, AgentStatus, Credential, Transport};
use super::server::{Core, Notice};
use crate::prompt_park::PromptEnd;

/// Asks the window about the authorization `pending` of `client` and settles it.
pub(super) async fn decide(
    core: Arc<Core>,
    oauth: Arc<OAuthState>,
    pending: String,
    client: Client,
) {
    let (decision, refresh) = match ask(&core, &client).await {
        Some((agent, refresh)) => (Decision::Granted { agent }, Some(refresh)),
        None => (Decision::Denied, None),
    };
    oauth.settle(&pending, decision, refresh);
}

/// The registered agent and its credential, or `None` when the user or a block declined.
async fn ask(core: &Arc<Core>, client: &Client) -> Option<(Uuid, Credential)> {
    let slug = oauth_rows::slug_of(client.name.as_deref());
    let client_name = client.name.clone().unwrap_or_else(|| slug.clone());
    let device = core.device;
    if core.declined(&slug) {
        return None;
    }
    let lookup = slug.clone();
    let (blocked, previous) = match core
        .db
        .read(move |q| {
            Ok((
                registry::is_blocked(q, device, &lookup)?,
                oauth_rows::previous(q, device, &lookup)?,
            ))
        })
        .await
    {
        Ok(found) => found,
        Err(error) => {
            log::warn!("mcp server: could not look up an HTTP agent: {error}");
            return None;
        }
    };
    if blocked {
        return None;
    }
    // One dialog per slug; another authorization of the same client waits for none.
    let _ticket = core.gate.try_open(&slug)?;
    let question = RegistrationQuestion {
        slug: slug.clone(),
        client_name: client_name.clone(),
        client_version: None,
        transport: Transport::Http,
        rights: previous
            .as_ref()
            .map_or_else(default_rights, |(_, rights)| rights.clone()),
        mode: previous
            .as_ref()
            .map_or_else(AgentMode::default, |(agent, _)| agent.mode),
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
    let answer = match answer {
        Ok(answer) => answer,
        Err(PromptEnd::Expired | PromptEnd::NoWindow) => {
            core.decline(&slug);
            return None;
        }
        Err(PromptEnd::Cancelled) => return None,
    };
    if answer.decision == RegistrationDecision::Deny || grants::validate(&answer.rights).is_err() {
        if answer.decision == RegistrationDecision::Deny && answer.remember {
            let (blocked_slug, name, now) = (slug.clone(), client_name.clone(), now_millis());
            if let Err(error) = core
                .db
                .write(move |tx| registry::block(tx, device, &blocked_slug, &name, now))
                .await
            {
                log::warn!("mcp server: could not remember a block: {error}");
            }
        }
        core.decline(&slug);
        (core.notify)(Notice::Changed);
        return None;
    }
    let refresh = match Credential::generate() {
        Ok(refresh) => refresh,
        Err(error) => {
            log::warn!("mcp server: no random source for a credential: {error}");
            return None;
        }
    };
    let digest = registry::hash(refresh.as_str());
    let authorized = Authorized {
        slug,
        client_name,
        client_id: client.id.clone(),
        mode: answer.mode,
    };
    let previous_id = previous.map(|(agent, _)| agent.id);
    let _agent_guard = match previous_id {
        Some(id) => Some(core.live.synchronize(id).await),
        None => None,
    };
    let rights = answer.rights.clone();
    let now = now_millis();
    let stored = core
        .db
        .write(move |tx| {
            oauth_rows::authorize(tx, device, previous_id, &authorized, &digest, &rights, now)
        })
        .await;
    (core.notify)(Notice::Changed);
    match stored {
        Ok(Some(agent)) => {
            // An earlier authorization's connections keep running with the rights just chosen.
            core.live.set_rights(agent.id, &answer.rights).await;
            core.live.set_mode(agent.id, agent.mode);
            Some((agent.id, refresh))
        }
        Ok(None) => None,
        Err(error) => {
            log::warn!("mcp server: could not store an authorization: {error}");
            None
        }
    }
}

/// A token response: a fresh access token and the refresh token to keep.
#[derive(Debug)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
}

/// `grant_type=refresh_token`: the agent behind the refresh token, still registered, not expired
/// and authorized for `client_id`, gets a new access token and a rotated refresh token.
pub(super) async fn refresh(
    core: &Arc<Core>,
    oauth: &OAuthState,
    refresh: &str,
    client_id: &str,
) -> Result<Tokens, OAuthError> {
    let device = core.device;
    let credential = refresh.to_owned();
    let found = core
        .db
        .read(move |q| {
            let Some(agent) = registry::find_by_credential(q, device, &credential)? else {
                return Ok(None);
            };
            let client = oauth_rows::client_of(q, device, agent.id)?;
            Ok(Some((agent, client)))
        })
        .await
        .map_err(|_| OAuthError::InvalidGrant)?;
    let Some((agent, client)) = found else {
        return Err(OAuthError::InvalidGrant);
    };
    if client.as_deref() != Some(client_id) {
        return Err(OAuthError::InvalidGrant);
    }
    let now = now_millis();
    if manage::is_expired(&agent, now) {
        let id = agent.id;
        if core
            .db
            .write(move |tx| manage::expire(tx, device, id))
            .await
            .is_ok()
        {
            (core.notify)(Notice::Changed);
        }
        return Err(OAuthError::InvalidGrant);
    }
    if agent.status != AgentStatus::Registered {
        return Err(OAuthError::InvalidGrant);
    }
    let next = Credential::generate().map_err(|_| OAuthError::TooMany)?;
    let (old_hash, new_hash, id) = (
        registry::hash(refresh),
        registry::hash(next.as_str()),
        agent.id,
    );
    let rotated = core
        .db
        .write(move |tx| oauth_rows::rotate(tx, device, id, &old_hash, &new_hash, now))
        .await
        .map_err(|_| OAuthError::InvalidGrant)?;
    if !rotated {
        return Err(OAuthError::InvalidGrant);
    }
    Ok(Tokens {
        access: oauth.issue_access(agent.id, Instant::now())?,
        refresh: next.into_string(),
    })
}

/// Whether the agent of an access token is still registered (a revocation removes the row).
pub(super) async fn still_registered(core: &Arc<Core>, agent: Uuid) -> bool {
    let device = core.device;
    matches!(
        core.db.read(move |q| registry::get(q, device, agent)).await,
        Ok(Some(found)) if found.status == AgentStatus::Registered
    )
}
