//! Tests for the search over the pages (spec 097 FR-028, research R4, T010).

use super::*;

fn page<'a>(id: &'a str, title: &'a str, description: &'a str, body: &'a str) -> SearchPage<'a> {
    SearchPage {
        id,
        title,
        description,
        body,
        changed_at: 0,
    }
}

fn ids(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.id.as_str()).collect()
}

#[test]
fn words_match_without_case_and_accents() {
    let pages = [
        page("1", "Bernd Krause", "Steuerberater", ""),
        page(
            "2",
            "Briefe",
            "Wie ich Briefe beende",
            "Immer mit Viele Grüße.",
        ),
    ];
    assert_eq!(ids(&search(&pages, "kraus")), vec!["1"]);
    assert_eq!(ids(&search(&pages, "grusse")), vec!["2"]);
    assert_eq!(ids(&search(&pages, "GRÜSSE")), vec!["2"]);
    assert!(search(&pages, "Anwalt").is_empty());
}

#[test]
fn title_ranks_above_description_above_body() {
    let pages = [
        page(
            "body",
            "Briefe",
            "Schluss",
            "Mein Steuerberater bekommt Briefe.",
        ),
        page("title", "Steuerberater", "Kontakt", ""),
        page("description", "Bernd", "Mein Steuerberater", ""),
    ];
    assert_eq!(
        ids(&search(&pages, "steuerberater")),
        vec!["title", "description", "body"]
    );
}

#[test]
fn more_matched_words_rank_higher_and_ties_go_to_the_page_changed_last() {
    let mut older = page("older", "Notiz", "", "Termin beim Steuerberater");
    older.changed_at = 1;
    let mut newer = page("newer", "Notiz", "", "Termin beim Steuerberater");
    newer.changed_at = 2;
    let both = page("both", "Notiz", "", "Termin beim Steuerberater im März");
    let pages = [older, newer, both];
    assert_eq!(
        ids(&search(&pages, "Steuerberater März")),
        vec!["both", "newer", "older"]
    );
}

#[test]
fn at_most_ten_results() {
    let titles: Vec<String> = (0..25).map(|n| format!("Seite {n}")).collect();
    let pages: Vec<SearchPage<'_>> = titles
        .iter()
        .map(|title| page(title, title, "", ""))
        .collect();
    assert_eq!(search(&pages, "seite").len(), SEARCH_RESULTS);
}

#[test]
fn the_excerpt_is_short_and_around_the_first_hit_in_the_text() {
    let body = format!(
        "{}Bernd hat die Unterlagen. {}",
        "ä".repeat(500),
        "ö".repeat(500)
    );
    let pages = [page("1", "Notiz", "", &body)];
    let hit = &search(&pages, "unterlagen")[0];
    assert!(hit.excerpt.chars().count() <= EXCERPT_CHARS);
    assert!(hit.excerpt.contains("Unterlagen"), "{}", hit.excerpt);
}

#[test]
fn a_hit_in_the_title_gives_the_start_of_the_text() {
    let pages = [page("1", "Mail-Stil", "Kurz", "Kurz und\nohne Anrede.")];
    assert_eq!(search(&pages, "mail")[0].excerpt, "Kurz und ohne Anrede.");
}

#[test]
fn an_empty_query_finds_nothing() {
    let pages = [page("1", "Notiz", "", "Text")];
    assert!(search(&pages, "").is_empty());
    assert!(search(&pages, "   ").is_empty());
}
