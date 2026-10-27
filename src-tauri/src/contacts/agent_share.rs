//! The native executor of `contacts.share` for agents (spec 060 US5, FR-029 to FR-032, research
//! R8). The agent needs the permission "Kontakte lesen" for the contact, which is asked for like
//! for every contact action; the contact it may not read looks like a missing one. Sharing it
//! waits for the user in the share dialog, **every time** and without "remember": the user sees
//! the agent's proposal, may change it, and only what is left standing goes into the text. The
//! agent never gets more than the dialog offers, and with a "no" it gets nothing of the contact.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use super::agent::ContactsAgent;
use super::agent_share_confirm::{
    ContactShareConfirm, ContactShareConfirmRequest, EVENT_CONTACT_SHARE_CONFIRM,
};
use super::share::build;
use super::share::model::{
    ContactShareHolzi, ContactShareHolziChoice, ContactShareOptions, ContactShareSelection,
};
use crate::chat::tools::native_action::{NativeError, NativeExecutor, NativeReply};
use crate::error::HolziError;
use crate::files::agent::prompt::PermissionPrompt;
use crate::files::permissions::BUILTIN_AGENT;
use crate::state::AppState;
use crate::vault_gate::VaultDb;

/// The actions this executor runs.
pub const SHARE_ACTION_IDS: &[&str] = &["contacts.share"];

/// What the executor needs from the running app.
pub trait ContactsShareEnv: Send + Sync {
    fn database(&self) -> Result<VaultDb, HolziError>;

    /// The question about the permission "Kontakte lesen".
    fn prompt(&self) -> PermissionPrompt;

    fn confirm(&self) -> &ContactShareConfirm;
}

/// The share action of the built-in agent.
pub struct ContactsShareAgent<E> {
    env: E,
    agent_id: String,
}

