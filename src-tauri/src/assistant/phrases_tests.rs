//! Spec 066 T034: end phrases, empty utterances and stop words.

use super::lexicon::{self, lexicon};
use super::phrases::{self, is_empty, is_end};
use crate::stt::interrupt::InterruptCommand;

#[test]
fn every_end_phrase_ends_on_its_own_in_every_language() {
    for language in lexicon::languages() {
        let ends = lexicon().phrases("hear.end", language);
        assert!(!ends.is_empty(), "{language}");
        for phrase in ends {
            let text = phrase.join(" ");
            assert!(is_end(language, &text), "{language}: {text}");
        }
    }
}

#[test]
fn end_phrases_allow_small_additions() {
    assert!(is_end("de", "Danke, das war's erstmal."));
    assert!(is_end("de", "Okay, tschüss Haex!"));
    assert!(is_end("en", "Thanks, that's all for now."));
    assert!(is_end("fr", "Merci, c'est tout pour l'instant."));
    assert!(is_end("es", "Gracias, eso es todo por ahora."));
    assert!(is_end("it", "Grazie, è tutto per ora."));
    assert!(is_end("pt", "Obrigado, é tudo por enquanto."));
}

#[test]
fn end_phrases_inside_a_request_do_not_end() {
    assert!(!is_end(
        "de",
        "Das war's noch nicht, schreib Jonas auch eine Mail."
    ));
    assert!(!is_end("de", "Ich glaube, das war's schon mit der Liste"));
    assert!(!is_end("de", "Das reicht noch nicht"));
    assert!(!is_end("en", "Is that all the files you found?"));
    assert!(!is_end("en", "Say goodbye to Anna in the chat"));
    assert!(!is_end("fr", "Dis au revoir à Paul"));
}

#[test]
fn thanks_or_filler_alone_do_not_end() {
    assert!(!is_end("de", "Danke"));
    assert!(!is_end("de", "Okay, mach das, danke"));
    assert!(!is_end("en", "Thank you"));
    assert!(!is_end("de", ""));
}

#[test]
fn end_phrases_are_heard_in_their_own_language() {
    assert!(!is_end("en", "Tschüss"));
    assert!(!is_end("xx", "das war's"));
}

#[test]
fn an_utterance_of_only_hesitation_is_empty() {
    assert!(is_empty("de", ""));
    assert!(is_empty("de", "   "));
    assert!(is_empty("de", "Äh … ähm."));
    assert!(is_empty("en", "Um, hmm…"));
    assert!(!is_empty("de", "Äh, wie spät ist es?"));
    assert!(!is_empty("en", "stop"));
}

#[test]
fn stop_words_are_the_interrupt_words_of_spec_008() {
    assert_eq!(phrases::stop("Stopp!"), Some(InterruptCommand::Stop));
    assert_eq!(phrases::stop("Halt."), Some(InterruptCommand::Halt));
    assert_eq!(
        phrases::stop("Abbrechen"),
        Some(InterruptCommand::Abbrechen)
    );
    assert_eq!(phrases::stop("Arrête !"), Some(InterruptCommand::Stop));
    assert_eq!(phrases::stop("Basta"), Some(InterruptCommand::Stop));
    assert_eq!(phrases::stop("Stopp, und dann schreib Jonas"), None);
    for language in lexicon::languages() {
        for word in lexicon().phrases("hear.stop", language) {
            let text = word.join(" ");
            assert!(phrases::stop(&text).is_some(), "{language}: {text}");
        }
    }
}

#[test]
fn spoken_notices_come_from_the_word_list() {
    assert_eq!(
        phrases::repeat("de"),
        "Das habe ich nicht verstanden. Kannst du das wiederholen?"
    );
    assert_eq!(phrases::goodbye("en"), "Talk to you later.");
    for language in lexicon::languages() {
        for text in [
            phrases::repeat(language),
            phrases::busy(language),
            phrases::busy_gave_up(language),
            phrases::goodbye(language),
        ] {
            assert!(!text.is_empty(), "{language}");
        }
    }
    assert_eq!(phrases::repeat("xx"), phrases::repeat("en"));
}
