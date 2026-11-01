//! Where the password actions find what an external agent may do (spec 021 US5, T076): its group
//! `passwords` in `haex_mcp_agent_grants` on this device, turned into the grant the password
//! manager checks, like the approval of a browser client. An agent without the group, or one this
//! device does not know, gets no grant.

use async_trait::async_trait;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use super::registry;
use crate::error::HolziError;
use crate::passwords::access::Grant;
use crate::passwords::agent_actions::PasswordsAgentEnv;
use crate::passwords::service::PasswordsService;
use crate::state::AppState;
use crate::storage::wm_session_commands::current_device_uuid;

pub struct TauriPasswordsAgentEnv {
    app: AppHandle,
}

impl TauriPasswordsAgentEnv {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl PasswordsAgentEnv for TauriPasswordsAgentEnv {
    fn service(&self) -> Result<PasswordsService, HolziError> {
        let state = self.app.state::<AppState>();
        Ok(PasswordsService::with_usage(
            state.database()?,
            state.usage(),
        ))
    }

    async fn grants(&self, agent_id: &str) -> Vec<Grant> {
        let Ok(agent) = Uuid::parse_str(agent_id) else {
            return Vec::new();
        };
        let Ok(db) = self.app.state::<AppState>().database() else {
            return Vec::new();
        };
        let Ok(device) = current_device_uuid(&self.app, &db) else {
            return Vec::new();
        };
        match db
            .read(move |q| registry::load_rights(q, device, agent))
            .await
        {
            Ok(rights) => rights
                .passwords
                .map(|passwords| passwords.grants())
                .unwrap_or_default(),
            Err(error) => {
                log::warn!("mcp server: could not read an agent's password rights: {error}");
                Vec::new()
            }
        }
    }
}