impl<E: ContactsShareEnv> ContactsShareAgent<E> {
    pub fn builtin(env: E) -> Self {
        Self {
            env,
            agent_id: BUILTIN_AGENT.to_owned(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareInput {
    contact_id: String,
    #[serde(default)]
    selection: Option<Proposal>,
}

/// What the agent proposes: any part it leaves out is "everything the dialog offers".
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Proposal {
    values: Option<Vec<String>>,
    display_name: Option<bool>,
    nickname: Option<bool>,
    organization: Option<bool>,
    job_title: Option<bool>,
    note: Option<bool>,
    holzi: Option<ContactShareHolziChoice>,
}

/// A contact the agent may not read, or none: the same answer (spec 051 FR-030).
fn no_contact() -> NativeError {
    NativeError::new(
        "not_found",
        "no contact of that id is available to this agent",
    )
}

fn failed(message: &str) -> NativeError {
    NativeError::new("failed", message)
}

/// Everything the dialog offers, chosen.
fn everything(options: &ContactShareOptions) -> ContactShareSelection {
    ContactShareSelection {
        values: options.rows.iter().map(|row| row.id.clone()).collect(),
        display_name: options.head.display_name.is_some(),
        nickname: options.head.nickname.is_some(),
        organization: options.head.organization.is_some(),
        job_title: options.head.job_title.is_some(),
        note: options.head.note.is_some(),
        holzi: match &options.holzi {
            ContactShareHolzi::None
            | ContactShareHolzi::Own {
                can_sign: false, ..
            } => ContactShareHolziChoice::None,
            ContactShareHolzi::Card { .. } => ContactShareHolziChoice::Card,
            ContactShareHolzi::Own {
                devices, relays, ..
            } => ContactShareHolziChoice::Own {
                devices: devices
                    .iter()
                    .map(|device| device.device_pubkey.clone())
                    .collect(),
                relays: relays.clone(),
            },
        },
    }
}

/// `wanted` cut down to what the dialog offers: never more (FR-031), whoever wanted it.
fn restrict(
    options: &ContactShareOptions,
    wanted: &ContactShareSelection,
) -> ContactShareSelection {
    let all = everything(options);
    let mut values: Vec<String> = Vec::new();
    for id in &wanted.values {
        if all.values.contains(id) && !values.contains(id) {
            values.push(id.clone());
        }
    }
    let holzi = match (&wanted.holzi, &all.holzi) {
        (ContactShareHolziChoice::Card, ContactShareHolziChoice::Card) => {
            ContactShareHolziChoice::Card
        }
        (
            ContactShareHolziChoice::Own { devices, relays },
            ContactShareHolziChoice::Own {
                devices: offered_devices,
                relays: offered_relays,
            },
        ) => ContactShareHolziChoice::Own {
            devices: devices
                .iter()
                .filter(|device| offered_devices.contains(device))
                .cloned()
                .collect(),
            relays: relays
                .iter()
                .filter(|relay| offered_relays.contains(relay))
                .cloned()
                .collect(),
        },
        _ => ContactShareHolziChoice::None,
    };
    ContactShareSelection {
        values,
        display_name: wanted.display_name && all.display_name,
        nickname: wanted.nickname && all.nickname,
        organization: wanted.organization && all.organization,
        job_title: wanted.job_title && all.job_title,
        note: wanted.note && all.note,
        holzi,
    }
}

impl Proposal {
    /// The proposal as a selection inside the dialog's offer.
    fn within(self, options: &ContactShareOptions) -> ContactShareSelection {
        let all = everything(options);
        restrict(
            options,
            &ContactShareSelection {
                values: self.values.unwrap_or(all.values),
                display_name: self.display_name.unwrap_or(all.display_name),
                nickname: self.nickname.unwrap_or(all.nickname),
                organization: self.organization.unwrap_or(all.organization),
                job_title: self.job_title.unwrap_or(all.job_title),
                note: self.note.unwrap_or(all.note),
                holzi: self.holzi.unwrap_or(all.holzi),
            },
        )
    }
}

#[async_trait]
impl<E: ContactsShareEnv + 'static> NativeExecutor for ContactsShareAgent<E> {
    fn action_ids(&self) -> &'static [&'static str] {
        SHARE_ACTION_IDS
    }

    fn confirming_action_ids(&self) -> &'static [&'static str] {
        SHARE_ACTION_IDS
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        if action_id != "contacts.share" {
            return Err(NativeError::new(
                "unknown_action",
                format!("{action_id} is no contact share action"),
            ));
        }
        let input: ShareInput = serde_json::from_value(input)
            .map_err(|_| NativeError::invalid_input(None, "contactId is needed"))?;
        let db = self
            .env
            .database()
            .map_err(|_| failed("no vault is open"))?;
        // The permission "Kontakte lesen": asked for like by every contact action, and a refusal
        // looks like a missing contact.
        let contact = ContactsAgent::builtin(db.clone(), self.env.prompt())
            .get(&input.contact_id, cancel)
            .await
            .map_err(|error| match error {
                HolziError::ContactsNotFound | HolziError::ContactsForbidden => no_contact(),
                _ => failed("the contact could not be read"),
            })?;

        let id = input.contact_id.clone();
        let options = db
            .read(move |q| build::options(q, &id).map_err(Into::into))
            .await
            .map_err(|_| failed("the contact could not be read"))?;
        if !options.can_share {
            return Err(NativeError::new(
                "nothing_to_share",
                "the contact has nothing that could be shared",
            ));
        }
        let proposed = input.selection.unwrap_or_default().within(&options);
        let request = ContactShareConfirmRequest {
            request_id: String::new(),
            agent_id: self.agent_id.clone(),
            contact_id: input.contact_id.clone(),
            contact_name: contact.display_name,
            proposed,
            options: options.clone(),
        };
        // Every call asks, with no way to remember an answer.
        let Some(confirmed) = self.env.confirm().ask(&request, cancel).await else {
            return Err(NativeError::new(
                "denied_by_user",
                "the user did not confirm sharing the contact",
            ));
        };
        let chosen = restrict(&options, &confirmed);
        let id = input.contact_id;
        let output = db
            .read(move |q| build::build(q, &id, &chosen).map_err(Into::into))
            .await
            .map_err(|_| failed("the share could not be built"))?;
        Ok(NativeReply::from(json!({
            "link": output.link,
            "qrAvailable": output.qr_svg.is_some(),
            "size": output.size,
        })))
    }
}

/// What the app gives the executor: the open vault, the permission question and the dialog.
pub struct TauriContactsShareEnv {
    app: AppHandle,
    confirm: ContactShareConfirm,
}

impl TauriContactsShareEnv {
    /// Sends the confirmation to the window, if there is one (no window: refused).
    pub fn new(app: AppHandle, confirm: ContactShareConfirm) -> Self {
        let emitting = app.clone();
        confirm.set_emitter(std::sync::Arc::new(
            move |request: &ContactShareConfirmRequest| {
                !emitting.webview_windows().is_empty()
                    && emitting.emit(EVENT_CONTACT_SHARE_CONFIRM, request).is_ok()
            },
        ));
        Self { app, confirm }
    }
}

impl ContactsShareEnv for TauriContactsShareEnv {
    fn database(&self) -> Result<VaultDb, HolziError> {
        self.app.state::<AppState>().database()
    }

    fn prompt(&self) -> PermissionPrompt {
        PermissionPrompt::clone(&self.app.state::<PermissionPrompt>())
    }

    fn confirm(&self) -> &ContactShareConfirm {
        &self.confirm
    }
}

#[cfg(test)]
#[path = "agent_share_tests.rs"]
mod tests;
