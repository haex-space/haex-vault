//! What the address book tidies once when a vault is opened: photos no contact uses any more
//! (FR-005). Runs as tracked session work; a failure is logged and never blocks opening.

use super::photos;
use crate::passwords::clock::unix_millis;
use crate::state::AppState;

/// Starts the tidy-up of the open vault in the background.
pub fn start_after_open(state: &AppState) {
    let Ok(db) = state.database() else {
        return;
    };
    let started = state.gate().spawn(async move {
        let now = unix_millis(std::time::SystemTime::now());
        match db
            .write(move |tx| photos::prune(tx, now).map_err(Into::into))
            .await
        {
            Ok(0) => {}
            Ok(deleted) => log::info!("contacts: removed {deleted} unused photos"),
            Err(error) => log::warn!("contacts: the clean-up of photos failed: {error}"),
        }
    });
    if let Err(error) = started {
        log::warn!("contacts: the clean-up could not start: {error}");
    }
}
