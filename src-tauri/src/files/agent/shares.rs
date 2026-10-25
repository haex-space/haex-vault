//! The native executor of the `files.shares.*` actions for agents (spec 056 US6, FR-045, research
//! R17). Every call needs the agent's permission "Dateifreigaben verwalten". Giving files away — a
//! new share, another contact, a higher level — waits for the user's confirmation in the window,
//! every time and whatever the approval mode; each contact it reaches must be one the agent may read
//! through its contacts permission (spec 051), and a new share's folder one it may read. Taking away
//! — revoke, a lower level, delete — runs without a question. The window runs the same actions for
//! the user through its own handlers.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::reach::{files_error, parse};
use super::share_confirm::{FileShareConfirmGrant, FileShareConfirmRequest, ShareConfirm};
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeError, NativeExecutor, NativeReply};
use crate::contacts::agent::grants_from;
use crate::contacts::service::{ContactsService, VaultCard};
use crate::files::access::{check, Target, Verdict, Want};
use crate::files::encrypted::route::{route, Route};
use crate::files::local::{resolve, OwnPlaces};
use crate::files::peer::connect::key_of;
use crate::files::peer::serve::storage::StorageOpener;
use crate::files::permissions::{self, BUILTIN_AGENT};
use crate::files::share::manage::{self, FileShare, FileShareSaveArgs, Here};
use crate::files::share::{FileShareLevel, FEATURE};
use crate::files::{FilesError, FilesErrorCode, SourceRef};
use crate::vault_gate::VaultDb;

/// The actions this executor runs.
pub const SHARE_ACTION_IDS: &[&str] = &[
    "files.shares.list",
    "files.shares.of",
    "files.shares.save",
    "files.shares.revoke",
    "files.shares.delete",
    "files.shares.ofContact",
];

/// What the executor needs from the running app.
#[async_trait]
pub trait SharesEnv: Send + Sync {
    /// The open vault and this device's `vault_device_uuid`.
    async fn vault(&self) -> Result<(VaultDb, String), FilesError>;

    /// holzi's own places, never shared (FR-007).
    fn own(&self) -> OwnPlaces;

    fn confirm(&self) -> &ShareConfirm;

    /// Opens a storage of this vault, for a share of it (T081); none: storages are not shared.
    fn storages(&self) -> Option<StorageOpener> {
        None
    }
}

/// The share actions of one agent.
pub struct SharesAgent<E> {
    env: E,
    agent_id: String,
    caller: Caller,
}

