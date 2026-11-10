use super::super::scoring::Lang;
use super::speech::{answer_language, declines, speech_fit};

#[test]
fn speech_fitness_rejects_markup_and_long_answers() {
    assert!(speech_fit("Es ist 9:41 Uhr."));
    assert!(speech_fit(
        "Anna fragt, ob es am Donnerstag um sieben klappt. Soll ich antworten?"
    ));
    assert!(!speech_fit(
        "| Von | Betreff |\n|---|---|\n| Anna | Donnerstag |"
    ));
    assert!(!speech_fit("- Anna\n- Bernd"));
    assert!(!speech_fit("Das ist **wichtig**."));
    assert!(!speech_fit("Eins. Zwei. Drei. Vier. Fünf."));
    assert!(!speech_fit("Siehe https://example.org."));
}

#[test]
fn the_language_of_an_answer_is_told_only_when_clear() {
    assert_eq!(answer_language("Es ist 9:41 Uhr."), Some(Lang::De));
    assert_eq!(answer_language("It is 09:41."), Some(Lang::En));
    assert_eq!(
        answer_language("Das kann ich leider noch nicht."),
        Some(Lang::De)
    );
    assert_eq!(
        answer_language("Sorry, I can’t set alarms."),
        Some(Lang::En)
    );
    // Too short or mixed to tell.
    assert_eq!(answer_language("9:41"), None);
    assert_eq!(answer_language("Anna Schmidt?"), None);
    assert_eq!(answer_language("Es ist the time."), None);
}

#[test]
fn a_refusal_text_declines_and_a_claim_of_done_does_not() {
    assert!(declines("Das kann ich noch nicht."));
    assert!(declines("Einen Wecker gibt es hier leider nicht."));
    assert!(declines("I can't set alarms yet."));
    assert!(declines("I don’t have a shopping list."));
    assert!(!declines("Erledigt, dein Wecker klingelt in 15 Minuten."));
    assert!(!declines("Done, your alarm is set."));
}
