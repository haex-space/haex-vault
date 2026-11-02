//! Local, deterministic interrupt-word matching (spec 008 FR-007/FR-009).
//!
//! A pure function — no adapter, no provider involved — because
//! recognizing an interrupt must not depend on which STT backend
//! transcribed the utterance, or on the assistant's own state (FR-008).
//! The bounded local fast path (`stop_voice_recording`, T019) calls this on
//! the locally transcribed text before waiting on any external provider.

use serde::Serialize;

use crate::assistant::lexicon;

/// Wire representation matches exactly `"stop"` / `"halt"` / `"abbrechen"`
/// (data-model.md §`InterruptCommand`) — never a Rust variant name — since
/// `TranscriptionResult.interrupt` serializes this directly to the
/// frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InterruptCommand {
    Stop,
    Halt,
    Abbrechen,
}

/// Matches only when the *entire* utterance is one of the stop words of the word list (row
/// `hear.stop`, any spoken language), ignoring case and punctuation. A longer sentence merely
/// containing one of them (e.g. "bitte nicht mehr stoppen mitten im Satz") must not match — no
/// substring match (FR-009, SC-006). "halt" and "abbrechen" keep their own wire value; every other
/// stop word is `stop`.
pub fn match_interrupt(transcript: &str) -> Option<InterruptCommand> {
    let lexicon = lexicon::lexicon();
    let word = lexicon
        .languages()
        .iter()
        .find_map(|language| lexicon.matched("hear.stop", language, transcript))?;
    Some(match word {
        [w] if w == "halt" => InterruptCommand::Halt,
        [w] if w == "abbrechen" => InterruptCommand::Abbrechen,
        _ => InterruptCommand::Stop,
    })
}
