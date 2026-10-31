//! The native executor of the password actions of agents (spec 034 FR-027, spec 021 US5, research
//! R13): `passwords.items.search` for the built-in agent and every external one, and
//! `passwords.secret.get`, one secret of one entry, for external agents only. Each runs as the
//! calling agent with its own grants; the call prompt in every approval mode comes from the MCP
//! access (`prompts::call_gate`). An entry outside the grant answers like a missing one (FR-029).

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tokio_util::sync::CancellationToken;

use super::access::{Caller, Grant};
use super::model::AgentHeader;
use super::service::PasswordsService;
use crate::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use crate::error::HolziError;

pub const PASSWORD_ACTION_IDS: &[&str] = &["passwords.items.search", "passwords.secret.get"];

/// The prefix of a custom field in the input `field` of `passwords.secret.get`.
const CUSTOM: &str = "custom:";

#[async_trait]
pub trait PasswordsAgentEnv: Send + Sync {
    fn service(&self) -> Result<PasswordsService, HolziError>;

    /// The password grants of the external agent `agent_id`; none when it has none.
    async fn grants(&self, agent_id: &str) -> Vec<Grant>;
}

pub struct PasswordsActions<E> {
    env: E,
}

impl<E: PasswordsAgentEnv> PasswordsActions<E> {
    pub fn new(env: E) -> Self {
        Self { env }
    }
}

#[derive(Deserialize)]
struct SearchInput {
    query: Option<String>,
    tag: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecretInput {
    entry_id: String,
    field: String,
}

/// Which secret of an entry an agent asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum SecretField {
    Password,
    Totp,
    Custom(String),
}

impl SecretField {
    pub fn parse(field: &str) -> Option<Self> {
        match field {
            "password" => Some(Self::Password),
            "totp" => Some(Self::Totp),
            _ => field
                .strip_prefix(CUSTOM)
                .filter(|name| !name.trim().is_empty())
                .map(|name| Self::Custom(name.to_owned())),
        }
    }
}

fn parse<T: for<'de> Deserialize<'de>>(input: Value) -> Result<T, NativeError> {
    serde_json::from_value(input)
        .map_err(|error| NativeError::invalid_input(None, error.to_string()))
}

fn forbidden() -> NativeError {
    NativeError::new(
        "forbidden_for_agents",
        "This action is not available to this agent.",
    )
}

/// A refusal and a missing entry read alike (034 FR-029).
fn no_entry() -> NativeError {
    NativeError::new(
        "not_found",
        "no entry of that id is available to this agent",
    )
}

fn passwords_error(error: HolziError) -> NativeError {
    match error {
        HolziError::PasswordsNotFound | HolziError::PasswordsForbidden => no_entry(),
        other => NativeError::new("failed", other.to_string()),
    }
}

/// The five fields of the result schema, nothing else, absent title and folder left out.
fn header_json(header: AgentHeader) -> Value {
    let mut item = Map::new();
    item.insert("id".to_owned(), Value::String(header.id));
    if let Some(title) = header.title {
        item.insert("title".to_owned(), Value::String(title));
    }
    item.insert("tags".to_owned(), json!(header.tags));
    if let Some(folder) = header.folder {
        item.insert("folder".to_owned(), Value::String(folder));
    }
    item.insert("hasTotp".to_owned(), Value::Bool(header.has_totp));
    Value::Object(item)
}

impl<E: PasswordsAgentEnv> PasswordsActions<E> {
    async fn search(
        &self,
        caller: &Caller,
        grants: &[Grant],
        input: Value,
    ) -> Result<Value, NativeError> {
        let input: SearchInput = parse(input)?;
        let service = self.env.service().map_err(|_| no_vault())?;
        let items = service
            .agent_search(caller, grants, input.query, input.tag, input.limit)
            .await
            .map_err(passwords_error)?;
        Ok(json!({ "items": items.into_iter().map(header_json).collect::<Vec<_>>() }))
    }

    async fn secret(
        &self,
        caller: &Caller,
        grants: &[Grant],
        input: Value,
    ) -> Result<Value, NativeError> {
        let input: SecretInput = parse(input)?;
        let Some(field) = SecretField::parse(&input.field) else {
            return Err(NativeError::invalid_input(
                Some("field"),
                "field is password, totp or custom:<name>",
            ));
        };
        let service = self.env.service().map_err(|_| no_vault())?;
        if field == SecretField::Totp {
            let code = service
                .totp_code(caller, grants, input.entry_id)
                .await
                .map_err(passwords_error)?;
            return Ok(json!({
                "code": code.code,
                "remainingSeconds": code.remaining_seconds,
            }));
        }
        let item = service
            .read_secret_item(caller, grants, input.entry_id)
            .await
            .map_err(passwords_error)?;
        let value = match &field {
            SecretField::Password => item.password.clone(),
            SecretField::Custom(name) => item
                .key_values
                .iter()
                .find(|row| row.key.as_deref() == Some(name.as_str()))
                .and_then(|row| row.value.clone()),
            SecretField::Totp => None,
        };
        value
            .map(|value| json!({ "value": value }))
            .ok_or_else(|| NativeError::new("not_found", "the entry has no such field"))
    }
}

fn no_vault() -> NativeError {
    NativeError::new("failed", "no vault is open")
}

#[async_trait]
impl<E: PasswordsAgentEnv + 'static> NativeExecutor for PasswordsActions<E> {
    fn action_ids(&self) -> &'static [&'static str] {
        PASSWORD_ACTION_IDS
    }

    fn serves_external_agents(&self) -> bool {
        true
    }

    async fn run(
        &self,
        action_id: &str,
        input: Value,
        call: &NativeCall,
        _cancel: &CancellationToken,
    ) -> Result<NativeReply, NativeError> {
        // The built-in agent searches without a grant (Z2) and never reads a secret.
        let grants = match &call.caller {
            Caller::BuiltinAgent if action_id == "passwords.items.search" => Vec::new(),
            Caller::ExternalAgent { id } => self.env.grants(id).await,
            _ => return Err(forbidden()),
        };
        let result = match action_id {
            "passwords.items.search" => self.search(&call.caller, &grants, input).await?,
            "passwords.secret.get" => self.secret(&call.caller, &grants, input).await?,
            other => {
                return Err(NativeError::new(
                    "unknown_action",
                    format!("{other} is no password action"),
                ))
            }
        };
        Ok(NativeReply::from(result))
    }
}

#[cfg(test)]
#[path = "agent_actions_tests.rs"]
mod tests;
