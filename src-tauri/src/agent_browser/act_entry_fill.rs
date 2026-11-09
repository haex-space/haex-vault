//! `browser.entry.fill` (spec 065 US4, FR-037, FR-038, research R7a): holzi fills data of an entry
//! the user chooses (bank or card data, a username, a custom field) into a form of the agent's tab.
//! The agent names only the tab, perhaps the form, and its purpose; it learns only the numbers of
//! the elements holzi filled, never the entry, its field names or values.
//!
//! The entries are those the agent's rights group "Passwörter" reaches (the built-in agent's own
//! setting, read through the holzi function `agent_browser`), not those of the website: bank data
//! belongs to no website. Which page field gets which entry field decides `haex-autofill`, the
//! rule of the browser extension, card fields (`autocomplete` `cc-*`) included. holzi asks every
//! time, in Manuell and Auto, never remembered: the question shows the website, the purpose as the
//! agent's statement, the entries to choose from and one row per mapped field, each to deselect.
//! The values are written as in `browser.credentials.fill`: from holzi's world, only into the
//! fields the user saw, remembered by the session, removed from every later reply, and screenshots
//! stay off. Submitting the form asks again with the values as `•••`.
//!
//! Besides the tab's page holzi fills the frames of websites the agent may operate
//! ([`super::entry_docs`]); their rows name the frame's website. The agent's reply counts the
//! fields filled in frames, whose elements it cannot number.

use std::collections::BTreeMap;

use haex_autofill::{map_fields, EntryKeys, FieldFacts, Target as FieldTarget};
use serde_json::{json, Value};
use url::Url;

use zeroize::Zeroizing;

use super::credentials::{no_matching_entry, password_caller, FillFacts};
use super::entry_docs::Doc;
use super::{element_gone, Acting};
use crate::agent_browser::calls::{browser_error, site_of};
use crate::agent_browser::error::BrowserError;
use crate::agent_browser::prompts::{
    BrowserDecision, BrowserEntryChoice, BrowserMappingRow, BrowserPromptKind, BrowserQuestion,
};
use crate::agent_browser::tab::Tab;
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::passwords::access::Grant;
#[cfg(test)]
use crate::passwords::autofill::aliases_of;
use crate::passwords::autofill::{fields_of, OTP_MARKER};
use crate::passwords::model::SecretItem;
use crate::passwords::service::PasswordsService;

/// The longest purpose the agent may give.
pub const MAX_PURPOSE: usize = 200;

fn nothing_mapped() -> NativeError {
    NativeError::new(
        "nothing_mapped",
        "No field of this form matches a field of an entry this agent may use.",
    )
}

/// An entry the agent's password group reaches: its id and title for the user and the names of
/// its fields that hold a value, for the rows; no value.
pub(crate) struct EntryShape {
    pub id: String,
    pub title: String,
    pub keys: EntryKeys,
}

/// The values of an entry's fields, read once the user chose it.
pub(crate) type EntryValues = BTreeMap<String, Zeroizing<String>>;

/// The names of the fields of `item` that hold a value (the TOTP as its marker) and its own
/// autofill aliases.
#[cfg(test)]
pub(crate) fn keys_of(item: &SecretItem, has_totp: bool) -> EntryKeys {
    let fields = fields_of(item);
    EntryKeys {
        keys: fields
            .iter()
            .filter(|(_, value)| value.as_str().is_some_and(|text| !text.is_empty()))
            .map(|(key, _)| key.clone())
            .collect(),
        aliases: serde_json::from_value(aliases_of(item)).unwrap_or_default(),
        has_totp,
    }
}

/// The values of the fields of `item`, without the TOTP marker.
pub(crate) fn values_of(item: &SecretItem) -> EntryValues {
    fields_of(item)
        .into_iter()
        .filter(|(key, _)| key.as_str() != OTP_MARKER)
        .filter_map(|(key, value)| {
            let text = value.as_str().filter(|text| !text.is_empty())?;
            Some((key, Zeroizing::new(text.to_owned())))
        })
        .collect()
}

/// Every entry `grants` reach, with only the names of its fields; an entry gone or out of reach
/// since the list is left out. Secret values are read only after the user chooses an entry.
pub(super) async fn reachable_entries(
    service: &PasswordsService,
    caller: &Caller,
    grants: &[Grant],
) -> Vec<EntryShape> {
    let Ok(shapes) = service.list_autofill_shapes(caller, grants).await else {
        return Vec::new();
    };
    shapes
        .into_iter()
        .map(|shape| EntryShape {
            id: shape.id,
            title: shape.title.unwrap_or_default(),
            keys: EntryKeys {
                keys: shape.keys,
                aliases: shape
                    .aliases
                    .as_deref()
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or_default(),
                has_totp: shape.has_totp,
            },
        })
        .collect()
}

