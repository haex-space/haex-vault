//! Spec 066 FR-022d: each passage of an answer is spoken in its own language.

use super::passage::PassageLanguage;

#[test]
fn a_quoted_passage_and_its_translation_switch_back_and_forth() {
    let mut p = PassageLanguage::new("de");
    assert_eq!(
        p.of("Hier ist der Anfang des Monologs, im Original und auf Deutsch."),
        "de"
    );
    assert_eq!(p.of("To be, or not to be, that is the question."), "en");
    assert_eq!(p.of("Whether 'tis nobler in the mind to suffer."), "en");
    assert_eq!(p.of("Sein oder Nichtsein, das ist hier die Frage."), "de");
    assert_eq!(p.of("Ob's edler im Gemüt, die Pfeil und Schleudern."), "de");
}

#[test]
fn a_sentence_without_clues_keeps_the_language_before() {
    let mut p = PassageLanguage::new("de");
    assert_eq!(p.of("This is what the play says about it."), "en");
    assert_eq!(p.of("Hamlet."), "en");
    assert_eq!(p.of("Sterben, schlafen."), "en");
}

#[test]
fn one_stray_word_does_not_switch() {
    let mut p = PassageLanguage::new("en");
    // "die" is a German article, but the English words outnumber it.
    assert_eq!(p.of("To die, to sleep, no more."), "en");
    let mut p = PassageLanguage::new("de");
    // One English word among German ones stays German.
    assert_eq!(p.of("Das ist ein schönes Beispiel für die Sache."), "de");
    assert_eq!(p.of("Ich finde das cool, aber nicht für mich."), "de");
}

#[test]
fn every_spoken_language_is_found() {
    let cases = [
        ("Je pense que c'est une très bonne idée pour nous.", "fr"),
        ("Creo que es una muy buena idea para todos nosotros.", "es"),
        ("Penso che sia una molto buona idea per tutti noi.", "it"),
        ("Eu acho que é uma ideia muito boa para todos nós.", "pt"),
        ("I think that this is a very good idea for all of us.", "en"),
    ];
    for (sentence, language) in cases {
        let mut p = PassageLanguage::new("de");
        assert_eq!(p.of(sentence), language, "{sentence}");
    }
}

#[test]
fn the_next_answer_starts_in_the_base_language() {
    let mut p = PassageLanguage::new("de");
    assert_eq!(p.of("That is the question for all of us."), "en");
    p.reset();
    assert_eq!(p.of("Hamlet."), "de");
}
