//! `browser.credentials.fill` (spec 065 US4, FR-019 to FR-022, research R7): holzi fills a login of
//! the password manager into a form of the agent's tab; the agent names only the tab and perhaps
//! the form and learns only whether it worked.
//!
//! The entries are those of the agent's rights group "Passwörter" whose address has the site of
//! the page ([`PasswordsService::headers_for_page`]), read with the agent's grants; the built-in
//! agent reads through the holzi function `agent_browser` with the grants of its own setting. Only
//! over `https` and on a website the agent may operate. The first fill per website asks (in every
//! mode but Plan, which never gets here); "Für diese Website merken" lets the next fill go without
//! asking, unless several entries match. Which field gets which value decides `haex-autofill`, the
//! rule of the browser extension. The values are written in holzi's world and remembered by the
//! session, which removes them from every later reply and refuses screenshots from then on.

use std::collections::BTreeMap;

use haex_autofill::{map_fields, EntryKeys, FieldFacts, Target as FieldTarget};
use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

use super::{element_gone, Acting};
use crate::agent_browser::calls::{browser_error, site_of};
use crate::agent_browser::error::BrowserError;
use crate::agent_browser::prompts::{
    BrowserEntryChoice, BrowserPromptKind, BrowserQuestion, Remember,
};
use crate::agent_browser::read;
use crate::agent_browser::sites::Site;
use crate::agent_browser::tab::Tab;
use crate::caller::Caller;
use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::passwords::access::Grant;
use crate::passwords::autofill::{aliases_of, fields_of, OTP_MARKER};
use crate::passwords::service::PasswordsService;

/// The holzi function the built-in agent's fills read entries through (034 FR-027: the built-in
/// agent itself never reads a secret).
pub const FEATURE: &str = "agent_browser";

/// What the reading script reports about the fields a fill may write.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct FillFacts {
    gone: bool,
    url: String,
    fields: Vec<FieldFacts>,
}

/// An entry that matches the page, with its values; never leaves holzi.
pub(crate) struct LoginEntry {
    pub id: String,
    pub title: String,
    pub keys: EntryKeys,
    pub values: BTreeMap<String, String>,
}

/// Who reads the entries for `caller`.
pub fn password_caller(caller: &Caller) -> Caller {
    match caller {
        Caller::BuiltinAgent => Caller::Internal { feature: FEATURE },
        other => other.clone(),
    }
}

pub(super) fn no_matching_entry() -> NativeError {
    NativeError::new(
        "no_matching_entry",
        "No entry of the password manager that this agent may use matches this website.",
    )
}

/// The entries of `grants` for `page`, each with its values; an entry gone or out of reach since
/// the list is left out.
pub(super) async fn login_entries(
    service: &PasswordsService,
    caller: &Caller,
    grants: &[Grant],
    page: &Url,
) -> Vec<LoginEntry> {
    let Ok(headers) = service.headers_for_page(caller, grants, page).await else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for header in headers {
        let Ok(item) = service
            .read_secret_item(caller, grants, header.id.clone())
            .await
        else {
            continue;
        };
        let fields = fields_of(&item);
        let aliases = serde_json::from_value(aliases_of(&item)).unwrap_or_default();
        let values: BTreeMap<String, String> = fields
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
            .collect();
        entries.push(LoginEntry {
            id: item.id.clone(),
            title: item.title.clone().unwrap_or_default(),
            keys: EntryKeys {
                keys: fields.keys().cloned().collect(),
                aliases,
                has_totp: header.has_totp,
            },
            values,
        });
    }
    entries
}

/// The values for `fields` from `entry`, by field index; a one-time-code field gets `otp`.
pub(crate) fn values_for(
    fields: &[FieldFacts],
    entry: &LoginEntry,
    otp: Option<&str>,
) -> Vec<(usize, String)> {
    map_fields(fields, &entry.keys)
        .into_iter()
        .enumerate()
        .filter_map(|(index, target)| match target? {
            FieldTarget::Key(key) if key != OTP_MARKER => {
                Some((index, entry.values.get(&key)?.clone()))
            }
            FieldTarget::Key(_) => None,
            FieldTarget::Otp => Some((index, otp?.to_owned())),
        })
        .collect()
}

