//! The instruction that goes with the tools (spec 032, measured in research R17/R18). One text for
//! every model and every turn that offers tools, nothing in it depends on what the person wrote;
//! only a routed turn (a local model, `route::routes`) is also told to finish what it found and to
//! look in the next area. The
//! first measurement without it showed a local model that never searched for a tool outside the
//! core offer and answered in text instead of calling a core tool.

use crate::adapters::types::ToolSpec;
use crate::chat::reply_language::ReplyLanguage;
use crate::local_time::{Civil, Clock};

/// What the model is told about the tools holzi offers. English works best for the models that
/// run locally; the model answers in the language of the person.
pub const TOOL_INSTRUCTION: &str = "You operate the app holzi for the user with the tools you are offered. \
When the user asks to look at or change something in holzi, call a tool instead of describing what you would do. \
If none of the offered tools fits, call find_actions with a few keywords: the tools it finds are available in your next step. \
Reply in the user's language and name things by their title; never show ids such as app ids or UUIDs. \
Look up what the user names (a person, a subject, a time) with the offered tools; never ask the user for ids. \
If several match, ask which one by their titles. \
For questions that have nothing to do with holzi, just answer.";

/// The instruction for a turn whose tools do not include the optional action search.
const TOOL_INSTRUCTION_WITHOUT_SEARCH: &str = "You operate the app holzi for the user with the tools you are offered. \
When the user asks to look at or change something in holzi, call one of the offered tools instead of describing what you would do. \
Reply in the user's language and name things by their title; never show ids such as app ids or UUIDs. \
Look up what the user names (a person, a subject, a time) with the offered tools; never ask the user for ids. \
If several match, ask which one by their titles. \
For questions that have nothing to do with holzi, just answer.";

// ponytail: only routed turns get it (spec 087 research R11.2; hosted models must not get worse).
// Local models gained (Qwen3.5 4B 80 → 86, Gemma 4 E4B 79 → 80 of 106); hosted ones, which do not
// route, lost refusals by searching on (Sonnet 5.5 101 → 95). Results: spec 087 results.md "Step:
// a search that finds nothing".
/// Told to a routed turn before [`CLOSING`]: a person or a thing a search did not find may be in
/// another offered area.
pub const NEXT_AREA: &str = "If a search finds nothing, search with the other offered tools that could hold it before you say that nothing was found.";

/// Told to a routed turn after [`SEVERAL_MATCH`] (spec 087 research R7.1): models found what the
/// request was about and then asked whether to show or move it.
pub const FINISH: &str = "When you have found what the user asked about, do what was asked (show it, answer, move it) without asking whether to.";
/// The sentence both tool instructions share after the look-up sentence.
const SEVERAL_MATCH: &str = "If several match, ask which one by their titles.";
/// The last sentence of both tool instructions.
const CLOSING: &str = "For questions that have nothing to do with holzi, just answer.";

/// Added when the turn offers `ask_user` (spec 046 FR-016): small models otherwise answer that an
/// instruction they cannot map is impossible.
const ASK_USER_INSTRUCTION: &str = "If you cannot carry out an instruction unambiguously, call ask_user with the possible options instead of refusing.";

// ponytail: the bounds are shown with yesterday's dates (spec 087 research R4, T019). Told only that
// "a day runs from its date to the next", Qwen3.5 4B still sent the start alone and Gemma 4 E4B an
// end on the same day; with the dates both send the end. Results: spec 087 results.md "Step: a
// day's bounds".
/// What the model is told about where a day ends, followed by yesterday's two bounds: models sent
/// the day's start and no end, so "yesterday's mails" ran up to now.
pub const DAY_BOUNDS: &str =
    "A search for one day needs both bounds, its date as the start and the next date as the end:";

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// What the model is told about now: without it, a model knows neither the day nor the time. It
/// also names yesterday and tomorrow and where a day ends (spec 087 US4).
pub fn now_line(clock: Clock) -> String {
    let now = clock.now;
    let offset = clock.utc_offset_minutes;
    let sign = if offset < 0 { '-' } else { '+' };
    let date = |shift: i64| {
        let date = Civil::from_days(now.days() + shift, 0, 0);
        format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
    };
    let day = |shift: i64| {
        let weekday = WEEKDAYS[Civil::from_days(now.days() + shift, 0, 0).weekday()];
        format!("{weekday}, {}", date(shift))
    };
    format!(
        "The user's local date and time now: {} {:02}:{:02} (UTC{sign}{:02}:{:02}). \
         Yesterday was {}; tomorrow is {}. {DAY_BOUNDS} yesterday is {} to {}.",
        day(0),
        now.hour,
        now.minute,
        offset.abs() / 60,
        offset.abs() % 60,
        day(-1),
        day(1),
        date(-1),
        date(0),
    )
}

