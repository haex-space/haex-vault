//! Tests for the table of contents and the memory block (spec 097 FR-010, FR-011, FR-017, SC-009,
//! contracts "The memory block", T026, T061).

use super::super::store::Page;
use super::*;

fn page(title: &str, description: &str, by_user: bool, at: i64) -> Page {
    Page {
        id: title.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        body: String::new(),
        author_kind: if by_user { "user" } else { "model" }.to_owned(),
        author_key: None,
        author_name: if by_user { "" } else { "Qwen3.5 4B" }.to_owned(),
        author_device: "d".to_owned(),
        created_at: at,
        changed_kind: None,
        changed_key: None,
        changed_name: None,
        changed_device: None,
        changed_at: None,
    }
}

/// 2026-11-20 and 2026-11-19, 12:00 UTC.
const NOV_20: i64 = 1_795_176_000_000;
const NOV_19: i64 = 1_795_089_600_000;

#[test]
fn a_line_names_title_description_writer_and_date() {
    let pages = [
        page("Mail-Stil", "Wie ich Mails beantworte", true, NOV_20),
        page("Bernd Krause", "Steuerberater", false, NOV_19),
    ];
    assert_eq!(
        render(&pages),
        "- Mail-Stil: Wie ich Mails beantworte (the user, 2026-11-20)\n\
         - Bernd Krause: Steuerberater (Qwen3.5 4B, 2026-11-19)"
    );
    assert_eq!(render(&[]), "");
}

#[test]
fn the_one_who_changed_last_and_when_is_named() {
    let mut changed = page("Bernd Krause", "Steuerberater", true, NOV_19);
    changed.changed_kind = Some("model".to_owned());
    changed.changed_name = Some("Gemma 4".to_owned());
    changed.changed_at = Some(NOV_20);
    assert_eq!(
        render(&[changed]),
        "- Bernd Krause: Steuerberater (Gemma 4, 2026-11-20)"
    );
}

#[test]
fn the_block_frames_the_pages_as_notes_and_asks_to_remember_only_a_writer() {
    let snapshot = render(&[page("Mail-Stil", "Kurz", true, NOV_20)]);
    let reading = block(&snapshot, None, false);
    assert!(reading.starts_with("## Memory\n"));
    assert!(reading.contains(NOTICE));
    assert!(reading.contains("- Mail-Stil: Kurz (the user, 2026-11-20)"));
    assert!(!reading.contains(REMEMBER));
    let writing = block(&snapshot, None, true);
    assert!(writing.ends_with(REMEMBER));
}

#[test]
fn an_empty_memory_says_so() {
    let block = block("", Some(32_768), true);
    assert!(block.contains("No pages yet."));
    assert!(!block.contains("more pages"));
}

#[test]
fn the_same_snapshot_gives_the_same_bytes() {
    let snapshot = render(&[page("A", "a", true, NOV_20), page("B", "b", false, NOV_19)]);
    assert_eq!(
        block(&snapshot, Some(4_096), true),
        block(&snapshot, Some(4_096), true)
    );
}

fn many(count: usize) -> String {
    let pages: Vec<Page> = (0..count)
        .map(|n| {
            page(
                &format!("Seite {n} über ein Thema des Alltags"),
                "Eine Beschreibung in einer Zeile, wie sie ein Agent für eine Seite schreibt, mit etwas Länge",
                n % 2 == 0,
                NOV_20 - n as i64 * 60_000,
            )
        })
        .collect();
    render(&pages)
}

#[test]
fn a_small_context_gets_the_pages_changed_last_and_the_count_of_the_rest() {
    let snapshot = many(500);
    for context in [4_096, 32_768, 200_000] {
        let block = block(&snapshot, Some(context), true);
        assert!(
            block.chars().count() <= context / SHARE * CHARS_PER_TOKEN,
            "{context}: {} characters",
            block.chars().count()
        );
        let shown = block.lines().filter(|line| line.starts_with("- ")).count();
        if shown < 500 {
            assert!(
                block.contains(&format!("{} more pages are not listed", 500 - shown)),
                "{context}"
            );
        }
        assert!(
            block.contains("- Seite 0 über"),
            "{context}: the page changed last comes first"
        );
    }
    assert!(!block(&snapshot, None, false).contains("more pages"));
    for context in [0, 512, 1_500, 3_000] {
        for snapshot in ["", &snapshot] {
            assert!(
                block(snapshot, Some(context), true).chars().count()
                    <= context / SHARE * CHARS_PER_TOKEN
            );
        }
    }
}

#[test]
fn a_model_on_the_list_gets_no_block_at_the_start() {
    assert!(gives_start_toc("model:p1:qwen"));
}

#[test]
fn dates_are_read_from_milliseconds() {
    assert_eq!(iso_date(0), "1970-01-01");
    assert_eq!(iso_date(NOV_20), "2026-11-20");
    assert_eq!(iso_date(951_782_400_000), "2000-02-29");
}
