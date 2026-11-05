//! haex-mail's tool names in a run. The app names an extension's tools `x_<slug>_<tool>` after the
//! extension's name (haex-mail: `x_haexmail_`), and a mail client could be called anything
//! (Thunderbird). The set, the fake world and the scoring speak of `x_mail_<tool>` whatever the run
//! calls the extension; the run offers and the model calls the app's names.

use std::collections::HashMap;

use crate::extensions::agent_tools::defs::ExtensionToolDef;

/// How the set names haex-mail's tools.
pub const SET_MAIL_PREFIX: &str = "x_mail_";

/// The set's name of one mail tool.
pub fn set_name(def: &ExtensionToolDef) -> String {
    format!("{SET_MAIL_PREFIX}{}", def.tool.name)
}

/// Both ways between the names a run offers and the set's names.
#[derive(Debug, Clone, Default)]
pub struct MailNames {
    to_set: HashMap<String, String>,
    to_app: HashMap<String, String>,
}

impl MailNames {
    pub fn of(mail: &[ExtensionToolDef]) -> Self {
        let pairs: Vec<(String, String)> = mail
            .iter()
            .map(|def| (def.model_name.clone(), set_name(def)))
            .collect();
        Self {
            to_set: pairs.iter().cloned().collect(),
            to_app: pairs.into_iter().map(|(app, set)| (set, app)).collect(),
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
}
