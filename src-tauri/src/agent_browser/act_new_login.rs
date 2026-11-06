//! `browser.credentials.create` (spec 065 US4, FR-036, research R7): a website needs a new login
//! (an account, a changed password). The agent names the new password field and the username;
//! holzi generates the password from the default preset (`haex-passgen`), writes it into that
//! field and the repeat field of its form, and asks every time. "Speichern" stores an entry with
//! the username, the password and the website in the password manager; "Ablehnen" empties the
//! fields again. The agent learns only `created`.
//!
//! The entry is created with the agent's resolved caller and its grants (the built-in agent through
//! the holzi function `agent_browser`), so 034's checks hold: the rights group "Passwörter" must
//! allow writing, and the entry gets the tag chosen there for new entries (with chosen tags it is
//! one of them). Those checks run before anything is generated; the question never replaces them.

use serde::Deserialize;
use serde_json::json;
use url::Url;

use super::credentials::password_caller;
use super::{element_gone, Acting};
use crate::agent_browser::calls::{browser_error, site_of};
use crate::agent_browser::error::BrowserError;
use crate::agent_browser::prompts::{BrowserDecision, BrowserPromptKind, BrowserQuestion};
use crate::agent_browser::read;
use crate::agent_browser::tab::Tab;
use crate::chat::tools::native_action::{NativeError, NativeReply};
use crate::mcp_server::grants::PasswordRights;
use crate::passwords::access::authorize_create;
use crate::passwords::model::ItemInput;

/// The longest username the agent may name.
pub const MAX_USERNAME: usize = 200;

/// What the page script reports about a new password field.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct NewPassword {
    gone: bool,
    not_password: bool,
    changed: bool,
    url: String,
}

pub(super) fn no_write_rights() -> NativeError {
    NativeError::new(
        "no_write_rights",
        "This agent may not create entries in the password manager: its rights group \
         \"Passwörter\" does not allow writing.",
    )
}

/// The tags a new entry of an agent with `rights` gets: the tag chosen for new entries, if any.
pub(crate) fn tags_for_new_entry(rights: &PasswordRights) -> Vec<String> {
    rights.default_tag.iter().cloned().collect()
}

/// The entry for a new login on `page`: titled with the website's host, its address the origin.
pub(crate) fn new_entry(
    page: &Url,
    username: &str,
    password: &str,
    tags: Vec<String>,
) -> ItemInput {
    ItemInput {
        title: page.host_str().map(str::to_owned),
        url: Some(page.origin().ascii_serialization()),
        username: Some(username.to_owned()),
        password: Some(password.to_owned()),
        tags,
        ..ItemInput::default()
    }
}

impl Acting<'_> {
    /// `browser.credentials.create` on `tab` for the password field `element`.
    pub(super) async fn create_credentials(
        &self,
        tab: &Tab,
        element: u32,
        username: &str,
    ) -> Result<NativeReply, NativeError> {
        let username = username.trim();
        if username.is_empty() || username.chars().count() > MAX_USERNAME {
            return Err(NativeError::new(
                "invalid_input",
                "Name a username of 1 to 200 characters.",
            ));
        }
        let facts = self.new_password(tab, element, None, None).await?;
        if facts.not_password {
            return Err(NativeError::new(
                "not_password_field",
                "This element is not a password field.",
            ));
        }
        self.operable(&facts.url)?;
        let page = Url::parse(&facts.url).map_err(|_| no_write_rights())?;
        if page.scheme() != "https" {
            return Ok(
                json!({ "tab": tab.id, "created": false, "reason": "not_encrypted" }).into(),
            );
        }
        let caller = password_caller(&self.session.caller);
        let env = self.tools.env();
        let (Some(rights), Some(service)) = (
            env.password_rights(&self.session.caller).await,
            env.passwords(),
        ) else {
            return Err(no_write_rights());
        };
        let grants = rights.grants();
        let tags = tags_for_new_entry(&rights);
        authorize_create(&caller, &grants, &tags).map_err(|_| no_write_rights())?;
        let password = service
            .generate_for_new_login(&caller, &grants)
            .await
            .map_err(|_| no_write_rights())?;
        self.session.note_filled([password.as_str()]);
        self.new_password(tab, element, Some(&password), Some(&facts.url))
            .await?;

        let question = BrowserQuestion {
            kind: BrowserPromptKind::NewCredentials,
            agent: String::new(),
            site: Some(site_of(&facts.url)),
            tool: Some("browser.credentials.create".to_owned()),
            label: Some(username.to_owned()),
            fields: Vec::new(),
            reason: None,
            bytes: None,
            entries: Vec::new(),
            profile: None,
            mapping: Vec::new(),
        };
        let saved = match self
            .tools
            .ask_for(self.session, question, self.cancel.clone())
            .await
        {
            Ok(answer) if answer.decision == BrowserDecision::Allow => {
                // Do not save credentials for a page that navigated while the user was deciding.
                self.new_password(tab, element, None, Some(&facts.url))
                    .await?;
                service
                    .create_item(
                        &caller,
                        &grants,
                        new_entry(&page, username, &password, tags),
                        None,
                    )
                    .await
                    .map(|_| ())
                    .map_err(|_| no_write_rights())
            }
            Ok(_) => Err(NativeError::new(
                "user_declined",
                "The user did not save the new login; holzi emptied the fields.",
            )),
            Err(error) => Err(error),
        };
        if let Err(error) = saved {
            if let Err(cleared) = self
                .new_password(tab, element, Some(""), Some(&facts.url))
                .await
            {
                log::warn!(
                    "agent browser: emptying a declined password: {}",
                    cleared.code
                );
            }
            return Err(error);
        }
        Ok(json!({ "tab": tab.id, "created": true }).into())
    }

    /// Reports on the password field `element` of `tab`, or writes `value` into it and its repeat
    /// field in holzi's world.
    async fn new_password(
        &self,
        tab: &Tab,
        element: u32,
        value: Option<&str>,
        expected_url: Option<&str>,
    ) -> Result<NewPassword, NativeError> {
        let args = json!({
            "newPassword": {
                "element": element,
                "value": value,
                "url": expected_url,
            }
        });
        let reply = read::script(self.connection, &tab.session, &args)
            .await
            .map_err(browser_error)?;
        let facts: NewPassword = serde_json::from_value(reply)
            .map_err(|error| browser_error(BrowserError::Protocol(error.to_string())))?;
        if facts.gone {
            return Err(element_gone());
        }
        if facts.changed {
            return Err(NativeError::new(
                "page_changed",
                "The page changed while the user decided; nothing was saved.",
            ));
        }
        Ok(facts)
    }
}
