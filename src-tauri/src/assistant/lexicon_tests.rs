//! Spec 066 T153: the word list is complete in every spoken language and its rows are well formed.

use std::collections::{BTreeSet, HashSet};

use super::lexicon::{self, lexicon, Cell, Lexicon, Match};

fn placeholders(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        found.insert(rest[start + 1..start + len].to_owned());
        rest = &rest[start + len + 1..];
    }
    found
}

#[test]
fn the_word_list_loads_with_the_six_spoken_languages() {
    let parsed = Lexicon::parse(include_str!("lexicon.json")).expect("lexicon.json parses");
    assert_eq!(parsed.languages(), ["de", "en", "fr", "es", "it", "pt"]);
    assert_eq!(lexicon::languages(), parsed.languages());
    assert!(!lexicon().entries().is_empty());
}

#[test]
fn ids_are_unique() {
    let mut seen = HashSet::new();
    for entry in lexicon().entries() {
        assert!(seen.insert(entry.id.as_str()), "duplicate id {}", entry.id);
    }
}

#[test]
fn every_row_has_a_cell_for_every_language_and_nothing_else() {
    let languages: BTreeSet<&str> = lexicon::languages().iter().map(String::as_str).collect();
    for entry in lexicon().entries() {
        let cells: BTreeSet<&str> = entry.cells.keys().map(String::as_str).collect();
        assert_eq!(cells, languages, "{}", entry.id);
        for (language, cell) in &entry.cells {
            let empty = match cell {
                Cell::Text(text) => text.trim().is_empty(),
                Cell::Phrases(list) => {
                    list.is_empty() || list.iter().any(|p| lexicon::words(p).is_empty())
                }
            };
            assert!(!empty, "{} has an empty cell in {language}", entry.id);
        }
    }
}

#[test]
fn each_kind_of_row_has_its_shape() {
    for entry in lexicon().entries() {
        let id = entry.id.as_str();
        let lists = id.starts_with("hear.") || id == "detect";
        let texts = id.starts_with("say.") || id.starts_with("prompt.");
        assert!(lists || texts, "{id} has no known prefix");
        for (language, cell) in &entry.cells {
            match cell {
                Cell::Phrases(_) => assert!(lists, "{id} in {language} must be a sentence"),
                Cell::Text(_) => assert!(texts, "{id} in {language} must be a list"),
            }
        }
        if id.starts_with("hear.") {
            assert!(
                matches!(
                    entry.matching,
                    Some(Match::Whole | Match::Start | Match::Contains)
                ),
                "{id} needs a match rule"
            );
        } else {
            assert!(entry.matching.is_none(), "{id} has a match rule");
        }
    }
}

#[test]
fn sentences_use_the_same_placeholders_in_every_language() {
    for entry in lexicon().entries() {
        let mut expected: Option<BTreeSet<String>> = None;
        for language in lexicon::languages() {
            let Some(Cell::Text(text)) = entry.cells.get(language) else {
                continue;
            };
            let found = placeholders(text);
            match &expected {
                None => expected = Some(found),
                Some(first) => assert_eq!(&found, first, "{} in {language}", entry.id),
            }
        }
    }
}

#[test]
fn yes_and_no_share_no_phrase_within_a_language() {
    for language in lexicon::languages() {
        let yes: HashSet<&Vec<String>> = lexicon().phrases("hear.yes", language).iter().collect();
        for no in lexicon().phrases("hear.no", language) {
            assert!(!yes.contains(no), "{language}: {no:?} is yes and no");
        }
        // A start match also overlaps when one phrase begins with the other.
        for y in &yes {
            for no in lexicon().phrases("hear.no", language) {
                assert!(
                    !y.starts_with(no) && !no.starts_with(y),
                    "{language}: {y:?} and {no:?} overlap"
                );
            }
        }
    }
}

#[test]
fn phrases_match_by_their_rule_ignoring_case_and_punctuation() {
    assert!(lexicon::hears("hear.stop", "de", "Stopp!"));
    assert!(!lexicon::hears("hear.stop", "de", "stopp mal kurz"));
    assert!(lexicon::hears("hear.yes", "de", "Ja, mach das."));
    assert!(!lexicon::hears("hear.yes", "de", "Nein, lieber nicht"));
    assert!(lexicon::hears("hear.no", "fr", "Non, pas maintenant"));
    assert!(lexicon::hears("hear.stop", "fr", "Tais-toi"));
    assert!(lexicon::hears("hear.end", "en", "That’s all"));
    assert!(!lexicon::hears("hear.stop", "xx", "stop"));
    assert!(!lexicon::hears("say.repeat", "de", "stop"));
}

#[test]
fn normalizing_keeps_letters_digits_and_inner_apostrophes() {
    assert_eq!(
        lexicon::normalize("  Danke, das war’s!  "),
        "danke das war's"
    );
    assert_eq!(lexicon::normalize("Arrête-toi…"), "arrête toi");
    assert_eq!(lexicon::normalize("'Quoted' 3.5"), "quoted 3 5");
    assert!(lexicon::words("…?!").is_empty());
}

#[test]
fn sentences_fill_placeholders_and_fall_back_to_english() {
    let parsed = Lexicon::parse(
        r#"{"languages":["de","en"],"entries":[
            {"id":"say.hello","de":"Hallo {name}.","en":"Hello {name}."}]}"#,
    )
    .unwrap();
    assert_eq!(
        parsed.say("say.hello", "de", &[("name", "Jonas")]),
        "Hallo Jonas."
    );
    assert_eq!(
        parsed.say("say.hello", "xx", &[("name", "Jo")]),
        "Hello Jo."
    );
    assert_eq!(parsed.say("say.missing", "de", &[]), "");
    assert_eq!(
        lexicon::say("say.askMore", "de", &[]),
        "Soll ich mehr vorlesen?"
    );
}

#[test]
fn the_language_of_a_text_is_the_one_with_most_frequent_words() {
    let l = lexicon();
    assert_eq!(l.detect("Was ist das für eine Datei?", "en"), "de");
    assert_eq!(l.detect("What is this file and how big is it?", "de"), "en");
    assert_eq!(
        l.detect("Qu'est-ce que je dois faire avec ce fichier ?", "de"),
        "fr"
    );
    assert_eq!(l.detect("¿Qué tengo que hacer con el archivo?", "de"), "es");
    // Nothing to go on: the previous language stays.
    assert_eq!(l.detect("Hamburg", "it"), "it");
    assert_eq!(l.detect("", "pt"), "pt");
}
