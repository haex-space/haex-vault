//! Whose agent profile decides what a caller reaches in the data the agents share (contacts, files;
//! spec 083 FR-032, FR-033): the chat's agent is judged by the model of its turn, a registered agent
//! by its own row, an Abo-Agent by the model it runs with. One place reads and writes it, so no
//! kind of data has a way of its own.

use uuid::Uuid;

use super::{local, remember};
use crate::error::Result;
use crate::files::permissions::BUILTIN_AGENT;
use crate::mcp_server::grants::Rights;
use crate::mcp_server::registry;
use crate::vault_gate::VaultDb;

/// The caller of a data action.
#[derive(Debug, Clone)]
pub enum Identity {
    /// The chat's agent: the model of the running turn, "Standard" outside one.
    Local,
    /// A registered agent of this device; an Abo-Agent counts as the model it runs with.
    Registered { agent: Uuid, device: Uuid },
}

impl Identity {
    /// The identity behind `agent_id` (`builtin` or the id of a registered agent); `None` for an id
    /// that is neither, which has no rights.
    pub fn of(agent_id: &str, device: Uuid) -> Option<Self> {
        if agent_id == BUILTIN_AGENT {
            return Some(Self::Local);
        }
        Some(Self::Registered {
            agent: Uuid::parse_str(agent_id).ok()?,
            device,
        })
    }

    /// The rights that apply now, and the profile they come from. The model of the turn is read
    /// here, before the database thread, which has no task of its own.
    pub async fn rights(&self, db: &VaultDb) -> Result<(Rights, Uuid)> {
        match self.clone() {
            Self::Local => {
                let subject = local::builtin_subject();
                db.read(move |q| {
                    Ok((
                        local::rights_for(q, subject.as_ref())?,
                        remember::profile_to_remember(q, subject.as_ref())?,
                    ))
                })
                .await
            }
            Self::Registered { agent, device } => {
                db.read(move |q| {
                    let subject = registry::subject_of_id(q, device, agent)?;
                    Ok((
                        registry::load_rights(q, device, agent)?,
                        remember::profile_to_remember(q, Some(&subject))?,
                    ))
                })
                .await
            }
        }
    }

    /// Applies `change` to the rights of the profile `expected`, where a question was asked;
    /// `false` when another profile applies by now, so nothing is written.
    pub async fn remember(
        &self,
        db: &VaultDb,
        expected: Uuid,
        change: impl FnOnce(&mut Rights) + Send + 'static,
    ) -> Result<bool> {
        match self {
            Self::Local => {
                let subject = local::builtin_subject();
                db.write(move |tx| {
                    remember::change_rights_of(tx, subject.as_ref(), expected, change)
                })
                .await
            }
            Self::Registered { agent, device } => {
                let (agent, device) = (*agent, *device);
                db.write(move |tx| {
                    // Revocation and this write share one transaction. A permission answer that
                    // arrives after the agent was blocked must not grant Standard new access.
                    if !registry::get(tx, device, agent)?
                        .is_some_and(|row| row.status != registry::AgentStatus::Blocked)
                    {
                        return Ok(false);
                    }
                    let subject = registry::subject_of_id(tx, device, agent)?;
                    remember::change_rights_of(tx, Some(&subject), expected, change)
                })
                .await
            }
        }
    }
}
