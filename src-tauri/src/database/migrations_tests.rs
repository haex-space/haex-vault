//! Guard tests for the hand-maintained manual-migrations folder.
//!
//! These tests validate (they do NOT generate) the consistency between
//! `database/migrations-manual/_journal.json` and the `.sql` files sitting
//! next to it. The journal is hand-maintained at dev time, so it is easy to
//! add a `.sql` file and forget to journal it (it would then never be applied
//! by the runner) or journal a tag whose `.sql` is missing (the runner would
//! fail at vault open). Both classes of bug are caught here at test time.
//!
//! The bundled runner reads these files via the Tauri resource resolver at
//! runtime; here in a plain unit test there is no AppHandle, so we resolve the
//! folder relative to `CARGO_MANIFEST_DIR` (the `src-tauri` crate root).

use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct ManualJournalEntry {
    tag: String,
}

#[derive(Debug, Deserialize)]
struct ManualJournal {
    entries: Vec<ManualJournalEntry>,
}

fn manual_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("database/migrations-manual")
}

#[test]
fn manual_journal_and_sql_files_are_in_sync() {
    let dir = manual_dir();
    let journal_path = dir.join("_journal.json");

    let journal_content =
        std::fs::read_to_string(&journal_path).expect("manual _journal.json must exist");
    let journal: ManualJournal =
        serde_json::from_str(&journal_content).expect("manual _journal.json must be valid JSON");

    // Collect into a Vec first so duplicate tags are detectable — building a
    // HashSet directly would silently dedupe a duplicated manual tag.
    let journaled_tags_vec: Vec<String> = journal.entries.iter().map(|e| e.tag.clone()).collect();
    let journaled_tags: HashSet<String> = journaled_tags_vec.iter().cloned().collect();
    assert_eq!(
        journaled_tags_vec.len(),
        journaled_tags.len(),
        "manual _journal.json contains duplicate tags: {journaled_tags_vec:?}"
    );

    // Every manual tag must carry the `manual_` prefix. This is the naming
    // convention that keeps manual tags from colliding by-name with drizzle
    // tags in the applied-migrations tracking (see `load_bundled_migrations`).
    let missing_prefix: Vec<&String> = journaled_tags
        .iter()
        .filter(|tag| !tag.starts_with("manual_"))
        .collect();
    assert!(
        missing_prefix.is_empty(),
        "manual journal tags must start with the 'manual_' prefix: {missing_prefix:?}"
    );

    // Collect the .sql files actually present in the folder.
    let mut sql_tags: HashSet<String> = HashSet::new();
    for entry in std::fs::read_dir(&dir).expect("manual migrations folder must be readable") {
        let entry = entry.expect("readable dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("sql") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("sql file has a valid stem")
                .to_string();
            sql_tags.insert(stem);
        }
    }

    // Every journal entry must resolve to an existing .sql file.
    for tag in &journaled_tags {
        let sql_path = dir.join(format!("{tag}.sql"));
        assert!(
            sql_path.exists(),
            "manual journal lists tag '{tag}' but {sql_path:?} does not exist"
        );
    }

    // Every .sql file in the folder must be listed in the journal
    // (catches "file present but not journaled" — it would never be applied).
    let missing_from_journal: Vec<&String> = sql_tags.difference(&journaled_tags).collect();
    assert!(
        missing_from_journal.is_empty(),
        "manual .sql files present but not listed in _journal.json: {missing_from_journal:?}"
    );

    // And the reverse set difference, with a clearer message.
    let missing_files: Vec<&String> = journaled_tags.difference(&sql_tags).collect();
    assert!(
        missing_files.is_empty(),
        "manual journal tags with no matching .sql file: {missing_files:?}"
    );
}