impl<E: SharesEnv> SharesAgent<E> {
    /// The executor of the agent of the holzi chat.
    pub fn builtin(env: E) -> Self {
        Self {
            env,
            agent_id: BUILTIN_AGENT.to_owned(),
            caller: Caller::BuiltinAgent,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfInput {
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevokeInput {
    share_id: String,
    vault_pubkey: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareInput {
    share_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContactInput {
    contact_id: String,
}

fn refused(message: &str) -> NativeError {
    files_error(FilesError::new(FilesErrorCode::NotGranted, message))
}

/// A contact the agent may not read, or none: the same answer (spec 051 FR-030).
fn no_contact() -> NativeError {
    refused("no contact of that id or vault is available to this agent")
}

fn value_of<T: serde::Serialize>(value: &T) -> Result<Value, NativeError> {
    serde_json::to_value(value).map_err(|_| NativeError::new("failed", "the action failed"))
}

#[async_trait]
impl<E: SharesEnv + 'static> NativeExecutor for SharesAgent<E> {
    fn action_ids(&self) -> &'static [&'static str] {
        SHARE_ACTION_IDS
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        let (db, device) = self.env.vault().await.map_err(files_error)?;
        let agent = self.agent_id.clone();
        let grants = db
            .read(move |q| Ok(permissions::grants_of(q, &agent)?))
            .await
            .map_err(|_| NativeError::new("failed", "the permissions could not be read"))?;
        if !grants.file_shares {
            return Err(refused(
                "managing file shares is not granted to this agent: the user turns on \"Dateifreigaben verwalten\" in its settings",
            ));
        }
        let own = self.env.own();
        let here = Here {
            db: &db,
            device: &device,
            own: &own,
        };
        let result = match action_id {
            "files.shares.list" => {
                let shares = manage::list(&here).await.map_err(files_error)?;
                value_of(&self.redact_shares(&db, shares).await)
            }
            "files.shares.of" => {
                let input: OfInput = parse(input)?;
                let share = manage::of(&here, SourceRef::Device, input.path)
                    .await
                    .map_err(files_error)?;
                let share = match share {
                    Some(share) => Some(self.redact_share(&db, share).await),
                    None => None,
                };
                Ok(share.map_or_else(|| json!({}), |share| json!({ "share": share })))
            }
            "files.shares.ofContact" => {
                let input: ContactInput = parse(input)?;
                self.readable(&db, &input.contact_id).await?;
                let shares = manage::of_contact(&here, input.contact_id)
                    .await
                    .map_err(files_error)?;
                value_of(&self.redact_shares(&db, shares).await)
            }
            "files.shares.revoke" => {
                let input: RevokeInput = parse(input)?;
                let share = manage::revoke(&here, input.share_id, input.vault_pubkey)
                    .await
                    .map_err(files_error)?;
                value_of(&self.redact_share(&db, share).await)
            }
            "files.shares.delete" => {
                let input: ShareInput = parse(input)?;
                manage::delete(&here, input.share_id)
                    .await
                    .map_err(files_error)?;
                Ok(json!({}))
            }
            "files.shares.save" => self.save(&here, &grants, parse(input)?, cancel).await,
            other => Err(NativeError::new(
                "unknown_action",
                format!("{other} is no share action"),
            )),
        }?;
        Ok(NativeReply::from(result))
    }
}

impl<E: SharesEnv> SharesAgent<E> {
    /// Removes contact identities the agent is not allowed to read from share results.
    async fn redact_share(&self, db: &VaultDb, mut share: FileShare) -> FileShare {
        for grant in &mut share.grants {
            let Some(contact_id) = grant.contact_id.clone() else {
                continue;
            };
            if self.readable(db, &contact_id).await.is_err() {
                grant.contact_id = None;
                grant.contact_name = None;
            }
        }
        share
    }

    async fn redact_shares(&self, db: &VaultDb, shares: Vec<FileShare>) -> Vec<FileShare> {
        let mut redacted = Vec::with_capacity(shares.len());
        for share in shares {
            redacted.push(self.redact_share(db, share).await);
        }
        redacted
    }

    /// Creates or changes a share; what gives more away waits for the user (FR-045).
    async fn save(
        &self,
        here: &Here<'_>,
        grants: &crate::files::access::AgentGrants,
        args: FileShareSaveArgs,
        cancel: &CancellationToken,
    ) -> Result<Value, NativeError> {
        let existing = match &args.share_id {
            Some(id) => Some(
                manage::list(here)
                    .await
                    .map_err(files_error)?
                    .into_iter()
                    .find(|share| &share.id == id)
                    .ok_or_else(|| {
                        files_error(FilesError::new(FilesErrorCode::NotFound, "no such share"))
                    })?,
            ),
            None => manage::of(here, args.source.clone(), args.path.clone())
                .await
                .map_err(files_error)?,
        };
        if existing.is_none() {
            self.may_share(&args, grants).await?;
        }
        let before: HashMap<&str, FileShareLevel> = existing
            .iter()
            .flat_map(|share| &share.grants)
            .map(|grant| (grant.vault_pubkey.as_str(), grant.level))
            .collect();
        let mut wider = Vec::new();
        for grant in &args.grants {
            if before
                .get(grant.vault_pubkey.as_str())
                .is_some_and(|level| *level >= grant.level)
            {
                continue;
            }
            wider.push(FileShareConfirmGrant {
                contact_name: self.contact_of_vault(here.db, &grant.vault_pubkey).await?,
                level: grant.level,
            });
        }
        if existing.is_none() || !wider.is_empty() {
            let request = confirm_request(&self.agent_id, existing.as_ref(), &args, wider);
            if !self.env.confirm().ask(&request, cancel).await {
                return Err(NativeError::new(
                    "denied_by_user",
                    "the user did not confirm the share",
                ));
            }
        }
        let storages = self.env.storages();
        let share = manage::save_from(here, args, storages.as_ref())
            .await
            .map_err(files_error)?;
        value_of(&self.redact_share(here.db, share).await)
    }

    /// Whether the agent may share the folder or file `args` names: one it may read itself.
    async fn may_share(
        &self,
        args: &FileShareSaveArgs,
        grants: &crate::files::access::AgentGrants,
    ) -> Result<(), NativeError> {
        if let SourceRef::Storage { storage_id } = &args.source {
            return self.may_share_storage(storage_id, &args.path, grants).await;
        }
        let path = args.path.clone();
        let real = tokio::task::spawn_blocking(move || resolve(std::path::Path::new(&path)))
            .await
            .map_err(|_| NativeError::new("failed", "the action failed"))?
            .map_err(files_error)?;
        match check(
            &self.caller,
            &Target::Device(real),
            Want::Read,
            &self.env.own(),
            grants,
        ) {
            Verdict::Allowed => Ok(()),
            Verdict::Refused(error) => Err(files_error(error)),
            Verdict::ReadOnly | Verdict::Ask => Err(refused("not granted")),
        }
    }

    /// A folder or file of a storage the agent may read itself, outside every encrypted folder: what
    /// an encrypted folder holds is given away by the user only.
    async fn may_share_storage(
        &self,
        storage_id: &str,
        path: &str,
        grants: &crate::files::access::AgentGrants,
    ) -> Result<(), NativeError> {
        match check(
            &self.caller,
            &Target::Storage(storage_id.to_owned()),
            Want::Read,
            &self.env.own(),
            grants,
        ) {
            Verdict::Allowed => {}
            Verdict::Refused(error) => return Err(files_error(error)),
            Verdict::ReadOnly | Verdict::Ask => return Err(refused("not granted")),
        }
        let opener = self
            .env
            .storages()
            .ok_or_else(|| refused("storages cannot be shared here"))?;
        let files = opener(storage_id.to_owned()).await.map_err(files_error)?;
        match route(&files, path).await.map_err(files_error)? {
            Route::Plain => Ok(()),
            Route::Encrypted(_) => Err(refused(
                "the contents of an encrypted folder are shared by the user only",
            )),
        }
    }

    /// The name of the contact that carries the card of `vault`, when the agent may read it.
    async fn contact_of_vault(&self, db: &VaultDb, vault: &str) -> Result<String, NativeError> {
        let key = key_of(vault).ok_or_else(no_contact)?;
        let internal = Caller::Internal { feature: FEATURE };
        let VaultCard::Stored { contact_id, card } = ContactsService::new(db.clone())
            .card_of_vault(&internal, key)
            .await
            .map_err(|_| no_contact())?
        else {
            return Err(no_contact());
        };
        let name = self.readable(db, &contact_id).await?;
        Ok(if !name.is_empty() {
            name
        } else if !card.card.name.trim().is_empty() {
            card.card.name
        } else {
            crate::contacts::card::received::npub(&card.card.vault)
        })
    }

    /// The name of the contact `contact_id`, unless the agent may not read it (spec 051 FR-030); it
    /// is not asked here, the agent reads contacts through their own actions.
    async fn readable(&self, db: &VaultDb, contact_id: &str) -> Result<String, NativeError> {
        let agent = self.agent_id.clone();
        let rows = db
            .read(move |q| Ok(permissions::contacts_of(q, &agent)?))
            .await
            .map_err(|_| no_contact())?;
        ContactsService::new(db.clone())
            .get(&self.caller, &grants_from(&rows), contact_id)
            .await
            .map(|contact| contact.name)
            .map_err(|_| no_contact())
    }
}

/// What the window shows: the share, and the contacts that are new or get more.
fn confirm_request(
    agent_id: &str,
    existing: Option<&FileShare>,
    args: &FileShareSaveArgs,
    grants: Vec<FileShareConfirmGrant>,
) -> FileShareConfirmRequest {
    FileShareConfirmRequest {
        request_id: String::new(),
        agent_id: agent_id.to_owned(),
        new_share: existing.is_none(),
        share_name: args.name.clone(),
        path: existing.map_or_else(|| args.path.clone(), |share| share.path.clone()),
        device_name: existing
            .filter(|share| !share.serving_device.is_current)
            .map(|share| share.serving_device.name.clone().unwrap_or_default()),
        grants,
    }
}

#[cfg(test)]
#[path = "shares_tests.rs"]
mod tests;
