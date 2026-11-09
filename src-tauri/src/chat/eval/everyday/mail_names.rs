//! haex-mail's tool names in a run. The app names an extension's tools `x_<slug>_<tool>` after the
//! extension's name (haex-mail: `x_haexmail_`), and a mail client could be called anything
//! (Thunderbird). The set, the fake world and the scoring speak of `x_mail_<tool>` whatever the run
//! calls the extension; the run offers and the model calls the app's names.

use std::collections::HashMap;

use crate::adapters::types::ToolSpec;
use crate::extensions::agent_tools::defs::ExtensionToolDef;

/// How the set names haex-mail's tools.
pub const SET_MAIL_PREFIX: &str = "x_mail_";
/// How the set names haex-mail's routing domain.
pub const SET_MAIL_DOMAIN: &str = "x_mail";

/// The set's name of one mail tool.
pub fn set_name(def: &ExtensionToolDef) -> String {
    format!("{SET_MAIL_PREFIX}{}", def.tool.name)
}

/// Both ways between the names a run offers and the set's names.
#[derive(Debug, Clone, Default)]
pub struct MailNames {
    to_set: HashMap<String, String>,
    to_app: HashMap<String, String>,
    /// The app's domain key of the mail extension (`x_haexmail`), from its tools' prefix.
    app_domain: Option<String>,
}

impl MailNames {
    pub fn of(mail: &[ExtensionToolDef]) -> Self {
        let pairs: Vec<(String, String)> = mail
            .iter()
            .map(|def| (def.model_name.clone(), set_name(def)))
            .collect();
        let app_domain = mail.first().and_then(|def| {
            let prefix = def.model_name.strip_prefix("x_")?.split('_').next()?;
            Some(format!("x_{prefix}"))
        });
        Self {
            to_set: pairs.iter().cloned().collect(),
            to_app: pairs.into_iter().map(|(app, set)| (set, app)).collect(),
            app_domain,
        }
    }

    /// The set's name of a tool the model called; every other name stays.
    pub fn in_set<'a>(&'a self, name: &'a str) -> &'a str {
        self.to_set.get(name).map_or(name, String::as_str)
    }

    /// The app's name of a tool the set names; every other name stays.
    pub fn in_app<'a>(&'a self, name: &'a str) -> &'a str {
        self.to_app.get(name).map_or(name, String::as_str)
    }

    /// The set's key of a routed domain: the mail extension's is `x_mail`; every other key stays.
    pub fn domain_in_set<'a>(&'a self, key: &'a str) -> &'a str {
        if self.app_domain.as_deref() == Some(key) {
            SET_MAIL_DOMAIN
        } else {
            key
        }
    }

    /// The names of an offer in the set's names, sorted, so offers under different extension
    /// names compare request by request.
    pub fn offer_in_set(&self, offer: &[ToolSpec]) -> Vec<String> {
        let mut names: Vec<String> = offer
            .iter()
            .map(|tool| self.in_set(&tool.name).to_owned())
            .collect();
        names.sort();
        names
    }
}
