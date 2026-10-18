//! Import from haex-vault 2.x (Legacy) (spec 037): haex-vault 2.x has no export, so its vault file is read directly.
//! The password manager of holzi took its data model from haex-vault 2.x (spec 034), so the mapping is
//! nearly one to one (`specs/037-haex-vault-import/contracts/haex-vault-mapping.md`). Unlike the
//! other parsers this one needs the file on disk, not its bytes: it opens a copy with the vault
//! password ([`open`]), checks the layout and reads the rows ([`read`]).

mod groups;
mod history;
mod icons;
mod open;
mod read;

use std::path::Path;

use haex_crdt::rusqlite;

use super::{failed, Credentials, ImportModel};
use crate::error::{HolziError, Result};

/// The id of the trash folder in haex-vault 2.x (`stores/passwords/groups.ts:16`).
const TRASH_ID: &str = "trash";

/// What a preview says about the file besides the counts.
pub const WARNING_CLOSE_FIRST: &str = "legacy_haex_vault_close_first";
pub const WARNING_HAEX_PASS_TABLES: &str = "haex_pass_tables_ignored";

/// The model of a haex-vault 2.x file, and whether it holds tables of the old haex-pass extension
/// (they are not imported, the preview says so).
pub struct LegacyHaexVaultModel {
    pub model: ImportModel,
    pub has_haex_pass_tables: bool,
}

/// Reads the vault file at `path` with the vault password of `credentials`. Never changes `path` or
/// anything next to it; the copy it opens is gone when this returns.
pub fn read(path: &Path, credentials: &Credentials) -> Result<LegacyHaexVaultModel> {
    let password = credentials
        .password
        .as_deref()
        .ok_or_else(|| failed("legacy_haex_vault_locked"))?;
    let source = open::open(path, password)?;
    let layout = open::check_layout(&source.conn)?;
    let mut model = read::to_model(&source.conn)?;
    model.source_problems.extend(layout.problems);
    Ok(LegacyHaexVaultModel {
        model,
        has_haex_pass_tables: layout.has_haex_pass_tables,
    })
}

/// A query on the source that failed: the file is not what the layout check let through. The cause
/// goes to the log (it names no value), the caller gets the reason `corrupt`.
fn corrupt(error: rusqlite::Error) -> HolziError {
    log::warn!("passwords import: reading the haex-vault 2.x file failed: {error}");
    failed("corrupt")
}
