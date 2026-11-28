//! The header that stands in a request where an earlier tool result was left out to fit the context
//! (spec 086 FR-012a). A fixed sentence told the model only that something was gone, so it guessed what
//! and often called the same tool again; the header names the tool, its arguments, how the call ended and
//! how large the result was. A tool that only reads adds that the model may call it again; a tool that
//! changes anything does not, so an action already taken is not repeated.

use std::collections::HashSet;

use serde_json::Value;

/// How a ghost begins; also how one is recognised, so that it is never left out a second time.
pub const GHOST_START: &str = "[Tool result left out to fit the context:";
/// How many characters of the arguments a ghost shows.
const ARGS_CHARS: usize = 80;

/// The call a left-out result belongs to: the tool's name and the arguments it was called with.
pub type Call<'a> = (&'a str, &'a Value);

/// Whether `content` is a ghost already.
pub fn is_ghost(content: &str) -> bool {
    content.starts_with(GHOST_START)
}

/// The header for a result of `chars` characters that ended `is_error`. `call` is the call it answers, when
/// the request still holds it; `reads` the names of the tools that only read.
pub fn ghost(
    call: Option<Call<'_>>,
    is_error: bool,
    chars: usize,
    reads: &HashSet<String>,
) -> String {
    let outcome = if is_error { "an error" } else { "a result" };
    let Some((name, input)) = call else {
        return format!("{GHOST_START} it was {outcome} of {chars} characters.]");
    };
    let again = if reads.contains(name) {
        " Call it again if you need it."
    } else {
        ""
    };
    format!(
        "{GHOST_START} {name}({}) returned {outcome} of {chars} characters.{again}]",
        arguments(input)
    )
}

/// The arguments as JSON, without the braces of an empty object and cut to [`ARGS_CHARS`].
fn arguments(input: &Value) -> String {
    if input.as_object().is_some_and(|object| object.is_empty()) || input.is_null() {
        return String::new();
    }
    let text = input.to_string();
    if text.chars().count() <= ARGS_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(ARGS_CHARS).collect();
    cut.push('…');
    cut
}
