//! Importing a pass from a file (spec 092, FR-028 to FR-033): the preview reads the file into
//! memory, finds a duplicate and proposes a place; the save writes the pass in one transaction with
//! its folder, tag, logo and codes, as a pass created in the window is written. For the user's own
//! window only (rule Z11); no code value reaches a log line, an error or the report.

use super::{require_user, PasswordsService};
use crate::error::{HolziError, Result};
use crate::passwords::access::Caller;
use crate::passwords::binaries;
use crate::passwords::items::trashed_item_ids;
use crate::passwords::pass::import_model::{
    CodePreview, DuplicateOf, FieldPreview, PassImportChoices, PassImportPreview, PassImportReport,
    PassImportSaved, PassOnDuplicate, PassPreview,
};
use crate::passwords::pass::model::{PassInput, PassKind};
use crate::passwords::pass::pkpass::ParsedPass;
use crate::passwords::pass::place::{self, proposal};
use crate::passwords::pass::previews::PassImportPreviews;
use crate::passwords::pass::store;
use haex_crdt::rusqlite::params;

impl PasswordsService {
    /// The proposal of FR-019 for a pass of this kind: reads the folders of Wallet, creates nothing.
    pub async fn pass_place_proposal(
        &self,
        caller: &Caller,
        kind: PassKind,
        has_date: bool,
        provider: Option<String>,
    ) -> Result<place::Proposal> {
        require_user(caller)?;
        self.db()
            .read(move |q| Ok(proposal(q, &kind, has_date, provider.as_deref())?))
            .await
    }

    /// Keeps a pass read from a file for the dialog and returns what the dialog shows. Nothing is
    /// written to the vault.
    pub async fn pass_import_preview(
        &self,
        caller: &Caller,
        previews: &PassImportPreviews,
        parsed: ParsedPass,
    ) -> Result<PassImportPreview> {
        require_user(caller)?;
        let source_id = parsed.source_id.clone();
        let kind = parsed.pass_kind.clone();
        let has_date = parsed.start.is_some();
        let provider = parsed.provider.clone();
        let (proposal, duplicate_of) = self
            .db()
            .read(move |q| {
                let proposal = proposal(q, &kind, has_date, provider.as_deref())?;
                let duplicate = match &source_id {
                    Some(source) => duplicate_of(q, source)?,
                    None => None,
                };
                Ok((proposal, duplicate))
            })
            .await?;
        let pass = PassPreview {
            pass_kind: parsed.pass_kind.clone(),
            title: parsed.title.clone(),
            provider: parsed.provider.clone(),
            start: parsed.start.clone(),
            expires_at: parsed.expires_at.clone(),
            codes: parsed
                .codes
                .iter()
                .map(|c| CodePreview {
                    format: c.format.clone(),
                    label: c.label.clone(),
                    flag: c.flag.clone(),
                })
                .collect(),
            fields: parsed
                .key_values
                .iter()
                .map(|kv| FieldPreview {
                    name: kv.key.clone(),
                    value: kv.value.clone().unwrap_or_default(),
                })
                .collect(),
            has_icon: parsed.icon.is_some(),
        };
        let not_kept = parsed.not_kept.clone();
        let preview_id = previews.put(parsed);
        Ok(PassImportPreview {
            preview_id,
            pass,
            proposal,
            duplicate_of,
            not_kept,
        })
    }

    /// Writes the pass behind a preview with the user's choices, in one transaction, and drops the
    /// preview. A pass the vault holds already is asked about unless the choice says what to do.
    pub async fn pass_import_save(
        &self,
        caller: &Caller,
        previews: &PassImportPreviews,
        preview_id: String,
        choices: PassImportChoices,
    ) -> Result<PassImportSaved> {
        require_user(caller)?;
        let parsed = previews
            .get(&preview_id)
            .ok_or(HolziError::PasswordsNotFound)?;
        let kind = choices
            .pass_kind
            .clone()
            .unwrap_or_else(|| parsed.pass_kind.clone());
        let title = choices
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map_or_else(|| parsed.title.clone(), str::to_owned);
        let on_duplicate = choices.on_duplicate;
        let report = PassImportReport {
            title: title.clone(),
            folder_path: choices.folder_path.clone(),
            tag_name: choices.tag_name.clone().filter(|t| !t.trim().is_empty()),
            not_kept: parsed.not_kept.clone(),
            flagged_codes: parsed.codes.iter().filter(|c| c.flag.is_some()).count() as u32,
        };
        let saved = self
            .db()
            .write(move |tx| {
                if let Some(source) = &parsed.source_id {
                    if let Some(found) = duplicate_of(tx, source)? {
                        match on_duplicate {
                            PassOnDuplicate::Ask => {
                                return Err(HolziError::PassRefused {
                                    code: "duplicate".to_owned(),
                                    position: None,
                                    reason: Some(found.id),
                                }
                                .into())
                            }
                            PassOnDuplicate::Skip => return Ok(None),
                            PassOnDuplicate::Add => {}
                        }
                    }
                }
                let icon = match &parsed.icon {
                    Some(bytes) => {
                        let hash = binaries::hash_bytes(bytes);
                        binaries::ensure_binary(tx, &hash, bytes, "icon")?;
                        Some(format!("binary:{hash}"))
                    }
                    None => None,
                };
                let group_id = place::resolve_path(tx, &choices.folder_path)?;
                let input = PassInput {
                    title: Some(title),
                    note: None,
                    url: None,
                    icon,
                    color: None,
                    expires_at: parsed.expires_at.clone(),
                    pass_kind: kind,
                    start: choices.start.clone().or_else(|| parsed.start.clone()),
                    parent_id: choices.parent_id.clone().filter(|p| !p.is_empty()),
                    codes: parsed.codes.clone(),
                    key_values: parsed.key_values.clone(),
                    tag_names: choices
                        .tag_name
                        .iter()
                        .map(|t| t.trim().to_owned())
                        .filter(|t| !t.is_empty())
                        .collect(),
                    source_id: parsed.source_id.clone(),
                };
                store::create(tx, &input, group_id.as_deref())
                    .map(Some)
                    .map_err(Into::into)
            })
            .await?;
        previews.remove(&preview_id);
        Ok(PassImportSaved { id: saved, report })
    }
}

/// A pass of the vault, not in the trash, that comes from the same file.
fn duplicate_of(
    q: &mut impl crate::storage::query::Query,
    source_id: &str,
) -> Result<Option<DuplicateOf>> {
    let candidates = q.query_map(
        "SELECT d.item_id, i.title FROM haex_passwords_pass_details d \
         JOIN haex_passwords_item_details i ON i.id = d.item_id WHERE d.source_id = ?1",
        params![source_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
    )?;
    if candidates.is_empty() {
        return Ok(None);
    }
    let trashed = trashed_item_ids(q)?;
    Ok(candidates
        .into_iter()
        .find(|(id, _)| !trashed.contains(id))
        .map(|(id, title)| DuplicateOf { id, title }))
}
