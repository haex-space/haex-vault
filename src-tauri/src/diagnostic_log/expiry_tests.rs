use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::{VaultDb, VaultGate};

const NOW: i64 = 1_800_000_000_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const RUN: &str = "run-a";

fn vault() -> (tempfile::TempDir, VaultDb) {
    let (dir, db) = open_test_vault();
    (dir, VaultGate::new().vault_db(Arc::new(db)).unwrap())
}

fn switch_to(vault: &VaultDb, level: LogLevel, run: &'static str, now: i64) -> LevelState {
    vault
        .write_blocking(move |tx| switch(tx, level, run, now).map_err(Into::into))
        .unwrap()
}

fn read_at(vault: &VaultDb, now: i64, run: &'static str) -> Judged {
    vault
        .read_blocking(move |q| read(q, now, run).map_err(Into::into))
        .unwrap()
}

fn raw(vault: &VaultDb, level: &'static str, until: Option<i64>, run: Option<&'static str>) {
    #[allow(clippy::disallowed_methods)]
    vault
        .database()
        .with_connection(|conn| {
            conn.execute(
                "INSERT INTO haex_diagnostic_log_no_sync (id, level, until_ms, run_id) \
                 VALUES (1, ?1, ?2, ?3) \
                 ON CONFLICT(id) DO UPDATE SET level = excluded.level, \
                 until_ms = excluded.until_ms, run_id = excluded.run_id",
                params![level, until, run],
            )?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn no_row_reads_as_standard() {
    let (_dir, vault) = vault();
    let judged = read_at(&vault, NOW, RUN);
    assert_eq!(judged.state, LevelState::STANDARD);
    assert!(!judged.rewrite);
}

#[test]
fn switching_to_verbose_stores_level_end_and_run() {
    let (_dir, vault) = vault();
    let state = switch_to(&vault, LogLevel::Verbose, RUN, NOW);
    assert_eq!(state.level, LogLevel::Verbose);
    assert_eq!(state.until_ms, Some(NOW + DAY_MS));

    let judged = read_at(&vault, NOW + 1000, RUN);
    assert_eq!(judged.state, state);
    assert!(!judged.rewrite);
}

#[test]
fn switching_back_stores_standard_and_clears_end_and_run() {
    let (_dir, vault) = vault();
    switch_to(&vault, LogLevel::Verbose, RUN, NOW);
    let state = switch_to(&vault, LogLevel::Standard, RUN, NOW);
    assert_eq!(state, LevelState::STANDARD);

    #[allow(clippy::disallowed_methods)]
    let (until, run): (Option<i64>, Option<String>) = vault
        .database()
        .with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT until_ms, run_id FROM haex_diagnostic_log_no_sync WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!((until, run), (None, None));
    assert!(!read_at(&vault, NOW, RUN).rewrite);
}

#[test]
fn a_verbose_whose_end_has_passed_becomes_standard() {
    let (_dir, vault) = vault();
    switch_to(&vault, LogLevel::Verbose, RUN, NOW);
    let judged = read_at(&vault, NOW + DAY_MS, RUN);
    assert_eq!(judged.state, LevelState::STANDARD);
    assert!(judged.rewrite);
}

#[test]
fn a_verbose_of_another_run_becomes_standard() {
    let (_dir, vault) = vault();
    switch_to(&vault, LogLevel::Verbose, RUN, NOW);
    let judged = read_at(&vault, NOW + 1000, "run-b");
    assert_eq!(judged.state, LevelState::STANDARD);
    assert!(judged.rewrite, "a new start of the app resets it");
}

#[test]
fn a_damaged_row_counts_as_standard_and_is_to_be_rewritten() {
    let (_dir, vault) = vault();
    for (level, until, run) in [
        ("loud", None, None),
        ("verbose", None, Some(RUN)),
        ("verbose", Some(NOW + 1000), None),
    ] {
        raw(&vault, level, until, run);
        let judged = read_at(&vault, NOW, RUN);
        assert_eq!(
            judged.state,
            LevelState::STANDARD,
            "{level} {until:?} {run:?}"
        );
        assert!(judged.rewrite);
    }
}

#[test]
fn a_switch_creates_no_sync_change() {
    let (_dir, vault) = vault();
    let count = |vault: &VaultDb| -> i64 {
        #[allow(clippy::disallowed_methods)]
        vault
            .database()
            .with_connection(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM haex_crdt_dirty_tables_no_sync",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    };
    let before = count(&vault);
    switch_to(&vault, LogLevel::Verbose, RUN, NOW);
    switch_to(&vault, LogLevel::Standard, RUN, NOW);
    assert_eq!(count(&vault), before);
}

#[test]
fn the_remaining_time_is_capped_at_24_hours_from_the_monotonic_switch() {
    let hours = |h: u64| Duration::from_secs(h * 3600);
    // The switch was 23 hours of monotonic time ago; the stored date says 34 hours because the
    // wall clock was set back by ten: one hour remains.
    let left = remaining(NOW + 34 * 3600 * 1000, NOW, Some(hours(1)));
    assert_eq!(left, hours(1));
    // Without a monotonic switch (not this run's) the date still caps at 24 hours.
    assert_eq!(remaining(NOW + 34 * 3600 * 1000, NOW, None), hours(24));
    // An end that has passed is zero, not negative.
    assert_eq!(remaining(NOW - 5, NOW, Some(hours(3))), Duration::ZERO);
    // The wall clock can also end it earlier than the monotonic time.
    assert_eq!(remaining(NOW + 3600 * 1000, NOW, Some(hours(20))), hours(1));
}

#[test]
fn the_monotonic_deadline_counts_from_the_switch_not_from_the_start_of_the_app() {
    let start = Instant::now();
    let deadline = Deadline::new();
    assert_eq!(deadline.left(start), None, "nothing switched in this run");

    // The app has run for 30 hours when the person switches: a full day remains, not nothing.
    let switched = start + Duration::from_secs(30 * 3600);
    deadline.set(switched);
    assert_eq!(deadline.left(switched), Some(VERBOSE_FOR));
    assert_eq!(
        deadline.left(switched + Duration::from_secs(23 * 3600)),
        Some(Duration::from_secs(3600))
    );
    assert_eq!(
        deadline.left(switched + Duration::from_secs(25 * 3600)),
        Some(Duration::ZERO)
    );

    deadline.clear();
    assert_eq!(deadline.left(switched), None);
}

#[test]
fn the_run_id_stays_the_same_within_a_run() {
    assert_eq!(run_id(), run_id());
    assert!(!run_id().is_empty());
}
