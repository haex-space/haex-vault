//! Approvals and blocks of clients (spec 049, FR-008 to FR-012, data-model): remembered ones in
//! `haex_browser_clients` for this device, session ones in memory until the bridge stops or the
//! vault closes. Denial beats approval.

use std::collections::{BTreeSet, HashMap, HashSet};

use haex_crdt::rusqlite::params;
use haex_crdt::{CrdtTransaction, Result};
use uuid::Uuid;

use super::declaration::{BridgeAccess, Declared};
use crate::passwords::access::{Grant, GrantAction, Scope};
use crate::storage::query::Query;

/// Namespace of `haex_browser_clients.id`: `UUIDv5(NS, vault_device_uuid ":" client_id)`.
const NS_BROWSER_CLIENT: Uuid = Uuid::from_u128(0x4b0e_6f53_9c1a_4d8e_a2f7_51c3_0b9d_e049);

/// The longest client name holzi keeps; the name is the client's own claim and only shown.
pub const MAX_NAME_CHARS: usize = 100;

/// One client per device has one row; the id says which.
pub fn row_id(device: Uuid, client_id: &str) -> Uuid {
    Uuid::new_v5(
        &NS_BROWSER_CLIENT,
        format!("{device}:{client_id}").as_bytes(),
    )
}

/// Who a client is, as its handshake says: a remembered decision only holds while all of it
/// matches (FR-012, research R2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    /// The fingerprint, checked against the key (FR-006).
    pub client_id: String,
    pub public_key: String,
    /// The client's own claim, cut to [`MAX_NAME_CHARS`].
    pub client_name: String,
    pub origin: String,
    /// The canonical declaration.
    pub declaration: String,
}

impl ClientIdentity {
    /// Whether `other` is the same client making the same claims.
    pub fn matches(&self, other: &ClientIdentity) -> bool {
        self.client_id == other.client_id
            && self.public_key == other.public_key
            && self.origin == other.origin
            && self.declaration == other.declaration
    }
}

/// Cuts a claimed name to the kept length.
pub fn kept_name(name: Option<&str>) -> String {
    name.unwrap_or_default()
        .trim()
        .chars()
        .take(MAX_NAME_CHARS)
        .collect()
}

/// What an approved client may do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub access: BridgeAccess,
    /// `None`: every entry.
    pub scope_tags: Option<Vec<String>>,
    /// The tag new entries get (FR-010, FR-022).
    pub default_tag: Option<String>,
    /// A remembered "no" to writing (FR-015).
    pub write_denied: bool,
}

impl Approval {
    /// The one grant of the approval (data-model, "Aufrufer und Freigabe").
    pub fn grants(&self) -> Vec<Grant> {
        let action = match self.access {
            BridgeAccess::Read => GrantAction::Read,
            BridgeAccess::ReadWrite => GrantAction::ReadWrite,
        };
        let scope = match &self.scope_tags {
            None => Scope::All,
            Some(tags) => Scope::tags(tags.iter()),
        };
        vec![Grant::new(action, scope)]
    }
}

/// Why an approval the window sent is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidApproval {
    #[error("the grant is wider than the declaration")]
    WiderThanDeclared,
    #[error("a tag scope needs at least one tag")]
    EmptyScope,
    #[error("reading and writing with a tag scope needs a default tag in the scope")]
    DefaultTag,
}

/// Checks an approval from the window against the declaration (FR-010): never wider than declared,
/// a tag scope with tags, and for reading and writing in a tag scope a default tag inside it.
pub fn validate(
    declared: &Declared,
    access: BridgeAccess,
    scope_tags: Option<Vec<String>>,
    default_tag: Option<String>,
) -> std::result::Result<Approval, InvalidApproval> {
    let scope_tags = scope_tags.map(|tags| {
        tags.iter()
            .map(|tag| tag.trim().to_owned())
            .filter(|tag| !tag.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    });
    if scope_tags.as_ref().is_some_and(Vec::is_empty) {
        return Err(InvalidApproval::EmptyScope);
    }
    if !declared.fits(access, scope_tags.as_deref()) {
        return Err(InvalidApproval::WiderThanDeclared);
    }
    let default_tag = default_tag
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty());
    if let (Some(tags), Some(tag)) = (&scope_tags, &default_tag) {
        if !tags.contains(tag) {
            return Err(InvalidApproval::DefaultTag);
        }
    }
    if access == BridgeAccess::ReadWrite && scope_tags.is_some() && default_tag.is_none() {
        return Err(InvalidApproval::DefaultTag);
    }
    Ok(Approval {
        access,
        scope_tags,
        default_tag,
        write_denied: false,
    })
}

/// A remembered decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Allowed(Approval),
    Blocked,
}

/// A row of `haex_browser_clients`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClient {
    pub identity: ClientIdentity,
    pub status: Status,
    pub decided_at: i64,
    pub last_seen_at: Option<i64>,
}