/// The rows of entry `choice` for the fields of document `doc` (in `frame`, a website, or the tab's
/// own page): each page field the rule maps to a field of `entry` that holds a value, or to its
/// one-time code.
pub(crate) fn rows_for(
    choice: &str,
    doc: usize,
    frame: Option<&str>,
    fields: &[FieldFacts],
    labels: &[String],
    keys: &EntryKeys,
) -> Vec<BrowserMappingRow> {
    map_fields(fields, keys)
        .into_iter()
        .enumerate()
        .filter_map(|(index, target)| {
            let key = match target? {
                FieldTarget::Key(key) if key != OTP_MARKER && keys.keys.contains(&key) => key,
                FieldTarget::Key(_) => return None,
                FieldTarget::Otp => OTP_MARKER.to_owned(),
            };
            let field = labels
                .get(index)
                .filter(|label| !label.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| fields[index].name.clone());
            Some(BrowserMappingRow {
                id: format!("{choice}.{doc}.{index}"),
                entry: choice.to_owned(),
                field,
                key,
                frame: frame.map(str::to_owned),
            })
        })
        .collect()
}

/// The values of the rows the user kept, from `values`, by document and field index; the
/// one-time-code row gets `otp`, or nothing without one.
pub(crate) fn kept_values(
    rows: &[BrowserMappingRow],
    skip: &[String],
    values: &EntryValues,
    otp: Option<&str>,
) -> Vec<(usize, usize, String)> {
    rows.iter()
        .filter(|row| !skip.contains(&row.id))
        .filter_map(|row| {
            let mut parts = row.id.rsplit('.');
            let index = parts.next()?.parse().ok()?;
            let doc = parts.next()?.parse().ok()?;
            let value = if row.key == OTP_MARKER {
                otp?.to_owned()
            } else {
                values.get(&row.key)?.as_str().to_owned()
            };
            Some((doc, index, value))
        })
        .collect()
}

/// The purpose the agent gave, trimmed; `None` when it is empty or too long.
pub(crate) fn purpose_of(purpose: &str) -> Option<&str> {
    let purpose = purpose.trim();
    (!purpose.is_empty() && purpose.chars().count() <= MAX_PURPOSE).then_some(purpose)
}

