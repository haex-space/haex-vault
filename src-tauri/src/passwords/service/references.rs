//! Reference methods of the service (spec 036, `contracts/tauri-commands.md`, research R11, R12):
//! the window asks the backend for the marks of a text, the placeholder for a value, the keys of
//! an entry's custom fields and how many entries use a source. All for the user alone (Z11); none
//! returns a value.

use haex_crdt::rusqlite::params;

use super::{require_user, PasswordsService};
use crate::error::{HaexError, Result};
use crate::passwords::access::Caller;
use crate::passwords::model_references::{RefMark, RefMarkKind, ReferenceUsage};
use crate::passwords::references::{build_token, RefKind};
use crate::passwords::references_db;
use crate::storage::query::Query as _;

impl PasswordsService {
    /// The placeholders of a text with their sources and whether they resolve.
    pub async fn references_parse(&self, caller: &Caller, text: String) -> Result<Vec<RefMark>> {
        require_user(caller)?;
        self.db()
            .read(move |q| references_db::marks(q, &text).map_err(Into::into))
            .await
    }

    /// The placeholder for a value of an existing entry, with its escaping; the window never builds
    /// one itself.
    pub async fn reference_token(
        &self,
        caller: &Caller,
        item_id: String,
        kind: RefMarkKind,
        key: Option<String>,
    ) -> Result<String> {
        require_user(caller)?;
        let kind = match kind {
            RefMarkKind::Username => RefKind::Username,
            RefMarkKind::Password => RefKind::Password,
            RefMarkKind::Extra => {
                RefKind::Extra(key.filter(|k| !k.is_empty()).ok_or_else(|| {
                    HaexError::InvalidInput {
                        reason: "key".to_string(),
                    }
                })?)
            }
        };
        self.db()
            .read(move |q| {
                let exists = q
                    .query_row(
                        "SELECT COUNT(*) FROM haex_passwords_item_details WHERE id = ?1",
                        params![item_id],
                        |r| r.get::<_, i64>(0),
                    )?
                    .unwrap_or(0)
                    > 0;
                if !exists {
                    return Err(HaexError::PasswordsNotFound.into());
                }
                build_token(&item_id, &kind)
                    .ok_or(HaexError::PasswordsNotFound)
                    .map_err(Into::into)
            })
            .await
    }

    /// The keys of an entry's custom fields (never their values), first occurrence first.
    pub async fn item_key_names(&self, caller: &Caller, item_id: String) -> Result<Vec<String>> {
        require_user(caller)?;
        self.db()
            .read(move |q| {
                let keys = q.query_map(
                    "SELECT key FROM haex_passwords_item_key_values WHERE item_id = ?1 \
                     ORDER BY rowid",
                    params![item_id],
                    |r| r.get::<_, Option<String>>(0),
                )?;
                let mut unique: Vec<String> = Vec::new();
                for key in keys.into_iter().flatten() {
                    if !key.trim().is_empty() && !unique.contains(&key) {
                        unique.push(key);
                    }
                }
                Ok(unique)
            })
            .await
    }

    /// How many other entries point at each entry, asked before deleting for good (FR-048).
    pub async fn reference_usage(
        &self,
        caller: &Caller,
        item_ids: Vec<String>,
    ) -> Result<Vec<ReferenceUsage>> {
        require_user(caller)?;
        self.db()
            .read(move |q| references_db::targets_of(q, &item_ids).map_err(Into::into))
            .await
    }
}
