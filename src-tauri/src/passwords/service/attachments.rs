//! Attachment methods of the service (spec 034, US5, FR-019..FR-021): add from a file, rename,
//! remove, save to a file, preview an image, read a text or a PDF for the attachment view, and copy
//! an attachment for the system's app (spec 058). For the user alone (rule Z11). The file travels as
//! the choice of the system's dialog (spec 043, a path or a provider address) and is read and
//! written on a blocking thread, so a 25 MiB attachment never crosses the webview.

use std::path::PathBuf;

use uuid::Uuid;

use super::{require_user, PasswordsService};
use crate::error::{HolziError, Result};
use crate::files::local::text::TextContent;
use crate::files::picked::{Opener, PickedFile};
use crate::passwords::access::Caller;
use crate::passwords::model::AttachmentView;
use crate::passwords::{attachment_view, binaries};
use crate::storage::preferences::{self, PrefScope};

fn io_failed(error: tauri::Error) -> HolziError {
    HolziError::Io {
        reason: format!("attachment task failed: {error}"),
    }
}

impl PasswordsService {
    /// Attaches the chosen file to an entry. The size is checked before the file is read where
    /// it is known.
    pub async fn attachment_add(
        &self,
        caller: &Caller,
        item_id: String,
        opener: impl Opener + Send + 'static,
        file: PickedFile,
    ) -> Result<AttachmentView> {
        require_user(caller)?;
        let (name, bytes) = tauri::async_runtime::spawn_blocking(move || {
            binaries::read_attachment_file(&opener, &file)
        })
        .await
        .map_err(io_failed)??;
        self.db()
            .write(move |tx| binaries::add_bytes(tx, &item_id, &name, &bytes).map_err(Into::into))
            .await
    }

    /// Renames an attachment on its entry only; returns the stored name.
    pub async fn attachment_rename(
        &self,
        caller: &Caller,
        attachment_id: String,
        file_name: String,
    ) -> Result<String> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                binaries::rename_attachment(tx, &attachment_id, &file_name).map_err(Into::into)
            })
            .await
    }

    /// Removes the link of an attachment; the data stays until the clean-up.
    pub async fn attachment_remove(&self, caller: &Caller, attachment_id: String) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| binaries::remove_link(tx, &attachment_id).map_err(Into::into))
            .await
    }

    /// Writes an attachment byte for byte to the file the user chose.
    pub async fn attachment_save(
        &self,
        caller: &Caller,
        attachment_id: String,
        opener: impl Opener + Send + 'static,
        file: PickedFile,
    ) -> Result<()> {
        require_user(caller)?;
        let (_, bytes) = self
            .db()
            .read(move |q| binaries::attachment_data(q, &attachment_id).map_err(Into::into))
            .await?;
        tauri::async_runtime::spawn_blocking(move || binaries::save_to(&opener, &file, &bytes))
            .await
            .map_err(io_failed)?
    }

    /// The text of a text attachment for the attachment view (spec 058 FR-003), at most 5 MB.
    pub async fn attachment_text(
        &self,
        caller: &Caller,
        attachment_id: String,
    ) -> Result<TextContent> {
        require_user(caller)?;
        let (name, bytes) = self
            .db()
            .read(move |q| binaries::attachment_data(q, &attachment_id).map_err(Into::into))
            .await?;
        tauri::async_runtime::spawn_blocking(move || attachment_view::text_of(&name, bytes))
            .await
            .map_err(io_failed)?
    }

    /// The bytes of a PDF attachment for the attachment view (spec 058 FR-004), served by the
    /// media server.
    pub async fn attachment_pdf(&self, caller: &Caller, attachment_id: String) -> Result<Vec<u8>> {
        require_user(caller)?;
        let (name, bytes) = self
            .db()
            .read(move |q| binaries::attachment_data(q, &attachment_id).map_err(Into::into))
            .await?;
        attachment_view::pdf_bytes(&name, bytes)
    }

    /// A decrypted copy of an attachment below `root` for the system's app (spec 058 FR-010,
    /// FR-011). Without `confirmed`, and unless "Nicht mehr fragen" was chosen on `device` before,
    /// the answer is `None` and nothing is read; `confirmed` with `dont_ask_again` remembers that
    /// choice for `device` alone.
    pub async fn attachment_copy_for_app(
        &self,
        caller: &Caller,
        attachment_id: String,
        root: PathBuf,
        device: Uuid,
        confirmed: bool,
        dont_ask_again: bool,
    ) -> Result<Option<PathBuf>> {
        require_user(caller)?;
        let scope = PrefScope::Device(device);
        if !confirmed {
            let ack = self
                .db()
                .read(move |q| preferences::get(q, scope, attachment_view::OPEN_ACK_KEY))
                .await?;
            if ack.as_deref() != Some("1") {
                return Ok(None);
            }
        } else if dont_ask_again {
            self.db()
                .write(move |tx| {
                    preferences::insert_or_update(tx, scope, attachment_view::OPEN_ACK_KEY, "1")
                })
                .await?;
        }
        let (name, bytes) = self
            .db()
            .read(move |q| binaries::attachment_data(q, &attachment_id).map_err(Into::into))
            .await?;
        attachment_view::write_copy(&root, &name, &bytes)
            .await
            .map(Some)
    }

    /// The bytes of an image attachment for the preview.
    pub async fn attachment_preview(
        &self,
        caller: &Caller,
        attachment_id: String,
    ) -> Result<Vec<u8>> {
        require_user(caller)?;
        self.db()
            .read(move |q| {
                binaries::preview_bytes(q, &attachment_id)
                    .map(|(bytes, _)| bytes)
                    .map_err(Into::into)
            })
            .await
    }
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