const COLUMNS: &str = "client_id, public_key, client_name, origin, declaration, status, access, \
                       scope_tags, default_tag, write_denied, decided_at, last_seen_at";

fn from_row(
    row: &haex_crdt::rusqlite::Row<'_>,
) -> haex_crdt::rusqlite::Result<Option<StoredClient>> {
    let identity = ClientIdentity {
        client_id: row.get(0)?,
        public_key: row.get(1)?,
        client_name: row.get(2)?,
        origin: row.get(3)?,
        declaration: row.get(4)?,
    };
    let status: String = row.get(5)?;
    let access: Option<String> = row.get(6)?;
    let scope_tags: Option<String> = row.get(7)?;
    let status = match status.as_str() {
        "blocked" => Status::Blocked,
        "allowed" => {
            // A row another device or an old build wrote in a shape this one cannot read counts as
            // absent, so holzi asks again instead of guessing a grant.
            let Some(access) = access.as_deref().and_then(BridgeAccess::parse) else {
                return Ok(None);
            };
            let scope_tags = match scope_tags {
                None => None,
                Some(json) => match serde_json::from_str::<Vec<String>>(&json) {
                    Ok(tags) => Some(tags),
                    Err(_) => return Ok(None),
                },
            };
            Status::Allowed(Approval {
                access,
                scope_tags,
                default_tag: row.get(8)?,
                write_denied: row.get::<_, i64>(9)? != 0,
            })
        }
        _ => return Ok(None),
    };
    Ok(Some(StoredClient {
        identity,
        status,
        decided_at: row.get(10)?,
        last_seen_at: row.get(11)?,
    }))
}

/// The remembered decision for `client_id` on `device`.
pub fn load(q: &mut impl Query, device: Uuid, client_id: &str) -> Result<Option<StoredClient>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM haex_browser_clients WHERE vault_device_uuid = ?1 AND client_id = ?2"
    );
    Ok(
        q.query_row(&sql, params![device.to_string(), client_id], from_row)?
            .flatten(),
    )
}

/// Every remembered decision of `device`, newest first.
pub fn list(q: &mut impl Query, device: Uuid) -> Result<Vec<StoredClient>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM haex_browser_clients WHERE vault_device_uuid = ?1 \
         ORDER BY decided_at DESC"
    );
    Ok(q.query_map(&sql, params![device.to_string()], from_row)?
        .into_iter()
        .flatten()
        .collect())
}

/// Writes the decision for a client on `device`, replacing an earlier one. Check, then write, as
/// in `storage::preferences::insert_or_update`: an upsert would fire two CRDT triggers.
pub fn save(tx: &mut CrdtTransaction<'_>, device: Uuid, client: &StoredClient) -> Result<()> {
    let id = row_id(device, &client.identity.client_id).to_string();
    let (status, access, scope_tags, default_tag, write_denied) = match &client.status {
        Status::Blocked => ("blocked", None, None, None, 0i64),
        Status::Allowed(approval) => (
            "allowed",
            Some(approval.access.as_str()),
            approval
                .scope_tags
                .as_ref()
                .map(|tags| serde_json::Value::from(tags.clone()).to_string()),
            approval.default_tag.clone(),
            i64::from(approval.write_denied),
        ),
    };
    let identity = &client.identity;
    let exists = tx
        .query_row(
            "SELECT COUNT(*) FROM haex_browser_clients WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if exists {
        tx.execute(
            "UPDATE haex_browser_clients SET public_key = ?2, client_name = ?3, origin = ?4, \
             declaration = ?5, status = ?6, access = ?7, scope_tags = ?8, default_tag = ?9, \
             write_denied = ?10, decided_at = ?11, last_seen_at = ?12 WHERE id = ?1",
            params![
                id,
                identity.public_key,
                identity.client_name,
                identity.origin,
                identity.declaration,
                status,
                access,
                scope_tags,
                default_tag,
                write_denied,
                client.decided_at,
                client.last_seen_at,
            ],
        )?;
    } else {
        tx.execute(
            "INSERT INTO haex_browser_clients (id, vault_device_uuid, client_id, public_key, \
             client_name, origin, declaration, status, access, scope_tags, default_tag, \
             write_denied, decided_at, last_seen_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id,
                device.to_string(),
                identity.client_id,
                identity.public_key,
                identity.client_name,
                identity.origin,
                identity.declaration,
                status,
                access,
                scope_tags,
                default_tag,
                write_denied,
                client.decided_at,
                client.last_seen_at,
            ],
        )?;
    }
    Ok(())
}

/// Forgets the decision for a client on `device`; `false` when there was none.
pub fn delete(tx: &mut CrdtTransaction<'_>, device: Uuid, client_id: &str) -> Result<bool> {
    let id = row_id(device, client_id).to_string();
    Ok(tx.execute(
        "DELETE FROM haex_browser_clients WHERE id = ?1",
        params![id],
    )? > 0)
}

