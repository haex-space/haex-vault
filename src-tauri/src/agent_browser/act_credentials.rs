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
use zeroize::Zeroizing;

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
use crate::passwords::autofill::{aliases_of, OTP_MARKER};
use crate::passwords::model::SecretItem;
use crate::passwords::service::PasswordsService;

/// The holzi function the built-in agent's fills read entries through (034 FR-027: the built-in
/// agent itself never reads a secret).
pub const FEATURE: &str = "agent_browser";

/// What the reading script reports about the fields a fill may write.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct FillFacts {
    pub(super) gone: bool,
    pub(super) url: String,
    pub(super) fields: Vec<FieldFacts>,
    /// The fields' labels, for the user's question.
    pub(super) labels: Vec<String>,
}

/// A matching entry shown in the choice prompt; it contains no secret values.
pub(crate) struct LoginEntry {
    pub id: String,
    pub title: String,
    pub has_totp: bool,
}

pub(crate) struct LoginSecrets {
    pub(crate) keys: EntryKeys,
    pub(crate) values: BTreeMap<String, Zeroizing<String>>,
}

impl LoginSecrets {
    fn from_item(item: &SecretItem, has_totp: bool) -> Self {
        let aliases: BTreeMap<String, Vec<String>> =
            serde_json::from_value(aliases_of(item)).unwrap_or_default();
        let aliases = aliases
            .into_iter()
            .filter(|(key, _)| matches!(key.as_str(), "username" | "password"))
            .collect();
        let mut values = BTreeMap::new();
        if let Some(value) = item.username.as_ref().filter(|value| !value.is_empty()) {
            values.insert("username".to_owned(), Zeroizing::new(value.clone()));
        }
        if let Some(value) = item.password.as_ref().filter(|value| !value.is_empty()) {
            values.insert("password".to_owned(), Zeroizing::new(value.clone()));
        }
        let mut keys: Vec<String> = values.keys().cloned().collect();
        if has_totp {
            keys.push(OTP_MARKER.to_owned());
        }
        Self {
            keys: EntryKeys {
                keys,
                aliases,
                has_totp,
            },
            values,
        }
    }
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

/// The entries of `grants` for `page`, without reading their secret values.
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
        entries.push(LoginEntry {
            id: header.id,
            title: header.title.unwrap_or_default(),
            has_totp: header.has_totp,
        });
    }
    entries
}

/// The values for `fields` from `entry`, by field index; a one-time-code field gets `otp`.
pub(crate) fn values_for(
    fields: &[FieldFacts],
    entry: &LoginSecrets,
    otp: Option<&str>,
) -> Vec<(usize, String)> {
    map_fields(fields, &entry.keys)
        .into_iter()
        .enumerate()
        .filter_map(|(index, target)| match target? {
            FieldTarget::Key(key) if matches!(key.as_str(), "username" | "password") => {
                Some((index, entry.values.get(&key)?.as_str().to_owned()))
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
        let item = service
            .read_secret_item(&caller, &grants, entry.id.clone())
            .await
            .map_err(|_| no_matching_entry())?;
        let secrets = LoginSecrets::from_item(&item, entry.has_totp);
        let otp = if entry.has_totp {
            service
                .totp_code(&caller, &grants, entry.id.clone())
                .await
                .ok()
                .map(|code| code.code)
        } else {
            None
        };
        let fresh = self.fill_facts(tab, form).await?;
        self.operable(&fresh.url)?;
        let current_page = Url::parse(&fresh.url).map_err(|_| no_matching_entry())?;
        if current_page.scheme() != "https" || current_page.origin() != page.origin() {
            return Err(no_matching_entry());
        }
        let values = values_for(&fresh.fields, &secrets, otp.as_deref());
        if values.is_empty() {
            return Ok(json!({ "tab": tab.id, "filled": false, "reason": "no_fields" }).into());
        }
        self.write(tab, form, &fresh, &values).await?;
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
        let profile = self.tools.env().profile_note(&self.session.caller).await;
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
            profile: profile.clone(),
            mapping: Vec::new(),
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
                    .remember_fill(
                        &self.session.caller,
                        site,
                        profile.as_ref().map(|profile| profile.id.as_str()),
                    )
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
        facts: &FillFacts,
        values: &[(usize, String)],
    ) -> Result<Value, NativeError> {
        self.session
            .note_filled(values.iter().map(|(_, value)| value.as_str()));
        let args = json!({
            "fill": {
                "form": form,
                "url": facts.url,
                "fields": facts.fields,
                "values": values
            }
        });
        let reply = read::script(self.connection, &tab.session, &args)
            .await
            .map_err(|_| browser_error(BrowserError::Protocol("fill failed".to_owned())))?;
        if reply.get("changed").and_then(Value::as_bool) == Some(true) {
            return Err(NativeError::new(
                "page_changed",
                "The page changed while the user decided; nothing was filled.",
            ));
        }
        if reply.get("gone").and_then(Value::as_bool) == Some(true) {
            return Err(element_gone());
        }
        Ok(reply)
    }
}
