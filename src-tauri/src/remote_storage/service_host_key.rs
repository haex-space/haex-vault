//! Taking a new host key of an SFTP server (spec 050 research R5, contracts/tauri-commands.md
//! `storage_host_key_accept`): only exactly the key this process saw the server show last, never
//! one the window names on its own. The confirmed key syncs to the other devices of the vault.

use super::{invalid, StorageService};
use crate::error::Result;
use crate::passwords::clock;
use crate::remote_storage::model::ConnectionView;
use crate::remote_storage::sftp::keys;
use crate::remote_storage::{store, Location, Protocol};

impl StorageService {
    /// Confirms `host_key` (an OpenSSH line) for the SFTP connection `connection_id` when the
    /// server showed exactly that key at its last refused connection on this device.
    pub async fn accept_host_key(
        &self,
        connection_id: &str,
        host_key: &str,
    ) -> Result<ConnectionView> {
        let mut row = self.connection(connection_id).await?;
        if row.protocol != Protocol::Sftp {
            return Err(invalid("hostKey"));
        }
        let given = keys::parse_host_key(host_key)
            .map(|key| keys::host_key_line(&key))
            .ok_or_else(|| invalid("hostKey"))?;
        let shown = self
            .store
            .shown_host_key(&Location::of(&row, ""))
            .ok_or_else(|| invalid("hostKey"))?;
        if shown != given {
            return Err(invalid("hostKey"));
        }
        let now = clock::now();
        row.host_key = Some(given);
        row.host_key_confirmed_at = Some(now.clone());
        row.updated_at = now;
        let written = row.clone();
        self.db
            .write(move |tx| Ok(store::put_connection(tx, &written)?))
            .await?;
        self.view_of(&row).await
    }
}
