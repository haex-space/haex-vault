//! Pass methods of the service (spec 092, `contracts/tauri-commands.md`): create and update a pass,
//! read its codes, draw one. All for the user's own window (rule Z11, FR-027): no caller from
//! outside writes a pass or reads a code. Each write is one transaction that ends with a state of
//! the history, and no value of a code reaches a log line or an error.

use super::{require_user, PasswordsService};
use crate::error::{HolziError, Result};
use crate::passwords::access::Caller;
use crate::passwords::model::{ItemDetail, Patch};
use crate::passwords::pass::codes::{draw, CodeFormat};
use crate::passwords::pass::model::{CodeSvg, CodeView, PassInput, PassPatch};
use crate::passwords::pass::store;
use crate::passwords::references_db::{self, Reader};

impl PasswordsService {
    /// Creates a pass with its detail row, codes, tags, free fields, folder and first state, and
    /// returns it as the window shows it.
    pub async fn pass_create(
        &self,
        caller: &Caller,
        input: PassInput,
        group_id: Option<String>,
    ) -> Result<ItemDetail> {
        require_user(caller)?;
        let id = self
            .db()
            .write(move |tx| {
                let reader = Reader {
                    caller: &Caller::User,
                    grants: &[],
                };
                let texts = [&input.url, &input.note]
                    .into_iter()
                    .filter_map(|text| text.as_deref())
                    .chain(input.key_values.iter().filter_map(|kv| kv.value.as_deref()));
                references_db::check_sources_visible(tx, reader, texts)?;
                store::create(tx, &input, group_id.as_deref()).map_err(Into::into)
            })
            .await?;
        self.get_item(caller, id).await
    }

    /// Applies a partial update if the token `expected_updated_at` is still current. A pass that
    /// is not in the vault any more is `PasswordsConflict { deleted }`, one changed since the
    /// window read it `{ changed }`.
    pub async fn pass_update(
        &self,
        caller: &Caller,
        item_id: String,
        expected_updated_at: String,
        patch: PassPatch,
    ) -> Result<ItemDetail> {
        require_user(caller)?;
        let id = item_id.clone();
        self.db()
            .write(move |tx| {
                let item_patch = store::item_patch(&patch);
                let reader = Reader {
                    caller: &Caller::User,
                    grants: &[],
                };
                let set = [&item_patch.url, &item_patch.note]
                    .into_iter()
                    .filter_map(|value| match value {
                        Patch::Set(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .chain(
                        item_patch
                            .key_values
                            .iter()
                            .flatten()
                            .filter_map(|kv| kv.value.as_deref()),
                    );
                references_db::check_sources_visible(tx, reader, set)?;
                if let Some(texts) = references_db::texts_after_patch(tx, &item_id, &item_patch)? {
                    references_db::validate(tx, &item_id, &texts)?;
                }
                store::update(tx, &item_id, &expected_updated_at, &patch).map_err(Into::into)
            })
            .await?;
        self.get_item(caller, id).await
    }

    /// The codes of a pass with their values, one pass per call (the only bulk read of values).
    pub async fn pass_codes(&self, caller: &Caller, item_id: String) -> Result<Vec<CodeView>> {
        require_user(caller)?;
        self.db()
            .read(move |q| {
                require_pass(q, &item_id)?;
                store::codes_of(q, &item_id).map_err(Into::into)
            })
            .await
    }

    /// The drawing of one code, dark on white with its quiet zone. A code that arrived flagged,
    /// or that cannot be drawn, comes back as its text (FR-003).
    pub async fn pass_code_svg(
        &self,
        caller: &Caller,
        item_id: String,
        code_id: String,
    ) -> Result<CodeSvg> {
        require_user(caller)?;
        self.db()
            .read(move |q| {
                require_pass(q, &item_id)?;
                let code =
                    store::code_of(q, &item_id, &code_id)?.ok_or(HolziError::PasswordsNotFound)?;
                let drawing = match (&code.flag, CodeFormat::parse(&code.format)) {
                    (None, Some(format)) => draw(format, &code.value).ok(),
                    _ => None,
                };
                Ok(match drawing {
                    Some(d) => CodeSvg {
                        svg: Some(d.svg),
                        width: d.width,
                        height: d.height,
                        quiet_zone: d.quiet_zone,
                        text: None,
                    },
                    None => CodeSvg {
                        svg: None,
                        width: 0,
                        height: 0,
                        quiet_zone: 0,
                        text: Some(code.value),
                    },
                })
            })
            .await
    }
}

/// The entry must exist and be a pass: a missing one is `PasswordsNotFound`, a login or an unknown
/// type is refused.
fn require_pass(q: &mut impl crate::storage::query::Query, item_id: &str) -> Result<()> {
    match store::entry_type(q, item_id)? {
        None => Err(HolziError::PasswordsNotFound),
        Some(kind) if kind.is_pass() => Ok(()),
        Some(kind) => Err(store::not_a_pass(&kind)),
    }
}
