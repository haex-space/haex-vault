//! The native executor of `contacts.search`, `contacts.get` and `contacts.update` (spec 051 US6,
//! ADR 0011): the built-in agent and each external agent (spec 021 US5) reach the address book as
//! themselves through [`ContactsAgent`], which asks the user for a grant they lack. A refusal looks
//! like a missing contact.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use super::agent::ContactsAgent;
use super::model::ContactPatch;
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use crate::error::HolziError;
use crate::files::agent::prompt::PermissionPrompt;
use crate::state::AppState;
use crate::vault_gate::VaultDb;

pub const CONTACT_ACTION_IDS: &[&str] = &["contacts.search", "contacts.get", "contacts.update"];

/// The head fields `contacts.update` takes as text.
const HEAD_KEYS: [&str; 6] = [
    "givenName",
    "familyName",
    "displayName",
    "organization",
    "jobTitle",
    "note",
];

pub trait ContactsAgentEnv: Send + Sync {
    fn database(&self) -> Result<VaultDb, HolziError>;

    fn prompt(&self) -> PermissionPrompt;
}

pub struct ContactsActions<E> {
    env: E,
}

impl<E: ContactsAgentEnv> ContactsActions<E> {
    pub fn new(env: E) -> Self {
        Self { env }
    }
}

#[derive(Deserialize)]
struct SearchInput {
    #[serde(default)]
    query: String,
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct IdInput {
    id: String,
}

/// The service's patch from the input of `contacts.update`: a head field given as text is set, an
/// empty text clears it; values with an id change, without one they are added, ids in `remove` are
/// removed. Agents never change groups.
pub fn patch_of(input: &Value) -> Result<ContactPatch, NativeError> {
    let mut patch = Map::new();
    for key in HEAD_KEYS {
        if let Some(text) = input[key].as_str() {
            let value = if text.trim().is_empty() {
                Value::Null
            } else {
                Value::String(text.to_owned())
            };
            patch.insert(key.to_owned(), value);
        }
    }
    let upsert: Vec<Value> = input["values"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let (kind, text) = (value["kind"].as_str()?, value["value"].as_str()?);
            let mut row = json!({
                "kind": kind,
                "label": value["label"].as_str().unwrap_or("other"),
                "value": text,
            });
            if let Some(id) = value["id"].as_str() {
                row["id"] = Value::String(id.to_owned());
            }
            Some(row)
        })
        .collect();
    let remove: Vec<Value> = input["remove"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|id| id.is_string())
        .cloned()
        .collect();
    if !upsert.is_empty() || !remove.is_empty() {
        patch.insert(
            "values".to_owned(),
            json!({ "upsert": upsert, "remove": remove }),
        );
    }
    serde_json::from_value(Value::Object(patch))
        .map_err(|error| NativeError::invalid_input(None, error.to_string()))
}

fn parse<T: for<'de> Deserialize<'de>>(input: Value) -> Result<T, NativeError> {
    serde_json::from_value(input)
        .map_err(|error| NativeError::invalid_input(None, error.to_string()))
}

fn contacts_error(error: HolziError) -> NativeError {
    match error {
        HolziError::ContactsNotFound | HolziError::ContactsForbidden => NativeError::new(
            "not_found",
            "no contact of that id is available to this agent",
        ),
        HolziError::ContactsInvalid { reason } => NativeError::invalid_input(None, reason),
        other => NativeError::new("failed", other.to_string()),
    }
}

fn to_value(value: impl serde::Serialize) -> Result<Value, NativeError> {
    serde_json::to_value(value)
        .map_err(|_| NativeError::new("failed", "the answer could not be read"))
}

#[async_trait]
impl<E: ContactsAgentEnv + 'static> NativeExecutor for ContactsActions<E> {
    fn action_ids(&self) -> &'static [&'static str] {
        CONTACT_ACTION_IDS
    }

    fn serves_external_agents(&self) -> bool {
        true
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        call: &NativeCall,
        cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        let db = self
            .env
            .database()
            .map_err(|_| NativeError::new("failed", "no vault is open"))?;
        let agent = match &call.caller {
            Caller::BuiltinAgent => ContactsAgent::builtin(db, self.env.prompt()),
            Caller::ExternalAgent { id } => ContactsAgent::external(db, self.env.prompt(), id),
            _ => {
                return Err(NativeError::new(
                    "forbidden_for_agents",
                    "This action is not available to this agent.",
                ))
            }
        };
        let result = match action_id {
            "contacts.search" => {
                let input: SearchInput = parse(input)?;
                let contacts = agent
                    .search(&input.query, input.limit, cancel)
                    .await
                    .map_err(contacts_error)?;
                json!({ "contacts": to_value(contacts)? })
            }
            "contacts.get" => {
                let input: IdInput = parse(input)?;
                to_value(agent.get(&input.id, cancel).await.map_err(contacts_error)?)?
            }
            "contacts.update" => {
                let patch = patch_of(&input)?;
                let input: IdInput = parse(input)?;
                to_value(
                    agent
                        .update(&input.id, patch, cancel)
                        .await
                        .map_err(contacts_error)?,
                )?
            }
            other => {
                return Err(NativeError::new(
                    "unknown_action",
                    format!("{other} is no contact action"),
                ))
            }
        };
        Ok(NativeReply::from(result))
    }
}

/// The environment of the app: the open vault and the question of spec 044.
pub struct TauriContactsAgentEnv {
    app: AppHandle,
}

impl TauriContactsAgentEnv {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ContactsAgentEnv for TauriContactsAgentEnv {
    fn database(&self) -> Result<VaultDb, HolziError> {
        self.app.state::<AppState>().database()
    }

    fn prompt(&self) -> PermissionPrompt {
        PermissionPrompt::clone(&self.app.state::<PermissionPrompt>())
    }
}

#[cfg(test)]
#[path = "agent_actions_tests.rs"]
mod tests;