impl Acting<'_> {
    /// `browser.credentials.fill` on `tab`, in the form of element `form` or the login form.
    pub(super) async fn fill_credentials(
        &self,
        tab: &Tab,
        form: Option<u32>,
    ) -> Result<NativeReply, NativeError> {
        let facts = self.fill_facts(tab, form).await?;
        self.operable(&facts.url)?;
        let page = Url::parse(&facts.url).map_err(|_| no_matching_entry())?;
        if page.scheme() != "https" {
            return Ok(json!({ "tab": tab.id, "filled": false, "reason": "not_encrypted" }).into());
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
        let entries = login_entries(&service, &caller, &grants, &page).await;
        if entries.is_empty() {
            return Err(no_matching_entry());
        }
        let entry = self.choose(&facts.url, entries).await?;
        let otp = if entry.keys.has_totp {
            service
                .totp_code(&caller, &grants, entry.id.clone())
                .await
                .ok()
                .map(|code| code.code)
        } else {
            None
        };
        let values = values_for(&facts.fields, &entry, otp.as_deref());
        if values.is_empty() {
            return Ok(json!({ "tab": tab.id, "filled": false, "reason": "no_fields" }).into());
        }
        self.write(tab, form, &values).await?;
        Ok(json!({ "tab": tab.id, "filled": true }).into())
    }

    /// The fields a fill may write in `form` (or the login form) of `tab`.
    pub(super) async fn fill_facts(
        &self,
        tab: &Tab,
        form: Option<u32>,
    ) -> Result<FillFacts, NativeError> {
        let reply = read::script(
            self.connection,
            &tab.session,
            &json!({ "fillFields": form }),
        )
        .await
        .map_err(browser_error)?;
        let facts: FillFacts = serde_json::from_value(reply)
            .map_err(|error| browser_error(BrowserError::Protocol(error.to_string())))?;
        if facts.gone {
            return Err(element_gone());
        }
        Ok(facts)
    }

    /// The entry to fill: without asking when the website is remembered and only one entry
    /// matches, else the user's choice (FR-021).
    async fn choose(
        &self,
        url: &str,
        mut entries: Vec<LoginEntry>,
    ) -> Result<LoginEntry, NativeError> {
        let site = Site::parse(url).ok();
        let remembered = site
            .as_ref()
            .and_then(|site| self.rights.sites.get(site))
            .is_some_and(|grant| grant.fill_remembered);
        if remembered && entries.len() == 1 {
            return Ok(entries.remove(0));
        }
        let choices: Vec<BrowserEntryChoice> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| BrowserEntryChoice {
                id: format!("e{}", index + 1),
                title: entry.title.clone(),
            })
            .collect();
        let question = BrowserQuestion {
            kind: BrowserPromptKind::Fill,
            agent: String::new(),
            site: Some(site_of(url)),
            tool: Some("browser.credentials.fill".to_owned()),
            label: None,
            fields: Vec::new(),
            reason: None,
            bytes: None,
            entries: choices,
        };
        let answer = self
            .tools
            .ask_for(self.session, question, self.cancel.clone())
            .await?;
        let index = match answer.entry.as_deref() {
            Some(chosen) => chosen
                .strip_prefix('e')
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1))
                .filter(|index| *index < entries.len()),
            None if entries.len() == 1 => Some(0),
            None => None,
        }
        .ok_or_else(|| NativeError::new("user_declined", "The user chose no entry."))?;
        if answer.remember == Some(Remember::Fill) {
            if let Some(site) = &site {
                if let Err(error) = self
                    .tools
                    .env()
                    .remember_fill(&self.session.caller, site)
                    .await
                {
                    log::warn!("agent browser: remembering a fill: {error}");
                }
            }
        }
        Ok(entries.swap_remove(index))
    }

    /// Writes `values` into the fields of `form` in holzi's world; the session remembers them
    /// first, so no reply can carry them even when writing fails half way.
    pub(super) async fn write(
        &self,
        tab: &Tab,
        form: Option<u32>,
        values: &[(usize, String)],
    ) -> Result<Value, NativeError> {
        self.session
            .note_filled(values.iter().map(|(_, value)| value.as_str()));
        let args = json!({ "fill": { "form": form, "values": values } });
        read::script(self.connection, &tab.session, &args)
            .await
            .map_err(|_| browser_error(BrowserError::Protocol("fill failed".to_owned())))
    }
}
