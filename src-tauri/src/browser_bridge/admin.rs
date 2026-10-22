//! Managing approved and blocked clients from the settings (spec 049, US5, FR-037, FR-038): the
//! lists, changing an approval, revoking and unblocking. With a running bridge every change reaches
//! the session decisions and the open connections at once; without one only the remembered rows
//! change.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::approvals::{self, validate, Approval, ClientIdentity, Status, StoredClient};
use super::declaration::{BridgeAccess, Declared};
use super::prompts::BridgeDeclaredGrant;
use super::runtime::{BridgeCore, Control};
use crate::error::{HolziError, Result};
use crate::vault_gate::VaultDb;

/// One client as the settings show it (`contracts/tauri-commands.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgeClientView {
    pub client_id: String,
    /// The client's own claim (FR-008).
    pub client_name: String,
    pub origin: String,
    /// `None` for a blocked client.
    pub access: Option<BridgeAccess>,
    pub scope_tags: Option<Vec<String>>,
    pub default_tag: Option<String>,
    /// The widest grant the declaration allows; the editor offers nothing beyond it (FR-010).
    pub declared: Option<BridgeDeclaredGrant>,
    pub declares_write: bool,
    pub remembered: bool,
    pub connected: bool,
    #[ts(type = "number")]
    pub decided_at: i64,
    #[ts(type = "number | null")]
    pub last_seen_at: Option<i64>,
}

/// Approved and blocked clients, newest decision first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientLists {
    pub clients: Vec<BridgeClientView>,
    pub blocked: Vec<BridgeClientView>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgeClientUpdate {
    pub client_id: String,
    pub access: BridgeAccess,
    pub scope_tags: Option<Vec<String>>,
    pub default_tag: Option<String>,
}

/// The clients of one device, and the running bridge when there is one.
pub struct ClientAdmin {
    pub db: VaultDb,
    pub device: Uuid,
    pub core: Option<Arc<BridgeCore>>,
}

impl ClientAdmin {
    /// Both lists. A session decision hides a remembered one for the same client, and a block
    /// hides an approval, as [`approvals::decide`] weighs them.
    pub async fn overview(&self) -> Result<ClientLists> {
        let device = self.device;
        let stored = self.db.read(move |q| approvals::list(q, device)).await?;
        // Lower rank wins: session block, remembered block, session approval, remembered approval.
        let mut ranked: BTreeMap<String, (u8, BridgeClientView)> = BTreeMap::new();
        let mut offer = |rank: u8, view: BridgeClientView| {
            let keep = ranked
                .get(&view.client_id)
                .is_some_and(|(held, _)| *held <= rank);
            if !keep {
                ranked.insert(view.client_id.clone(), (rank, view));
            }
        };
        for row in &stored {
            let (rank, approval) = match &row.status {
                Status::Blocked => (1, None),
                Status::Allowed(approval) => (3, Some(approval)),
            };
            offer(
                rank,
                view(
                    &row.identity,
                    approval,
                    true,
                    row.decided_at,
                    row.last_seen_at,
                ),
            );
        }
        if let Some(core) = &self.core {
            let session = core.session();
            for (identity, decided_at) in session.all_blocked() {
                offer(0, view(identity, None, false, *decided_at, None));
            }
            for entry in session.all_allowed() {
                let approval = Some(&entry.approval);
                offer(
                    2,
                    view(&entry.identity, approval, false, entry.decided_at, None),
                );
            }
        }
        let mut lists = ClientLists::default();
        for (_, (_, mut view)) in ranked {
            view.connected = self
                .core
                .as_ref()
                .is_some_and(|core| core.is_connected(&view.client_id));
            if view.access.is_some() {
                lists.clients.push(view);
            } else {
                lists.blocked.push(view);
            }
        }
        for list in [&mut lists.clients, &mut lists.blocked] {
            list.sort_by_key(|view| std::cmp::Reverse(view.decided_at));
        }
        Ok(lists)
    }

