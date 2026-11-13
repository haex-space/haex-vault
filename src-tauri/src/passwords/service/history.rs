//! History methods of the service (spec 034, US4, FR-017): the states of an entry, one state
//! without its secrets, a secret of a state on request, and restoring a state. For the user alone
//! (rule Z11): a caller from outside never reads old passwords.

use super::{require_user, PasswordsService};
use crate::error::Result;
use crate::passwords::access::Caller;
use crate::passwords::model::{
    HistorySecret, RestoreOutcome, RevealedSecret, SnapshotHeader, SnapshotView,
};
use crate::passwords::references::Field;
use crate::passwords::references_db::{resolve_or_error, stored_texts, validate, Reader};
use crate::passwords::snapshots;
use crate::storage::query::Query as _;

impl PasswordsService {
    pub async fn history_list(
        &self,
        caller: &Caller,
        item_id: String,
    ) -> Result<Vec<SnapshotHeader>> {
        require_user(caller)?;
        self.db()
            .read(move |q| snapshots::list(q, &item_id).map_err(Into::into))
            .await
    }

    pub async fn history_get(&self, caller: &Caller, snapshot_id: String) -> Result<SnapshotView> {
        require_user(caller)?;
        self.db()
            .read(move |q| snapshots::get(q, &snapshot_id).map_err(Into::into))
            .await
    }

    pub async fn history_reveal(
        &self,
        caller: &Caller,
        snapshot_id: String,
        field: HistorySecret,
    ) -> Result<RevealedSecret> {
        require_user(caller)?;
        self.db()
            .read(move |q| {
                let mut secret = snapshots::reveal(q, &snapshot_id, &field)?;
                // Spec 036, FR-049: a state keeps its placeholders; revealing resolves them as
                // they are now, and a source that is gone is an error, never the placeholder.
                let origin = match &field {
                    HistorySecret::Password => Field::Password,
                    HistorySecret::KeyValue { key } => Field::Extra(key.clone()),
                    // A code is a plain value; it has no placeholders (spec 092).
                    HistorySecret::OtpSecret | HistorySecret::Code { .. } => return Ok(secret),
                };
                let item_id = q
                    .query_row(
                        "SELECT item_id FROM haex_passwords_item_snapshots WHERE id = ?1",
                        haex_crdt::rusqlite::params![snapshot_id],
                        |r| r.get::<_, String>(0),
                    )?
                    .unwrap_or_default();
                secret.value =
                    resolve_or_error(q, Reader::user(), &item_id, origin, &secret.value)?;
                Ok(secret)
            })
            .await
    }

    /// Makes a state the current one (the token `expected_updated_at` must still be current).
    pub async fn history_restore(
        &self,
        caller: &Caller,
        item_id: String,
        snapshot_id: String,
        expected_updated_at: String,
    ) -> Result<RestoreOutcome> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                let outcome = snapshots::restore(tx, &item_id, &snapshot_id, &expected_updated_at)?;
                // Spec 036, FR-046: a state from before another entry pointed back can close a
                // cycle; the error rolls the restore back.
                if let Some(texts) = stored_texts(tx, &item_id)? {
                    validate(tx, &item_id, &texts)?;
                }
                Ok(outcome)
            })
            .await
    }
}
