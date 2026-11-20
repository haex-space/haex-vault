//! Usage query of the service (spec 034, FR-034): which functions use an entry. For the window
//! (rule Z11); the answer holds names of functions, never a value.

use super::{require_user, PasswordsService};
use crate::error::Result;
use crate::passwords::access::Caller;

impl PasswordsService {
    /// The names of the haex-vault functions that use the entry.
    pub fn item_usage(&self, caller: &Caller, item_id: &str) -> Result<Vec<String>> {
        require_user(caller)?;
        Ok(self
            .usage()
            .used_by(item_id)
            .into_iter()
            .map(str::to_string)
            .collect())
    }
}