/// Notes a connection of an approved client.
// ponytail: written on every approved handshake, so it syncs to devices that never read it
// (research R13). Ceiling: one small row write per browser start. Upgrade path: a device-local
// table or a write at most once an hour.
pub fn touch(tx: &mut CrdtTransaction<'_>, device: Uuid, client_id: &str, now: i64) -> Result<()> {
    let id = row_id(device, client_id).to_string();
    tx.execute(
        "UPDATE haex_browser_clients SET last_seen_at = ?2 WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

/// A decision until the bridge stops or the vault closes (FR-009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionApproval {
    pub identity: ClientIdentity,
    pub approval: Approval,
    pub decided_at: i64,
}

/// The decisions without "remember".
#[derive(Debug, Default)]
pub struct SessionDecisions {
    allowed: HashMap<String, SessionApproval>,
    blocked: HashMap<String, (ClientIdentity, i64)>,
    /// Writing allowed once without "remember", with the default tag for new entries (FR-015).
    write_allowed: HashMap<String, Option<String>>,
    /// Writing declined without "remember" (FR-015).
    write_denied: HashSet<String>,
}

impl SessionDecisions {
    pub fn allow(&mut self, entry: SessionApproval) {
        self.blocked.remove(&entry.identity.client_id);
        self.allowed.insert(entry.identity.client_id.clone(), entry);
    }

    pub fn block(&mut self, identity: ClientIdentity, now: i64) {
        self.allowed.remove(&identity.client_id);
        self.blocked
            .insert(identity.client_id.clone(), (identity, now));
    }

    /// Forgets everything about one client; `true` when there was something.
    pub fn forget(&mut self, client_id: &str) -> bool {
        self.write_allowed.remove(client_id);
        self.write_denied.remove(client_id);
        let allowed = self.allowed.remove(client_id).is_some();
        let blocked = self.blocked.remove(client_id).is_some();
        allowed || blocked
    }

    /// Lets a read-only client write until the bridge stops (FR-015).
    pub fn allow_write(&mut self, client_id: &str, default_tag: Option<String>) {
        self.write_denied.remove(client_id);
        self.write_allowed.insert(client_id.to_owned(), default_tag);
    }

    /// Declines writing for a client until the bridge stops (FR-015).
    pub fn deny_write(&mut self, client_id: &str) {
        self.write_allowed.remove(client_id);
        self.write_denied.insert(client_id.to_owned());
    }

    /// The approval with this session's answers about writing applied.
    pub fn effective(&self, client_id: &str, approval: Approval) -> Approval {
        match self.write_allowed.get(client_id) {
            Some(default_tag) if approval.access == BridgeAccess::Read => Approval {
                access: BridgeAccess::ReadWrite,
                default_tag: default_tag.clone().or(approval.default_tag),
                ..approval
            },
            _ => Approval {
                write_denied: approval.write_denied || self.write_denied.contains(client_id),
                ..approval
            },
        }
    }

    pub fn allowed(&self, client_id: &str) -> Option<&SessionApproval> {
        self.allowed.get(client_id)
    }

    pub fn allowed_mut(&mut self, client_id: &str) -> Option<&mut SessionApproval> {
        self.allowed.get_mut(client_id)
    }

    pub fn is_blocked(&self, client_id: &str) -> bool {
        self.blocked.contains_key(client_id)
    }

    pub fn all_allowed(&self) -> impl Iterator<Item = &SessionApproval> {
        self.allowed.values()
    }

    pub fn all_blocked(&self) -> impl Iterator<Item = &(ClientIdentity, i64)> {
        self.blocked.values()
    }
}

/// What a handshake gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Authorized {
        approval: Approval,
        remembered: bool,
    },
    Blocked,
    /// Unknown, or known with another key, origin or declaration: holzi asks (FR-008, FR-012).
    Ask,
}

/// Decides a handshake. A block wins over every approval; an approval holds only for the same
/// identity.
pub fn decide(
    identity: &ClientIdentity,
    stored: Option<&StoredClient>,
    session: &SessionDecisions,
) -> Decision {
    if session.is_blocked(&identity.client_id)
        || matches!(
            stored,
            Some(StoredClient {
                status: Status::Blocked,
                ..
            })
        )
    {
        return Decision::Blocked;
    }
    if let Some(entry) = session.allowed(&identity.client_id) {
        if entry.identity.matches(identity) {
            return Decision::Authorized {
                approval: entry.approval.clone(),
                remembered: false,
            };
        }
    }
    if let Some(StoredClient {
        identity: known,
        status: Status::Allowed(approval),
        ..
    }) = stored
    {
        if known.matches(identity) {
            return Decision::Authorized {
                approval: approval.clone(),
                remembered: true,
            };
        }
    }
    Decision::Ask
}