impl Acting<'_> {
    /// `browser.entry.fill` on `tab`: in the form of element `form` (else the form with a password
    /// field, else the page) and in the frames of websites the agent may operate.
    pub(super) async fn fill_entry(
        &self,
        tab: &Tab,
        form: Option<u32>,
        purpose: &str,
    ) -> Result<NativeReply, NativeError> {
        let purpose = purpose_of(purpose).ok_or_else(|| {
            NativeError::new("invalid_input", "Give a purpose of 1 to 200 characters.")
        })?;
        let docs = self.documents(tab).await?;
        let result = self.fill_entry_in(tab, form, purpose, &docs.list).await;
        docs.release(self.connection).await;
        result
    }

    async fn fill_entry_in(
        &self,
        tab: &Tab,
        form: Option<u32>,
        purpose: &str,
        docs: &[Doc],
    ) -> Result<NativeReply, NativeError> {
        let facts = self.docs_facts(docs, form).await?;
        let main = &facts[0];
        self.operable(&main.url)?;
        let page = Url::parse(&main.url).map_err(|_| no_matching_entry())?;
        if page.scheme() != "https" {
            return Ok(json!({ "tab": tab.id, "filled": [], "reason": "not_encrypted" }).into());
        }
        let caller = password_caller(&self.session.caller);
        let env = self.tools.env();
        let (Some(rights), Some(service)) = (
            env.password_rights(&self.session.caller).await,
            env.passwords(),
        ) else {
            return Err(no_matching_entry());
        };
        let grants = rights.grants();
        let entries = reachable_entries(&service, &caller, &grants).await;
        if entries.is_empty() {
            return Err(no_matching_entry());
        }
        // Only entries with at least one row are offered.
        let mut offered: Vec<(BrowserEntryChoice, EntryShape, Vec<BrowserMappingRow>)> = Vec::new();
        for entry in entries {
            let choice = format!("e{}", offered.len() + 1);
            let rows: Vec<BrowserMappingRow> = facts
                .iter()
                .zip(docs)
                .enumerate()
                .flat_map(|(index, (facts, doc))| {
                    let frame = doc.site.as_deref();
                    rows_for(
                        &choice,
                        index,
                        frame,
                        &facts.fields,
                        &facts.labels,
                        &entry.keys,
                    )
                })
                .collect();
            if !rows.is_empty() {
                let title = entry.title.clone();
                offered.push((BrowserEntryChoice { id: choice, title }, entry, rows));
            }
        }
        if offered.is_empty() {
            return Err(nothing_mapped());
        }

        let question = BrowserQuestion {
            kind: BrowserPromptKind::EntryFill,
            agent: String::new(),
            site: Some(site_of(&main.url)),
            tool: Some("browser.entry.fill".to_owned()),
            label: None,
            fields: Vec::new(),
            reason: Some(purpose.to_owned()),
            bytes: None,
            entries: offered
                .iter()
                .map(|(choice, _, _)| choice.clone())
                .collect(),
            profile: None,
            mapping: offered
                .iter()
                .flat_map(|(_, _, rows)| rows.iter().cloned())
                .collect(),
        };
        let answer = self
            .tools
            .ask_for(self.session, question, self.cancel.clone())
            .await?;
        let declined = || NativeError::new("user_declined", "The user filled in nothing.");
        if answer.decision != BrowserDecision::Allow {
            return Err(declined());
        }
        let chosen = match answer.entry.as_deref() {
            Some(id) => offered.iter().find(|(choice, _, _)| choice.id == id),
            None if offered.len() == 1 => offered.first(),
            None => None,
        }
        .ok_or_else(declined)?;
        let (_, entry, rows) = chosen;
        let skip = answer.skip.unwrap_or_default();
        let wants_otp = rows
            .iter()
            .any(|row| row.key == OTP_MARKER && !skip.contains(&row.id));
        let otp = if wants_otp {
            service
                .totp_code(&caller, &grants, entry.id.clone())
                .await
                .ok()
                .map(|code| code.code)
        } else {
            None
        };
        // Only now, for the entry the user chose, holzi reads its values.
        let item = service
            .read_secret_item(&caller, &grants, entry.id.clone())
            .await
            .map_err(|_| no_matching_entry())?;
        let entry_values = values_of(&item);
        drop(item);
        let values = kept_values(rows, &skip, &entry_values, otp.as_deref());
        self.session
            .note_filled(values.iter().map(|(_, _, value)| value.as_str()));
        let mut filled = Vec::new();
        let mut in_frames = 0;
        for (index, (doc, facts)) in docs.iter().zip(&facts).enumerate() {
            let here: Vec<(usize, String)> = values
                .iter()
                .filter(|(of, _, _)| *of == index)
                .map(|(_, field, value)| (*field, value.clone()))
                .collect();
            if here.is_empty() {
                continue;
            }
            let form = if index == 0 { form } else { None };
            match self.write_in(doc, form, facts, &here).await {
                Ok(numbers) if index == 0 => filled = numbers,
                Ok(numbers) => in_frames += numbers.len(),
                Err(error) if index == 0 => return Err(error),
                Err(_) => {}
            }
        }
        let mut reply = json!({ "tab": tab.id, "filled": filled });
        if in_frames > 0 {
            reply["in_frames"] = json!(in_frames);
        }
        Ok(reply.into())
    }

    /// The fields of every document; the tab's own page in the form of element `form`. A frame
    /// that cannot be read offers nothing.
    async fn docs_facts(
        &self,
        docs: &[Doc],
        form: Option<u32>,
    ) -> Result<Vec<FillFacts>, NativeError> {
        let mut all = Vec::new();
        for (index, doc) in docs.iter().enumerate() {
            let args = json!({ "fillFields": if index == 0 { form } else { None } });
            let reply = match self.run_in(doc, &args).await {
                Ok(reply) => reply,
                Err(error) if index == 0 => return Err(error),
                Err(_) => {
                    all.push(FillFacts::default());
                    continue;
                }
            };
            let facts: FillFacts = serde_json::from_value(reply)
                .map_err(|error| browser_error(BrowserError::Protocol(error.to_string())))?;
            if facts.gone && index == 0 {
                return Err(element_gone());
            }
            if index > 0 {
                let frame_site = site_of(&facts.url);
                if !self.frame_operable(&facts.url)
                    || docs[index].site.as_deref() != Some(frame_site.as_str())
                {
                    // The approval row names the discovery-time site. Do not use values in a
                    // frame that navigated to a different site before its fields were read.
                    all.push(FillFacts::default());
                    continue;
                }
            }
            all.push(facts);
        }
        Ok(all)
    }

    /// Writes `values` into `doc` in holzi's world, if it still shows the fields of `facts`; the
    /// numbers of the elements it filled.
    async fn write_in(
        &self,
        doc: &Doc,
        form: Option<u32>,
        facts: &FillFacts,
        values: &[(usize, String)],
    ) -> Result<Vec<Value>, NativeError> {
        let args = json!({
            "fill": { "form": form, "url": facts.url, "fields": facts.fields, "values": values }
        });
        let reply = self.run_in(doc, &args).await?;
        if reply.get("changed").and_then(Value::as_bool) == Some(true) {
            return Err(NativeError::new(
                "page_changed",
                "The page changed while the user decided; nothing more was filled.",
            ));
        }
        if reply.get("gone").and_then(Value::as_bool) == Some(true) {
            return Err(element_gone());
        }
        Ok(reply["filled"].as_array().cloned().unwrap_or_default())
    }
}
