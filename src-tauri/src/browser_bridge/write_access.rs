//! Writing with a read-only approval (spec 049, FR-015): holzi asks like a permission prompt of
//! spec 017, but only when the client declared writing; a remembered "no" holds without a question,
//! and remembering never widens the approval beyond the declaration. The answer is held back for at
//! most 120 s; then the method fails with `PERMISSION_PROMPT_TIMEOUT`.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::approvals::{self, Approval, ClientIdentity, Status};
use super::declaration::{BridgeAccess, Declared};
use super::prompts::{BridgeAnswer, BridgeClientRef, BridgeQuestion, WriteAnswer, WriteQuestion};
use super::protocol::ErrorCode;
use super::runtime::{BridgeCore, Notice};
use crate::prompt_park::PromptEnd;

/// The approval a writing method runs with, after asking where FR-015 says so.
pub(super) async fn ensure_write(
    core: &Arc<BridgeCore>,
    client: &ClientIdentity,
    approval: Approval,
    remembered: bool,
    method: &str,
    cancel: &CancellationToken,
) -> Result<Approval, ErrorCode> {
    if approval.access == BridgeAccess::ReadWrite {
        return Ok(approval);
    }
    let may_ask = !approval.write_denied
        && Declared::from_canonical(&client.declaration)
            .is_some_and(|d| d.fits(BridgeAccess::ReadWrite, approval.scope_tags.as_deref()));
    if !may_ask {
        return Err(ErrorCode::PermissionDenied);
    }
    let question = BridgeQuestion::Write(WriteQuestion {
        client: BridgeClientRef {
            client_id: client.client_id.clone(),
            client_name: client.client_name.clone(),
            origin: client.origin.clone(),
        },
        method: method.to_owned(),
        scope_tags: approval.scope_tags.clone(),
        needs_default_tag: approval.scope_tags.is_some() && approval.default_tag.is_none(),
    });
    let mut prompt_id = String::new();
    let answer = core
        .prompts
        .ask(&question, cancel, |id| prompt_id = id.to_owned())
        .await;
    match answer {
        Ok(BridgeAnswer::Write(answer)) => apply(core, client, approval, remembered, answer).await,
        Ok(_) => Err(ErrorCode::PermissionDenied),
        Err(end) => {
            if end != PromptEnd::NoWindow {
                core.notify(Notice::PromptCancelled(prompt_id));
            }
            Err(match end {
                PromptEnd::Expired => ErrorCode::PermissionPromptTimeout,
                _ => ErrorCode::PermissionDenied,
            })
        }
    }
}

async fn apply(
    core: &Arc<BridgeCore>,
    client: &ClientIdentity,
    approval: Approval,
    remembered: bool,
    answer: WriteAnswer,
) -> Result<Approval, ErrorCode> {
    if !answer.allow {
        if answer.remember {
            if remembered {
                let denied = Approval {
                    write_denied: true,
                    ..approval
                };
                store(core, client, denied).await?;
            } else {
                core.session().deny_write(&client.client_id);
            }
        }
        return Err(ErrorCode::PermissionDenied);
    }
    let default_tag = answer
        .default_tag
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .or(approval.default_tag.clone());
    // With a tag scope, new entries need a default tag inside it (034 Z6, FR-010).
    if let Some(tags) = &approval.scope_tags {
        if !default_tag.as_ref().is_some_and(|tag| tags.contains(tag)) {
            return Err(ErrorCode::PermissionDenied);
        }
    }
    let widened = Approval {
        access: BridgeAccess::ReadWrite,
        default_tag,
        write_denied: false,
        ..approval
    };
    if answer.remember && remembered {
        store(core, client, widened.clone()).await?;
    } else {
        core.session()
            .allow_write(&client.client_id, widened.default_tag.clone());
    }
    core.notify(Notice::Changed);
    Ok(widened)
}

/// Replaces the remembered approval of a client, keeping when it was decided.
async fn store(
    core: &Arc<BridgeCore>,
    client: &ClientIdentity,
    approval: Approval,
) -> Result<(), ErrorCode> {
    let device = core.device;
    let client_id = client.client_id.clone();
    let written = core
        .db
        .write(move |tx| {
            let Some(mut row) = approvals::load(tx, device, &client_id)? else {
                return Ok(false);
            };
            row.status = Status::Allowed(approval);
            approvals::save(tx, device, &row)?;
            Ok(true)
        })
        .await;
    match written {
        Ok(true) => Ok(()),
        Ok(false) => Err(ErrorCode::PermissionDenied),
        Err(error) => {
            log::warn!("browser bridge: could not store the write decision: {error}");
            Err(ErrorCode::ServerError)
        }
    }
}
