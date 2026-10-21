//! Item methods of the service (spec 034, `contracts/access.md` §Dienst): the overview, the
//! entries, the single-entry secret read, reveal, TOTP, copy, create and update. The five item
//! methods are open to every caller that a grant covers (Z3–Z9, Z12, Z13); everything else here is
//! for the user alone (Z11), except the TOTP code, which a read grant covers since spec 049
//! (FR-024). A caller that is not the user gets only what its grants cover, and never a secret in
//! a list (Z4).

use zeroize::Zeroizing;

use super::{require_user, PasswordsService};
use crate::error::{HolziError, Result};
use crate::passwords::access::{
    authorize_create, authorize_list, authorize_read, authorize_update, Caller, Grant, ItemState,
    ListView,
};
use crate::passwords::ids::fold_for_search;
use crate::passwords::items;
use crate::passwords::model::{
    AgentHeader, CopyField, ItemDetail, ItemHeader, ItemInput, ItemPatch, Overview, Patch,
    RevealedSecret, SecretField, SecretItem, TextField, TotpCode,
};
use crate::passwords::references::{contains_reference, Field};
use crate::passwords::references_db::{self, Reader};
use crate::passwords::reveal::{self, Copied};

/// What `list_headers` delivers: the headers of the entries in the scope, or the narrow headers of
/// the built-in agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Headers {
    Items(Vec<ItemHeader>),
    Agent(Vec<AgentHeader>),
}

impl PasswordsService {
    /// The user's overview: every entry (also in the trash), every folder, every tag. Z11.
    pub async fn load_overview(&self, caller: &Caller) -> Result<Overview> {
        require_user(caller)?;
        self.db()
            .read(|q| items::load_overview(q).map_err(Into::into))
            .await
    }

    /// The headers a caller may see (Z2, Z4): the built-in agent gets the narrow headers, everybody
    /// else the entries in its scope, never a secret and never an entry in the trash (the window
    /// reads the trash through [`Self::load_overview`]).
    pub async fn list_headers(&self, caller: &Caller, grants: &[Grant]) -> Result<Headers> {
        let view = authorize_list(caller, grants)?;
        let caller = caller.clone();
        self.db()
            .read(move |q| {
                Ok(match view {
                    ListView::Agent => Headers::Agent(items::agent_headers(q)?),
                    ListView::Items(scope) => {
                        let mut headers = items::headers_in_scope(q, &scope, &caller)?;
                        // Spec 036, FR-047: a caller from outside never sees a placeholder in a
                        // list; the field is empty (the single-entry read resolves it).
                        if !matches!(caller, Caller::User) {
                            for header in &mut headers {
                                for field in [&mut header.username, &mut header.url] {
                                    if field.as_deref().is_some_and(contains_reference) {
                                        *field = None;
                                    }
                                }
                            }
                        }
                        Headers::Items(headers)
                    }
                })
            })
            .await
    }

