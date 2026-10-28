//! Exclusions (contracts/ignore-file.md): `.gitignore` patterns per side, their union, the fixed
//! exclusions, lines left out, and files over the limits.

use super::*;

fn ex(a: &str, b: &str) -> Exclusions {
    Exclusions::new(Some(a), Some(b)).expect("within the limits")
}

#[test]
fn patterns_work_as_in_gitignore() {
    let e = ex("*.tmp\nbuild/\n/root-only.txt\n!keep.tmp\n# comment\n", "");
    assert!(e.excludes("a.tmp", false));
    assert!(e.excludes("deep/b.tmp", false));
    assert!(!e.excludes("keep.tmp", false), "negation");
    assert!(e.excludes("build", true));
    assert!(
        e.excludes("build/out.bin", false),
        "a file below an excluded folder"
    );
    assert!(!e.excludes("build", false), "build/ matches folders only");
    assert!(e.excludes("root-only.txt", false));
    assert!(
        !e.excludes("sub/root-only.txt", false),
        "anchored at the root"
    );
    assert!(!e.excludes("a.txt", false));
}

#[test]
fn either_side_excludes() {
    let e = ex("*.tmp\n", "node_modules/\n");
    assert!(e.excludes("x.tmp", false));
    assert!(e.excludes("app/node_modules/lib.js", false));
    let only_b = Exclusions::new(None, Some("*.log\n")).unwrap();
    assert!(only_b.excludes("x.log", false));
}

#[test]
fn the_fixed_exclusions_always_hold_and_the_file_itself_never_is_excluded() {
    let e = Exclusions::new(None, None).unwrap();
    for path in [
        ".holzi-trash/run/a.txt",
        "sub/.x.txt.holzi-part-123",
        ".x.txt.holzi-old-123",
        "sub/.DS_Store",
        "Thumbs.db",
        "desktop.ini",
    ] {
        assert!(e.excludes(path, false), "{path}");
    }
    let all = ex("*\n", "");
    assert!(!all.excludes(IGNORE_FILE, false));
    assert!(all.excludes("x", false));
}

#[test]
fn a_bad_line_is_left_out_and_named() {
    let e = ex("*.tmp\n[z-a\n*.bak\n", "");
    assert!(e.excludes("x.bak", false), "the lines after it hold");
    assert_eq!(
        e.errors,
        [LineError {
            side: SyncRuleSide::A,
            line: 2,
            text: "[z-a".to_owned(),
        }]
    );
}

#[test]
fn files_over_the_limits_stop_the_run() {
    let long = "x\n".repeat(MAX_LINES + 1);
    assert!(matches!(
        Exclusions::new(None, Some(&long)),
        Err(Error::TooLarge(TooLarge {
            side: SyncRuleSide::B
        }))
    ));
    let big = "a".repeat(MAX_BYTES + 1);
    assert!(!fits(&big));
    assert!(fits("*.tmp\n"));
}

#[test]
fn the_union_keeps_the_lines_of_a_then_the_new_ones_of_b() {
    assert_eq!(
        union_text("*.tmp\n# A\nbuild/\n", "build/\n*.log\n"),
        "*.tmp\n# A\nbuild/\n*.log\n"
    );
    let union = union_text("*.tmp\n", "!keep.tmp\n");
    assert_eq!(union, "*.tmp\n!keep.tmp\n");
}