/// What the model is told about the language of its answers when the vault fixes one (operator
/// 2026-11-03): the interface language or the one chosen in the settings, whatever language the
/// user writes or speaks in.
pub fn language_line(language: ReplyLanguage) -> String {
    format!(
        "Always reply in {}, even when the user writes in another language.",
        language.english_name()
    )
}

/// The system prompt of a turn: what the caller passed, the tool instruction when the turn offers
/// tools (with [`NEXT_AREA`] when the turn was `routed`), the reply language when the vault fixes
/// one, and the date and time when the caller knows the clock. Without a reply language the tool
/// instruction lets the model follow the user.
pub fn system_prompt(
    given: Option<&str>,
    tools: &[ToolSpec],
    clock: Option<Clock>,
    language: Option<ReplyLanguage>,
    routed: bool,
) -> Option<String> {
    system_prompt_with(given, tools, clock, language, None, routed)
}

/// [`system_prompt`] with the `extensions` the agent profile gives (spec 018, amendment
/// 2026-11-09): which extensions the model can find tools of and what each is for
/// ([`super::extension_offer::extensions_overview`]). It follows the tool instruction and is left
/// out for a turn that cannot search, since the model finds nothing then.
pub fn system_prompt_with(
    given: Option<&str>,
    tools: &[ToolSpec],
    clock: Option<Clock>,
    language: Option<ReplyLanguage>,
    extensions: Option<&str>,
    routed: bool,
) -> Option<String> {
    let searches = tools
        .iter()
        .any(|tool| tool.name == crate::chat::tools::offer::FIND_ACTIONS_TOOL_NAME);
    let parts: Vec<String> = [
        given
            .filter(|given| !given.trim().is_empty())
            .map(str::to_owned),
        tool_instruction(tools, routed).map(|instruction| match language {
            // The fixed language replaces "the user's language" instead of contradicting it.
            Some(language) => instruction.replace(
                "Reply in the user's language and name",
                &format!("Reply in {} and name", language.english_name()),
            ),
            None => instruction,
        }),
        extensions.filter(|_| searches).map(str::to_owned),
        language.map(language_line),
        clock.map(now_line),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

/// A turn's system prompt before and after the tool search joins its offer (spec 087 research
/// R6.3): a first step that offers no search says nothing of it and leaves the extensions overview
/// out; the step that adds the search gets both. Since 2026-11-11 a routed first step offers the
/// search too, so this only comes into play for an offer that lacks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPrompt {
    pub first: Option<String>,
    pub with_search: Option<String>,
}

impl SearchPrompt {
    /// `prompt` with the turn's own part swapped for the one with the search; what compaction
    /// (spec 086) added after it stays. `None` when `prompt` does not start with the first part.
    pub fn apply(&self, prompt: Option<&str>) -> Option<Option<String>> {
        let rest = prompt
            .unwrap_or_default()
            .strip_prefix(self.first.as_deref().unwrap_or_default())?;
        let next = format!("{}{rest}", self.with_search.as_deref().unwrap_or_default());
        Some((!next.is_empty()).then_some(next))
    }
}

/// The instruction for the tools of a turn; nothing for a turn without tools.
fn tool_instruction(tools: &[ToolSpec], routed: bool) -> Option<String> {
    if tools.is_empty() {
        return None;
    }
    let instruction = if tools
        .iter()
        .any(|tool| tool.name == crate::chat::tools::offer::FIND_ACTIONS_TOOL_NAME)
    {
        TOOL_INSTRUCTION
    } else {
        TOOL_INSTRUCTION_WITHOUT_SEARCH
    };
    let instruction = if routed {
        instruction
            .replace(SEVERAL_MATCH, &format!("{SEVERAL_MATCH} {FINISH}"))
            .replace(CLOSING, &format!("{NEXT_AREA} {CLOSING}"))
    } else {
        instruction.to_owned()
    };
    Some(
        if tools
            .iter()
            .any(|tool| tool.name == crate::chat::tools::ask_user::ASK_USER_TOOL_NAME)
        {
            format!("{instruction} {ASK_USER_INSTRUCTION}")
        } else {
            instruction
        },
    )
}