    /// The search of the built-in agent (FR-027, action `passwords.items.search`): title, tag
    /// names and folder name of the entries outside the trash that match `query` (every word) and
    /// carry `tag`, at most `limit` (default 20, at most 50). Closed to every other caller.
    pub async fn agent_search(
        &self,
        caller: &Caller,
        query: Option<String>,
        tag: Option<String>,
        limit: Option<u32>,
    ) -> Result<Vec<AgentHeader>> {
        match authorize_list(caller, &[])? {
            ListView::Agent => {}
            ListView::Items(_) => return Err(HolziError::PasswordsForbidden),
        }
        let limit = limit.unwrap_or(20).clamp(1, 50) as usize;
        let words: Vec<String> = query
            .as_deref()
            .map(fold_for_search)
            .map(|q| q.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        let tag = tag
            .as_deref()
            .map(fold_for_search)
            .filter(|t| !t.is_empty());
        let all = self
            .db()
            .read(|q| items::agent_headers(q).map_err(Into::into))
            .await?;
        Ok(all
            .into_iter()
            .filter(|header| {
                tag.as_ref().is_none_or(|wanted| {
                    header
                        .tags
                        .iter()
                        .any(|name| &fold_for_search(name) == wanted)
                })
            })
            .filter(|header| {
                let texts: Vec<String> = header
                    .title
                    .iter()
                    .chain(header.tags.iter())
                    .chain(header.folder.iter())
                    .map(|text| fold_for_search(text))
                    .collect();
                words
                    .iter()
                    .all(|word| texts.iter().any(|text| text.contains(word)))
            })
            .take(limit)
            .collect())
    }

    /// The entry without secrets, for the window. Z11 (the other callers read through
    /// [`Self::read_secret_item`]).
    pub async fn get_item(&self, caller: &Caller, item_id: String) -> Result<ItemDetail> {
        require_user(caller)?;
        self.db()
            .read(move |q| {
                let mut detail =
                    items::get_item(q, &item_id)?.ok_or(HolziError::PasswordsNotFound)?;
                detail.references = references_db::item_references(q, &item_id)?;
                Ok(detail)
            })
            .await
    }

    /// The entry with its secrets on a single request (FR-026). An entry outside the scope or in
    /// the trash is `PasswordsNotFound`, the same as a missing one (Z5, Z13).
    pub async fn read_secret_item(
        &self,
        caller: &Caller,
        grants: &[Grant],
        item_id: String,
    ) -> Result<SecretItem> {
        let (caller, grants) = (caller.clone(), grants.to_vec());
        self.db()
            .read(move |q| {
                authorize_item_read(q, &caller, &grants, &item_id)?;
                let mut item =
                    reveal::secret_item(q, &item_id)?.ok_or(HolziError::PasswordsNotFound)?;
                reveal::resolve_secret_item(
                    q,
                    Reader {
                        caller: &caller,
                        grants: &grants,
                    },
                    &mut item,
                )?;
                Ok(item)
            })
            .await
    }

    /// The value of a secret for the moment the user asks to see it. Z11.
    pub async fn reveal(
        &self,
        caller: &Caller,
        item_id: String,
        field: SecretField,
    ) -> Result<RevealedSecret> {
        require_user(caller)?;
        self.db()
            .read(move |q| reveal::reveal(q, &item_id, &field).map_err(Into::into))
            .await
    }

    /// The current TOTP code of an entry and the seconds until it changes, never the secret. The
    /// user sees every entry; another caller needs a read grant for it (spec 049 FR-024, which
    /// changes Z11 for this method). An entry without TOTP is `PasswordsNotFound` for a caller from
    /// outside, the same as one outside the scope (FR-042).
    pub async fn totp_code(
        &self,
        caller: &Caller,
        grants: &[Grant],
        item_id: String,
    ) -> Result<TotpCode> {
        let (caller, grants) = (caller.clone(), grants.to_vec());
        self.db()
            .read(move |q| {
                authorize_item_read(q, &caller, &grants, &item_id)?;
                match reveal::totp_code(q, &item_id, reveal::unix_now()) {
                    Err(_) if !matches!(caller, Caller::User) => {
                        Err(HolziError::PasswordsNotFound.into())
                    }
                    result => result.map_err(Into::into),
                }
            })
            .await
    }

    /// The value for the clipboard; the command that called this writes it there. Z11.
    pub async fn copy_value(
        &self,
        caller: &Caller,
        item_id: String,
        field: CopyField,
    ) -> Result<Copied> {
        require_user(caller)?;
        self.db()
            .read(move |q| reveal::copy_value(q, &item_id, &field).map_err(Into::into))
            .await
    }

    /// A text the window holds (an editor value, a field of a history state) with its placeholders
    /// resolved as in `field` of the entry `item_id` (none for a new entry); a placeholder that
    /// does not resolve is the error, never the text (spec 036, FR-045).
    pub async fn resolve_text(
        &self,
        caller: &Caller,
        item_id: Option<String>,
        field: TextField,
        text: Zeroizing<String>,
    ) -> Result<Zeroizing<String>> {
        require_user(caller)?;
        if !contains_reference(&text) {
            return Ok(text);
        }
        let field = match field {
            TextField::Username => Field::Username,
            TextField::Password => Field::Password,
            TextField::Url => Field::Url,
            TextField::Note => Field::Note,
            TextField::KeyValue { key } => Field::Extra(key),
        };
        self.db()
            .read(move |q| {
                references_db::resolve_or_error(
                    q,
                    Reader::user(),
                    item_id.as_deref().unwrap_or_default(),
                    field,
                    &text,
                )
                .map_err(Into::into)
            })
            .await
    }

    /// How long a copy stays on the clipboard (the vault setting), `None` when it is off. Z11.
    pub async fn clipboard_delay(&self, caller: &Caller) -> Result<Option<std::time::Duration>> {
        require_user(caller)?;
        self.db()
            .read(|q| crate::passwords::settings::clear_after(q).map_err(Into::into))
            .await
    }

    /// Creates an entry. A caller with a tag scope must send only tags of its scope and at least
    /// one (Z6).
    pub async fn create_item(
        &self,
        caller: &Caller,
        grants: &[Grant],
        input: ItemInput,
        group_id: Option<String>,
    ) -> Result<String> {
        authorize_create(caller, grants, &input.tags)?;
        let (caller, grants) = (caller.clone(), grants.to_vec());
        self.db()
            .write(move |tx| {
                let reader = Reader {
                    caller: &caller,
                    grants: &grants,
                };
                let texts = [&input.username, &input.password, &input.url, &input.note]
                    .into_iter()
                    .filter_map(|text| text.as_deref())
                    .chain(input.key_values.iter().filter_map(|kv| kv.value.as_deref()));
                references_db::check_sources_visible(tx, reader, texts)?;
                items::create_item(tx, &input, group_id.as_deref()).map_err(Into::into)
            })
            .await
    }

    /// Applies a partial update if the token `expected_updated_at` is still current. For a caller
    /// with a tag scope the entry must lie in the scope before and after, tags outside the scope
    /// stay as they are and none can be added (Z7, Z12); a list of custom fields keeps those the
    /// caller cannot see (spec 036, FR-047).
    pub async fn update_item(
        &self,
        caller: &Caller,
        grants: &[Grant],
        item_id: String,
        expected_updated_at: String,
        mut patch: ItemPatch,
    ) -> Result<String> {
        let (caller, grants) = (caller.clone(), grants.to_vec());
        self.db()
            .write(move |tx| {
                let state = items::item_state(tx, &item_id)?;
                match (&state, &caller) {
                    (Some(state), _) => {
                        let result = authorize_update(
                            &caller,
                            &grants,
                            &state.view(),
                            patch.tags.as_deref(),
                        )
                        .map_err(HolziError::from)?;
                        if patch.tags.is_some() {
                            patch.tags = Some(result);
                        }
                    }
                    // A missing entry is a conflict for the user and unknown to everybody else.
                    (None, Caller::User) => {}
                    (None, _) => {
                        // Forbidden without a write grant, otherwise not found (Z3, Z5).
                        authorize_update(
                            &caller,
                            &grants,
                            &ItemState {
                                tags: &[],
                                in_trash: false,
                                owner: None,
                            },
                            None,
                        )
                        .map_err(HolziError::from)?;
                        return Err(HolziError::PasswordsNotFound.into());
                    }
                }
                // Spec 036, FR-047: only the values this patch sets; a placeholder the user stored
                // earlier does not block an outside caller's change of another field.
                let reader = Reader {
                    caller: &caller,
                    grants: &grants,
                };
                // Spec 036, FR-047: a caller from outside that replaces the custom fields keeps
                // those it cannot see, without learning that they exist.
                if !matches!(caller, Caller::User) {
                    if let Some(fields) = patch.key_values.as_mut() {
                        let hidden = reveal::hidden_key_values(tx, reader, &item_id)?;
                        fields.retain(|field| !hidden.iter().any(|kept| kept.id == field.id));
                        fields.extend(hidden);
                    }
                }
                let set = [&patch.username, &patch.password, &patch.url, &patch.note]
                    .into_iter()
                    .filter_map(|value| match value {
                        Patch::Set(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .chain(
                        patch
                            .key_values
                            .iter()
                            .flatten()
                            .filter_map(|kv| kv.value.as_deref()),
                    );
                references_db::check_sources_visible(tx, reader, set)?;
                // Spec 036, FR-046: no value may lead back to itself through references.
                if let Some(texts) = references_db::texts_after_patch(tx, &item_id, &patch)? {
                    references_db::validate(tx, &item_id, &texts)?;
                }
                items::update_item(tx, &item_id, &expected_updated_at, &patch).map_err(Into::into)
            })
            .await
    }
}

/// Checks that the caller may read one entry. A missing entry is forbidden without a read grant
/// and otherwise indistinguishable from one outside the scope (Z3, Z5).
fn authorize_item_read(
    q: &mut impl crate::storage::query::Query,
    caller: &Caller,
    grants: &[Grant],
    item_id: &str,
) -> haex_crdt::Result<()> {
    let Some(state) = items::item_state(q, item_id)? else {
        authorize_read(
            caller,
            grants,
            &ItemState {
                tags: &[],
                in_trash: false,
                owner: None,
            },
        )
        .map_err(HolziError::from)?;
        return Err(HolziError::PasswordsNotFound.into());
    };
    authorize_read(caller, grants, &state.view()).map_err(HolziError::from)?;
    Ok(())
}
