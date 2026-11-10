//! The everyday voice measurement (spec 078): spoken everyday requests scored as whole chains against
//! a small fake world, with safety errors counted on their own and a phone verdict per model. Spec
//! 032's set, runner and in-app self-test stay as they are; this module only reuses their offer.
//! Needs no vault and runs no action (FR-005).

pub mod consent;
pub mod mail_names;
pub mod report;
pub mod runner;
pub mod scoring;
pub mod speech;
pub mod world;

#[cfg(test)]
#[path = "chainset_tests.rs"]
mod chainset_tests;
#[cfg(test)]
#[path = "consent_tests.rs"]
mod consent_tests;
#[cfg(test)]
#[path = "mail_names_tests.rs"]
mod mail_names_tests;
#[cfg(test)]
#[path = "mod_tests.rs"]
mod mod_tests;
#[cfg(test)]
#[path = "report_tests.rs"]
mod report_tests;
#[cfg(test)]
#[path = "routing_tests.rs"]
mod routing_tests;
#[cfg(test)]
#[path = "runner_dialog_tests.rs"]
mod runner_dialog_tests;
#[cfg(test)]
#[path = "runner_tests.rs"]
mod runner_tests;
#[cfg(test)]
#[path = "scoring_safety_tests.rs"]
mod scoring_safety_tests;
#[cfg(test)]
#[path = "scoring_tests.rs"]
mod scoring_tests;
#[cfg(test)]
#[path = "speech_tests.rs"]
mod speech_tests;
#[cfg(test)]
#[path = "test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "world_tests.rs"]
mod world_tests;

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chat::tools::ask_user::ASK_USER_TOOL_NAME;
use crate::chat::tools::find_actions::FIND_ACTIONS_TOOL_NAME;
use crate::extensions::agent_tools::defs::ExtensionToolDef;

use super::scoring::Lang;
use super::{embedded_tools, extension_defs, FixtureExtension};
use mail_names::set_name;

/// The haex-mail manifest the agent tools are copied from (FR-001, research R5).
pub const MAIL_TOOLS_SOURCE: &str = "haex-space/haextension@39c499801157b8e71a68a198a27591ce1a581769:apps/haex-mail/haextension/manifest.json";

const SET_JSON: &str = include_str!("set.json");
const WORLD_JSON: &str = include_str!("world.json");
const MAIL_TOOLS_JSON: &str = include_str!("mail_tools.json");

/// What a request is about (FR-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Time,
    MailRead,
    MailNew,
    MailReply,
    MailTidy,
    Contact,
    Password,
    File,
    AppSetting,
    Clarify,
    NoTool,
}

/// Written clean, or the way speech recognition delivers it (FR-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phrasing {
    Clean,
    Spoken,
}

/// How one argument of an expected call must match (data-model.md).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ArgMatch {
    Contains {
        contains: String,
    },
    #[serde(rename_all = "camelCase")]
    AnyOf {
        any_of: Vec<Value>,
    },
    #[serde(rename_all = "camelCase")]
    RelativeDay {
        relative_day: i64,
    },
    #[serde(rename_all = "camelCase")]
    FromResult {
        from_result: bool,
    },
    Exact(Value),
}

/// One call the request expects, with the arguments that matter.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExpectedCall {
    pub tool: String,
    #[serde(default)]
    pub args: BTreeMap<String, ArgMatch>,
}

/// The expected result of a request (FR-006).
#[derive(Debug, Clone, PartialEq)]
pub enum Expectation {
    /// One of these call chains, each in order; read calls may come before or between them.
    /// Most requests have one chain; `anyOf` names several equally right ones.
    Calls(Vec<Vec<ExpectedCall>>),
    /// No tool; every inner list needs one of its alternatives in the final text.
    Answer(Vec<Vec<String>>),
    /// No tool, no command, a text that is not empty.
    Refusal,
    /// `ask_user`, or a question in the final text, and no change.
    Question,
}

impl<'de> Deserialize<'de> for Expectation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Answer {
            contains: Vec<Vec<String>>,
        }
        #[derive(Deserialize)]
        struct Chain {
            calls: Vec<ExpectedCall>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Word(String),
            Answer {
                answer: Answer,
            },
            Calls {
                calls: Vec<ExpectedCall>,
            },
            AnyOf {
                #[serde(rename = "anyOf")]
                any_of: Vec<Chain>,
            },
        }
        match Raw::deserialize(deserializer)? {
            Raw::Word(word) if word == "question" => Ok(Self::Question),
            Raw::Word(word) if word == "refusal" => Ok(Self::Refusal),
            Raw::Word(word) => Err(serde::de::Error::custom(format!(
                "expect must be \"question\", \"refusal\", an answer or calls, not \"{word}\""
            ))),
            Raw::Answer { answer } if answer.contains.iter().any(Vec::is_empty) => Err(
                serde::de::Error::custom("an answer needs at least one alternative per fact"),
            ),
            Raw::Answer { answer } => Ok(Self::Answer(answer.contains)),
            Raw::Calls { calls } if calls.is_empty() => {
                Err(serde::de::Error::custom("calls must not be empty"))
            }
            Raw::Calls { calls } => Ok(Self::Calls(vec![calls])),
            Raw::AnyOf { any_of }
                if any_of.is_empty() || any_of.iter().any(|c| c.calls.is_empty()) =>
            {
                Err(serde::de::Error::custom(
                    "anyOf needs chains that are not empty",
                ))
            }
            Raw::AnyOf { any_of } => Ok(Self::Calls(any_of.into_iter().map(|c| c.calls).collect())),
        }
    }
}

