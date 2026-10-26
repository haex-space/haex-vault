//! Sharing a contact in the service (spec 060 FR-008 to FR-019): the rows of the share dialog and
//! the text a choice makes. Only the user shares; nothing is stored and the choice is never kept.

use super::{Caller, ContactsService};
use crate::contacts::access::require_user;
use crate::contacts::share::build;
use crate::contacts::share::model::{
    ContactShareOptions, ContactShareOutput, ContactShareSelection,
};
use crate::error::Result;

impl ContactsService {
    /// What the share dialog shows for a contact, everything chosen.
    pub async fn share_options(
        &self,
        caller: &Caller,
        contact_id: String,
    ) -> Result<ContactShareOptions> {
        require_user(caller)?;
        self.db()
            .read(move |q| build::options(q, &contact_id).map_err(Into::into))
            .await
    }

    /// The text a choice of the dialog makes.
    pub async fn share_build(
        &self,
        caller: &Caller,
        contact_id: String,
        selection: ContactShareSelection,
    ) -> Result<ContactShareOutput> {
        require_user(caller)?;
        self.db()
            .read(move |q| build::build(q, &contact_id, &selection).map_err(Into::into))
            .await
    }
}