    /// Changes an approval; it holds from the next request on (FR-038). Never wider than the
    /// declaration, checked like an answer to the approval question. The answers about writing are
    /// forgotten, since the user just decided anew.
    pub async fn update(&self, update: BridgeClientUpdate) -> Result<()> {
        let BridgeClientUpdate {
            client_id,
            access,
            scope_tags,
            default_tag,
        } = update;
        let check = |identity: &ClientIdentity| {
            let declared = Declared::from_canonical(&identity.declaration).ok_or_else(|| {
                HolziError::InvalidInput {
                    reason: "the client has no readable declaration".to_owned(),
                }
            })?;
            validate(&declared, access, scope_tags.clone(), default_tag.clone()).map_err(|error| {
                HolziError::InvalidInput {
                    reason: error.to_string(),
                }
            })
        };
        if let Some(core) = &self.core {
            let mut session = core.session();
            if let Some(entry) = session.allowed_mut(&client_id) {
                entry.approval = check(&entry.identity)?;
                session.reset_write(&client_id);
                return Ok(());
            }
        }
        let device = self.device;
        let id = client_id.clone();
        let Some(mut row) = self
            .db
            .read(move |q| approvals::load(q, device, &id))
            .await?
            .filter(|row| matches!(row.status, Status::Allowed(_)))
        else {
            return Err(HolziError::BrowserBridgeClientNotFound);
        };
        let approval = check(&row.identity)?;
        let updated = self
            .db
            .write(move |tx| approvals::update_allowed(tx, device, &row.identity, &approval))
            .await?;
        if !updated {
            return Err(HolziError::BrowserBridgeClientNotFound);
        }
        if let Some(core) = &self.core {
            core.session().reset_write(&client_id);
        }
        Ok(())
    }

    /// Withdraws an approval: open connections hear `authorizationUpdate(false)` and close, which
    /// ends their questions and requests; the next connect asks again (FR-011).
    pub async fn revoke(&self, client_id: &str) -> Result<()> {
        let in_session = self
            .core
            .as_ref()
            .is_some_and(|core| core.session().revoke(client_id));
        let stored = self
            .forget_stored(client_id, |status| matches!(status, Status::Allowed(_)))
            .await?;
        if !(in_session || stored) {
            return Err(HolziError::BrowserBridgeClientNotFound);
        }
        if let Some(core) = &self.core {
            core.broadcast(client_id, Control::Denied);
        }
        Ok(())
    }

    /// Lifts a block; the next connect asks again.
    pub async fn unblock(&self, client_id: &str) -> Result<()> {
        let in_session = self
            .core
            .as_ref()
            .is_some_and(|core| core.session().unblock(client_id));
        let stored = self
            .forget_stored(client_id, |status| matches!(status, Status::Blocked))
            .await?;
        if in_session || stored {
            Ok(())
        } else {
            Err(HolziError::BrowserBridgeClientNotFound)
        }
    }

    /// Deletes the remembered row of a client when its status is the one meant.
    async fn forget_stored(&self, client_id: &str, meant: fn(&Status) -> bool) -> Result<bool> {
        let device = self.device;
        let client_id = client_id.to_owned();
        self.db
            .write(move |tx| match approvals::load(tx, device, &client_id)? {
                Some(StoredClient { status, .. }) if meant(&status) => {
                    approvals::delete(tx, device, &client_id)
                }
                _ => Ok(false),
            })
            .await
    }
}

fn view(
    identity: &ClientIdentity,
    approval: Option<&Approval>,
    remembered: bool,
    decided_at: i64,
    last_seen_at: Option<i64>,
) -> BridgeClientView {
    let declared = Declared::from_canonical(&identity.declaration);
    BridgeClientView {
        client_id: identity.client_id.clone(),
        client_name: identity.client_name.clone(),
        origin: identity.origin.clone(),
        access: approval.map(|a| a.access),
        scope_tags: approval.and_then(|a| a.scope_tags.clone()),
        default_tag: approval.and_then(|a| a.default_tag.clone()),
        declared: declared.as_ref().map(|d| {
            let (access, scope_tags) = d.proposal();
            BridgeDeclaredGrant { access, scope_tags }
        }),
        declares_write: declared.as_ref().is_some_and(Declared::declares_write),
        remembered,
        connected: false,
        decided_at,
        last_seen_at,
    }
}
