//! The plan of a run. The rows of the spec's table follow with the stories (tasks T020, T035,
//! T044); here only what the skeleton already decides.

use super::*;
use crate::file_sync::model::{SyncRuleDeleteMode, SyncRuleDirection, SyncRulePlace};
use crate::file_sync::place::Facts;
use crate::files::SourceRef;

fn rule() -> SyncRule {
    let place = |path: &str| SyncRulePlace {
        source: SourceRef::Device,
        path: path.to_owned(),
    };
    SyncRule {
        id: "r".to_owned(),
        name: "r".to_owned(),
        place_a: place("/a"),
        place_b: place("/b"),
        direction: SyncRuleDirection::Both,
        delete_mode: SyncRuleDeleteMode::Mirror,
        interval_secs: 900,
        enabled: true,
        created_at: String::new(),
        updated_at: String::new(),
        last_run_at: None,
        last_outcome: None,
        last_error: None,
    }
}

fn listing(files: &[&str], folders: &[&str]) -> Listing {
    let mut listing = Listing::default();
    for path in files {
        listing.files.insert(
            (*path).to_owned(),
            Facts {
                size: 1,
                mtime_ms: 1,
                hash: None,
            },
        );
    }
    for path in folders {
        listing.folders.insert((*path).to_owned());
    }
    listing
}

#[test]
fn excluded_entries_are_counted_on_both_sides() {
    let exclusions = Exclusions::new(Some("*.tmp\n"), Some("cache/\n")).unwrap();
    let plan = build(
        &rule(),
        &listing(&["a.tmp", "b.txt"], &["cache"]),
        &listing(&["cache/x", "c.tmp"], &[]),
        &[],
        &exclusions,
        &Hashes::new(),
    );
    assert_eq!(plan.ignored, 4);
}
