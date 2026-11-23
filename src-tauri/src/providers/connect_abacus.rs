//! Connecting Abacus AI from the app (spec 096 US1, `contracts/tauri-commands.md`): the device
//! sign-in of the vendor, run by HaexVault. The user types an e-mail address, confirms an address
//! and a code in the browser, and the device code the service then hands out becomes an entry of
//! the password manager. No terminal, no environment variable, no API key.
//!
//! The flow is a function of its parts (a sign-in client, the vault, the record, a callback for the
//! prompt) so a test drives it against a stand-in of the service. The e-mail address, the code and
//! the device code are never logged or recorded: the record lines carry fixed text only.

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::abacus_signin::{Client, Prompt, Secret, SigninError};
use super::connect::write_delegate_provider;
use super::delegate_credential::{self, Read, KIND_DEVICE_CODE};
use crate::adapters::cli_delegate::DelegateVendor;
use crate::cli_tools::record::Record;
use crate::error::{HaexError, Result};
use crate::storage::providers::Provider;
use crate::vault_gate::VaultDb;

/// The vendor id of the flow.
const VENDOR: &str = "abacusai";

/// The reasons the flow ends with that the frontend words (`errors.delegate.*`); never the text of
/// the service, which might echo what was typed.
pub(super) fn failure(error: &SigninError) -> HaexError {
    let reason = match error {
        SigninError::Refused(text) => format!("delegate.signin_refused: abacusai: {text}"),
        SigninError::Expired => "delegate.signin_expired: abacusai".to_owned(),
        SigninError::Cancelled => "delegate.signin_cancelled: abacusai".to_owned(),
        SigninError::Network => "delegate.signin_network: abacusai".to_owned(),
        SigninError::Unexpected => "delegate.signin_unexpected: abacusai".to_owned(),
    };
    HaexError::InvalidInput { reason }
}

/// What a scope held before the flow, to put it back when the flow cannot finish.
struct Prior {
    kind: String,
    secret: Secret,
}

async fn prior_credential(db: &VaultDb, scope: &str) -> Result<Option<Prior>> {
    Ok(match delegate_credential::read(db, VENDOR, scope).await? {
        Read::Ready(credential) => Some(Prior {
            kind: credential.kind().to_owned(),
            secret: Secret::new(credential.secret().to_owned()),
        }),
        Read::Missing => None,
    })
}

async fn restore(db: &VaultDb, scope: &str, prior: Option<Prior>) {
    let restored = match prior {
        Some(prior) => {
            delegate_credential::save(db, VENDOR, &prior.kind, prior.secret.expose(), scope).await
        }
        None => delegate_credential::delete_scope(db, VENDOR, scope).await,
    };
    if restored.is_err() {
        // Fixed text only: the error may carry what the vault printed.
        log::warn!("the prior Abacus credential could not be put back after a failed connect");
    }
}

/// Saves `device_code` and the provider row as one logical operation: the prior credential of the
/// scope and the prior row stay until both writes have succeeded, and a failure of either puts the
/// prior state back, so a failed connection leaves no orphan credential and never replaces a
/// working connection with a partial one.
pub(super) async fn save_connection(
    db: &VaultDb,
    name: String,
    device_code: &Secret,
) -> Result<Provider> {
    let scope = delegate_credential::scope_of(VENDOR);
    let prior = prior_credential(db, scope).await?;
    // Commit the provider row first. VaultGate may drop this future when the vault closes, while
    // the password-manager write runs on a blocking thread and can still commit. Having the row
    // in place first ensures that such a late credential commit is never orphaned.
    let provider = write_delegate_provider(db, VENDOR, name, Vec::new()).await?;
    if let Err(error) =
        delegate_credential::save(db, VENDOR, KIND_DEVICE_CODE, device_code.expose(), scope).await
    {
        restore(db, scope, prior).await;
        return Err(error);
    }
    Ok(provider)
}

/// Runs the sign-in: `start`, the prompt to `on_prompt`, polling until the person confirms in the
/// browser, then [`save_connection`]. `cancel` ends the polling at once without a further request.
pub(super) async fn connect(
    client: &Client,
    db: &VaultDb,
    record: &Record,
    name: String,
    email: &Secret,
    cancel: &CancellationToken,
    on_prompt: impl FnOnce(&Prompt),
) -> Result<Provider> {
    let vendor = DelegateVendor::Abacus.as_str();
    record.append("connect_started", &[("vendor", vendor)]);
    let outcome = sign_in(client, db, name, email, cancel, on_prompt).await;
    match &outcome {
        Ok(_) => record.append("connect_finished", &[("vendor", vendor)]),
        Err(_) => record.append("connect_failed", &[("vendor", vendor)]),
    }
    outcome
}

async fn sign_in(
    client: &Client,
    db: &VaultDb,
    name: String,
    email: &Secret,
    cancel: &CancellationToken,
    on_prompt: impl FnOnce(&Prompt),
) -> Result<Provider> {
    // A random id per sign-in: the service tells devices apart by it.
    let device_id = Uuid::new_v4().to_string();
    let prompt = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(failure(&SigninError::Cancelled)),
        prompt = client.start(email, &device_id) => prompt.map_err(|e| failure(&e))?,
    };
    if cancel.is_cancelled() {
        return Err(failure(&SigninError::Cancelled));
    }
    on_prompt(&prompt);
    let device_code = client
        .poll(&prompt, cancel)
        .await
        .map_err(|e| failure(&e))?;
    save_connection(db, name, &device_code).await
}

#[cfg(test)]
#[path = "connect_abacus_tests.rs"]
mod tests;
