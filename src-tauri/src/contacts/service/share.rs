//! Sharing a contact in the service (spec 060 FR-008 to FR-025a): the rows of the share dialog, the
//! text a choice makes, and reading a share in after a preview. Only the user does any of it; the
//! choice of the dialog is never kept, and what a preview showed is never trusted on accepting.

use super::{caller_text, Caller, ContactsService};
use crate::contacts::access::require_user;
use crate::contacts::model::{Contact, Origin, OriginKind};
use crate::contacts::share::model::{
    ContactShareDecision, ContactShareOptions, ContactShareOutput, ContactSharePreview,
    ContactShareSelection, ContactShareTarget,
};
use crate::contacts::share::{build, merge, preview};
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

    /// What reading a share in would do; nothing is stored (FR-021). Also reads an old
    /// `holzi:card:` text. `target` is the person's choice; without it a vault that is on a contact
    /// already selects that contact.
    pub async fn share_preview(
        &self,
        caller: &Caller,
        text: String,
        target: Option<ContactShareTarget>,
    ) -> Result<ContactSharePreview> {
        require_user(caller)?;
        self.db()
            .read(move |q| preview::preview(q, &text, target.as_ref()).map_err(Into::into))
            .await
    }

    /// Reads a share in as a new contact or into an existing one, adding and replacing only what
    /// the decisions say. Everything is recomputed from the text; an existing target must still be
    /// what the preview saw (`preview_revision`), else [`crate::error::HolziError::SharePreviewStale`].
    pub async fn share_accept(
        &self,
        caller: &Caller,
        text: String,
        target: ContactShareTarget,
        decisions: Vec<ContactShareDecision>,
        preview_revision: Option<String>,
    ) -> Result<Contact> {
        require_user(caller)?;
        let origin = Origin {
            kind: OriginKind::Card,
            device: self.device(),
            caller: caller_text(caller),
            account: None,
        };
        self.db()
            .write(move |tx| {
                merge::apply(
                    tx,
                    &text,
                    Some(&target),
                    &decisions,
                    preview_revision.as_deref(),
                    &origin,
                )
                .map_err(Into::into)
            })
            .await
    }
}

#[cfg(test)]
impl ContactsService {
    /// Reads a text in with every default choice, as a new contact or into `contact`: what one
    /// click on "Übernehmen" does.
    pub(crate) async fn accept_defaults(
        &self,
        text: String,
        contact: Option<String>,
    ) -> Result<Contact> {
        let target = match contact {
            Some(id) => ContactShareTarget::Contact { id },
            None => ContactShareTarget::New,
        };
        let seen = self
            .share_preview(&Caller::User, text.clone(), Some(target.clone()))
            .await?;
        self.share_accept(
            &Caller::User,
            text,
            target,
            Vec::new(),
            seen.target_revision,
        )
        .await
    }
}

#[cfg(test)]
#[path = "share_tests.rs"]
mod tests;