/// One request of the set.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub id: String,
    pub lang: Lang,
    pub category: Category,
    pub phrasing: Phrasing,
    #[serde(default)]
    pub twin_of: Option<String>,
    pub text: String,
    /// The user asked in words to send (a reply request); the app asks for consent before
    /// `send_mail` runs, so sending the expected draft is no safety error (operator, 2026-11-03).
    #[serde(default)]
    pub send_asked: bool,
    /// A spoken request whose text misnames someone ("anna smith" for Anna Schmidt): a question
    /// back about whom the user means passes after a search that changed nothing (operator,
    /// 2026-11-03). Only on spoken requests, so a twin need not carry it.
    #[serde(default)]
    pub misheard: bool,
    pub expect: Expectation,
}

/// The versioned set (contracts/everyday-format.md).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EverydaySet {
    pub version: u32,
    pub mail_tools: String,
    pub requests: Vec<Request>,
}

#[derive(Deserialize)]
struct MailToolsFile {
    source: String,
    extension: FixtureExtension,
}

/// The pinned revision `mail_tools.json` names.
pub fn mail_tools_source() -> String {
    let file: MailToolsFile =
        serde_json::from_str(MAIL_TOOLS_JSON).expect("the embedded mail_tools.json is valid");
    file.source
}

/// haex-mail's agent tools as the app offers them: named after the extension, `x_haexmail_<tool>`.
pub fn embedded_mail_tools() -> Vec<ExtensionToolDef> {
    let file: MailToolsFile =
        serde_json::from_str(MAIL_TOOLS_JSON).expect("the embedded mail_tools.json is valid");
    extension_defs(vec![file.extension])
}

/// haex-mail's agent tools as the app would offer them for an extension called `name` with the
/// title `title` (a mail client called Thunderbird, say): the names follow the extension, the tools
/// stay haex-mail's.
pub fn embedded_mail_tools_as(name: &str, title: &str) -> Vec<ExtensionToolDef> {
    let file: MailToolsFile =
        serde_json::from_str(MAIL_TOOLS_JSON).expect("the embedded mail_tools.json is valid");
    extension_defs(vec![file.extension.renamed(name, title)])
}

/// The embedded fixture world.
pub fn embedded_world() -> world::FixtureWorld {
    serde_json::from_str(WORLD_JSON).expect("the embedded world.json is valid")
}

/// The embedded set. Panics on a set that does not load: it is built in, and a test loads it.
pub fn embedded_set() -> EverydaySet {
    load_set(SET_JSON).unwrap_or_else(|e| panic!("the embedded set.json is invalid: {e}"))
}

/// Reads and checks a set file (spec 087's chain set, `HAEX_EVAL_SET`) as [`load_set`] does.
pub fn load_set_file(path: &std::path::Path) -> Result<EverydaySet, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    load_set(&json).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parses and checks a set against the tools the chat offers and the pinned mail tools.
pub fn load_set(json: &str) -> Result<EverydaySet, String> {
    let set: EverydaySet = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let source = mail_tools_source();
    if set.mail_tools != source {
        return Err(format!(
            "the set is written for {}, the mail tools are {source}",
            set.mail_tools
        ));
    }
    let mut known: HashSet<String> = embedded_tools().into_iter().map(|d| d.tool_name).collect();
    known.extend(embedded_mail_tools().iter().map(set_name));
    known.insert(ASK_USER_TOOL_NAME.to_owned());
    known.insert(FIND_ACTIONS_TOOL_NAME.to_owned());

    let mut ids = HashSet::new();
    for request in &set.requests {
        if !ids.insert(request.id.as_str()) {
            return Err(format!("duplicate id {}", request.id));
        }
        if request.misheard && request.phrasing != Phrasing::Spoken {
            return Err(format!(
                "{}: only a spoken request can be misheard",
                request.id
            ));
        }
        if let Expectation::Calls(chains) = &request.expect {
            if let Some(call) = chains.iter().flatten().find(|c| !known.contains(&c.tool)) {
                return Err(format!("{}: unknown tool {}", request.id, call.tool));
            }
        }
    }
    for request in &set.requests {
        check_twin(&set, request)?;
    }
    Ok(set)
}

/// A spoken request names a clean twin of the same language, category and expectation (US3).
fn check_twin(set: &EverydaySet, request: &Request) -> Result<(), String> {
    match (request.phrasing, &request.twin_of) {
        (Phrasing::Clean, None) => Ok(()),
        (Phrasing::Clean, Some(_)) => Err(format!("{}: a clean request has no twin", request.id)),
        (Phrasing::Spoken, None) => Err(format!("{}: a spoken request needs twinOf", request.id)),
        (Phrasing::Spoken, Some(twin)) => {
            let clean = set
                .requests
                .iter()
                .find(|r| &r.id == twin)
                .ok_or_else(|| format!("{}: twin {twin} does not exist", request.id))?;
            let same = clean.phrasing == Phrasing::Clean
                && clean.lang == request.lang
                && clean.category == request.category
                && clean.expect == request.expect
                && clean.send_asked == request.send_asked;
            if same {
                Ok(())
            } else {
                Err(format!(
                    "{}: twin {twin} must be clean with the same language, category and expectation",
                    request.id
                ))
            }
        }
    }
}
