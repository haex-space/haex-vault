//! Tests for the pointers between pages (spec 097 FR-029, data-model.md "Pointers", T008).

use super::*;

#[test]
fn pointers_are_read_in_order_trimmed_and_once() {
    let body = "Mein Steuerberater, siehe [[ Steuer 2026 ]] und [[Belege]].\n\
                Nochmal [[steuer 2026]] und [[Bernd Krause|Bernd]].";
    assert_eq!(
        pointers(body),
        vec!["Steuer 2026", "Belege", "Bernd Krause"],
        "a second mention in other case is the same pointer; the shown text is not the title"
    );
}

#[test]
fn what_is_not_a_pointer() {
    assert!(pointers("[[ ]] und [[]] und [[|nur Text]]").is_empty());
    assert!(pointers("offen [[ohne Ende").is_empty());
    assert!(pointers("[[über\nzwei Zeilen]]").is_empty());
    assert!(pointers("im Code `[[Kein Verweis]]` steht").is_empty());
    assert!(pointers("```\n[[Auch kein Verweis]]\n```\n").is_empty());
    assert_eq!(
        pointers("```rust\nlet x = [[1]];\n```\nDanach [[Echter Verweis]]"),
        vec!["Echter Verweis"]
    );
}

#[test]
fn the_map_gives_pointers_out_and_in() {
    let pages = [
        ("Bernd Krause", "Steuerberater. [[Steuer 2026]], [[Fehlt]]"),
        ("Steuer 2026", "Belege bei [[bernd krause]]"),
        ("Mail-Stil", "Kurz. Für Steuersachen [[Bernd Krause]]"),
    ];
    let map = LinkMap::build(pages.iter().map(|(t, b)| (*t, *b)));
    assert_eq!(
        map.points_to("bernd krause"),
        vec![
            Pointer {
                title: "Steuer 2026".to_owned(),
                missing: false
            },
            Pointer {
                title: "Fehlt".to_owned(),
                missing: true
            },
        ]
    );
    assert_eq!(
        map.pointed_from("Bernd Krause"),
        vec!["Steuer 2026", "Mail-Stil"]
    );
    assert_eq!(map.pointed_from("Steuer 2026"), vec!["Bernd Krause"]);
    assert!(map.pointed_from("Mail-Stil").is_empty());
    assert!(map.points_to("Unbekannt").is_empty());
}

#[test]
fn a_page_pointing_to_itself_is_not_its_own_pointer_in() {
    let map = LinkMap::build([("Notiz", "siehe [[Notiz]]")]);
    assert!(map.pointed_from("Notiz").is_empty());
}

/// T014: whether the map needs a cache. A full memory (1,000 pages of 10,000 characters, 20
/// pointers each) is built per request; run with `--ignored --nocapture` to see the time.
#[test]
#[ignore = "measurement"]
fn the_map_of_a_full_memory_builds_quickly() {
    let pages: Vec<(String, String)> = (0..1_000)
        .map(|n| {
            let pointers: String = (0..20)
                .map(|k| format!("[[Seite {}]] ", (n + k) % 1_000))
                .collect();
            let filler = "Text ohne Verweis. ".repeat((10_000 - pointers.len()) / 19);
            (format!("Seite {n}"), format!("{pointers}{filler}"))
        })
        .collect();
    let started = std::time::Instant::now();
    let map = LinkMap::build(pages.iter().map(|(t, b)| (t.as_str(), b.as_str())));
    let took = started.elapsed();
    println!("LinkMap of 1,000 full pages: {took:?}");
    assert_eq!(map.pointed_from("Seite 0").len(), 19);
}
