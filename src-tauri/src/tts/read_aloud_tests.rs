//! Spec 066 FR-022c: what the speaker button of a chat answer reads, in which language.

use super::read_aloud::reading;
use crate::assistant::speakable::{Civil, Clock};

fn clock() -> Clock {
    Clock {
        now: Civil::new(2026, 11, 7, 10, 0),
        utc_offset_minutes: 60,
    }
}

#[test]
fn an_answer_is_read_in_the_language_it_is_written_in() {
    let (language, sentences) = reading(
        "To die, to sleep, no more. And by a sleep to say we end the heart-ache.",
        "de",
        clock(),
    );
    assert_eq!(language, "en");
    assert_eq!(sentences.len(), 2);

    let (language, _) = reading("Das ist die Antwort auf deine Frage.", "en", clock());
    assert_eq!(language, "de");
}

#[test]
fn a_text_without_clues_takes_the_interface_language() {
    assert_eq!(reading("Hamburg", "it", clock()).0, "it");
    // An interface language haex does not speak falls back to the word list's fallback.
    assert_eq!(reading("Hamburg", "nl", clock()).0, "en");
}

#[test]
fn the_whole_answer_is_read_as_spoken_sentences() {
    let (_, sentences) = reading(
        "**Wichtig:** Eins ist erledigt. Zwei auch. Drei ebenso. Vier folgt. Fünf zuletzt.",
        "de",
        clock(),
    );
    assert_eq!(
        sentences.first().map(String::as_str),
        Some("Wichtig: Eins ist erledigt.")
    );
    assert_eq!(sentences.len(), 5, "{sentences:?}");
}

#[test]
fn an_empty_answer_reads_nothing() {
    assert!(reading("   ", "de", clock()).1.is_empty());
}
