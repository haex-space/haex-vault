//! The switch of the level and the start and end of the log of a vault (spec 098 T015). The log
//! and the level are process-wide, so each test takes `TEST_LOCK` and ends the log when done.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use haex_crdt::rusqlite::params;
use log::{Level, LevelFilter};
use tokio::sync::MutexGuard;

use super::*;
use crate::diagnostic_log::{end, expiry_running, start, status, TEST_LOCK};
use crate::passwords::test_support::open_test_vault;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

struct Fixture {
    _lock: MutexGuard<'static, ()>,
    _vault_dir: tempfile::TempDir,
    logs: tempfile::TempDir,
    db: VaultDb,
    gate: VaultGate,
    events: Arc<Mutex<Vec<DiagnosticLogStatus>>>,
    emit: Emitter,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        end();
    }
}

impl Fixture {
    async fn new() -> Self {
        let lock = TEST_LOCK.lock().await;
        end();
        let (vault_dir, db) = open_test_vault();
        let gate = VaultGate::new();
        let db = gate.vault_db(Arc::new(db)).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&events);
        let emit: Emitter = Arc::new(move |status| seen.lock().unwrap().push(status));
        Self {
            _lock: lock,
            _vault_dir: vault_dir,
            logs: tempfile::tempdir().unwrap(),
            db,
            gate,
            events,
            emit,
        }
    }

    fn folder(&self) -> std::path::PathBuf {
        self.logs.path().join("logs")
    }

    async fn start(&self) {
        start(&self.db, &self.gate, &self.folder(), &self.emit).await;
    }

    async fn switch(&self, level: LogLevel) -> DiagnosticLogStatus {
        switch_level(&self.db, &self.gate, level, &self.emit)
            .await
            .unwrap()
    }

    fn events(&self) -> Vec<DiagnosticLogStatus> {
        self.events.lock().unwrap().clone()
    }

    fn stored(&self) -> expiry::Judged {
        let now = now_ms();
        self.db
            .read_blocking(move |q| expiry::read(q, now, expiry::run_id()).map_err(Into::into))
            .unwrap()
    }

    fn store_raw(&self, level: &'static str, until: Option<i64>, run: Option<&'static str>) {
        #[allow(clippy::disallowed_methods)]
        self.db
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

    #[allow(clippy::disallowed_methods)]
    fn execute_sql(&self, sql: &str) {
        let sql = sql.to_owned();
        self.db
            .database()
            .with_connection(|conn| {
                conn.execute_batch(&sql)?;
                Ok(())
            })
            .unwrap();
    }
}

#[tokio::test]
async fn a_fresh_vault_starts_at_standard_writable_and_announces_its_session() {
    let f = Fixture::new().await;
    f.start().await;

    let status = status();
    assert_eq!(status.level, LogLevel::Standard);
    assert_eq!(status.until_ms, None);
    assert!(status.writable);
    assert!(status.session_id.is_some());
    assert_eq!(log::max_level(), LevelFilter::Info);
    assert!(f.folder().join("haex.log").exists());
    let events = f.events();
    assert_eq!(events.len(), 1, "one event at the start");
    assert_eq!(
        events[0], status,
        "the event carries the token of this unlock"
    );
}

#[tokio::test]
async fn nothing_is_attached_before_the_start_and_after_the_end() {
    let f = Fixture::new().await;
    assert_eq!(status().session_id, None);
    ui_message("any", UiLevel::Error, "nowhere");
    assert!(!f.folder().exists());

    f.start().await;
    end();
    assert_eq!(status().session_id, None);
    assert_eq!(status().level, LogLevel::Standard);
    assert_eq!(log::max_level(), LevelFilter::Info);
    assert!(!expiry_running());
}

#[tokio::test]
async fn verbose_sets_debug_and_ends_24_hours_after_the_switch() {
    let f = Fixture::new().await;
    f.start().await;
    let before = now_ms();

    let status = f.switch(LogLevel::Verbose).await;

    assert_eq!(status.level, LogLevel::Verbose);
    let until = status.until_ms.expect("an end time");
    assert!(until >= before + DAY_MS && until <= now_ms() + DAY_MS);
    assert_eq!(log::max_level(), LevelFilter::Debug);
    assert_eq!(f.stored().state.level, LogLevel::Verbose);
    assert!(expiry_running(), "the timer runs without a restart");
}

#[tokio::test]
async fn repeating_the_level_in_force_changes_nothing() {
    let f = Fixture::new().await;
    f.start().await;
    f.switch(LogLevel::Verbose).await;
    let events = f.events().len();
    let until = status().until_ms;

    f.switch(LogLevel::Verbose).await;

    assert_eq!(f.events().len(), events, "no second event");
    assert_eq!(status().until_ms, until, "the end is not pushed out");
}

#[tokio::test]
async fn each_change_is_announced_once_with_the_token_of_the_unlock() {
    let f = Fixture::new().await;
    f.start().await;
    let session = status().session_id;

    f.switch(LogLevel::Verbose).await;
    f.switch(LogLevel::Standard).await;

    let events = f.events();
    assert_eq!(events.len(), 3, "the start and two changes");
    assert_eq!(events[1].level, LogLevel::Verbose);
    assert_eq!(events[2].level, LogLevel::Standard);
    assert!(events.iter().all(|event| event.session_id == session));
}

#[tokio::test]
async fn standard_cancels_the_timer_and_restores_info() {
    let f = Fixture::new().await;
    f.start().await;
    f.switch(LogLevel::Verbose).await;
    assert!(expiry_running());

    let status = f.switch(LogLevel::Standard).await;

    assert_eq!(status.level, LogLevel::Standard);
    assert_eq!(status.until_ms, None);
    assert_eq!(log::max_level(), LevelFilter::Info);
    assert!(!expiry_running());
    assert_eq!(DEADLINE.left(std::time::Instant::now()), None);
    assert_eq!(f.stored().state, expiry::LevelState::STANDARD);
}

#[tokio::test]
async fn switching_back_to_standard_cuts_the_archives_down_at_once() {
    let f = Fixture::new().await;
    f.start().await;
    f.switch(LogLevel::Verbose).await;
    let record = "v".repeat(16 * 1024 - 100);
    for number in 0..200 {
        SINK.write_record(
            Level::Debug,
            "haex_lib::test",
            &format!("{number:03} {record}"),
        );
    }

    f.switch(LogLevel::Standard).await;

    let limits = LogLevel::Standard.limits();
    let mut total = 0u64;
    let mut files = 0usize;
    for entry in std::fs::read_dir(f.folder()).unwrap() {
        let entry = entry.unwrap();
        let len = entry.metadata().unwrap().len();
        assert!(
            len <= limits.bytes,
            "{:?} is {len} bytes",
            entry.file_name()
        );
        total += len;
        files += 1;
    }
    assert!(files <= limits.files, "{files} files");
    assert!(total <= limits.bytes * limits.files as u64);
    SINK.flush_sync();
    let newest = std::fs::read_to_string(f.folder().join("haex.log")).unwrap();
    assert!(newest.contains("] 199 "), "the newest record stays");
    assert!(
        newest.lines().all(|line| line.starts_with('[')),
        "no record is split"
    );
}

#[tokio::test]
async fn a_ui_message_is_written_only_for_the_vault_that_is_open() {
    let f = Fixture::new().await;
    f.start().await;
    let token = status().session_id.unwrap();

    ui_message(&token, UiLevel::Error, "Error: probe");
    ui_message("stale", UiLevel::Error, "from an earlier vault");
    SINK.flush_sync();

    let text = std::fs::read_to_string(f.folder().join("haex.log")).unwrap();
    assert!(text.contains("[ERROR][ui] Error: probe"));
    assert!(!text.contains("earlier vault"));
}

#[tokio::test]
async fn a_verbose_of_an_earlier_run_is_reset_at_the_start() {
    let f = Fixture::new().await;
    f.store_raw("verbose", Some(now_ms() + DAY_MS), Some("another-run"));

    f.start().await;

    assert_eq!(status().level, LogLevel::Standard);
    assert_eq!(log::max_level(), LevelFilter::Info);
    assert!(!f.stored().rewrite, "the row was rewritten as Standard");
    assert_eq!(f.stored().state, expiry::LevelState::STANDARD);
    assert!(!expiry_running());
    assert_eq!(f.events().last().unwrap().level, LogLevel::Standard);
}

#[tokio::test]
async fn a_verbose_of_this_run_that_has_not_ended_stays_and_keeps_its_timer() {
    let f = Fixture::new().await;
    f.start().await;
    f.switch(LogLevel::Verbose).await;
    let until = status().until_ms;

    // The same run opens the vault again (a second vault session).
    f.start().await;

    assert_eq!(status().level, LogLevel::Verbose);
    assert_eq!(status().until_ms, until);
    assert!(expiry_running());
}

#[tokio::test]
async fn a_damaged_row_starts_at_standard_and_is_rewritten() {
    let f = Fixture::new().await;
    f.store_raw("loud", None, None);

    f.start().await;

    assert_eq!(status().level, LogLevel::Standard);
    assert_eq!(f.stored().state, expiry::LevelState::STANDARD);
    assert!(!f.stored().rewrite);
}

#[tokio::test]
async fn the_timer_ends_verbose_by_itself_and_announces_it() {
    let f = Fixture::new().await;
    f.start().await;
    f.switch(LogLevel::Verbose).await;

    // The end time comes within a moment (as if 24 hours had passed).
    let until_ms = now_ms() + 50;
    apply_state(expiry::LevelState {
        level: LogLevel::Verbose,
        until_ms: Some(until_ms),
    });
    schedule_expiry(f.db.clone(), &f.gate, until_ms, &f.emit);

    let mut waited = 0;
    while !f
        .events()
        .last()
        .is_some_and(|event| event.level == LogLevel::Standard)
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
        waited += 20;
        assert!(waited < 5000, "expiry did not emit Standard");
    }
    assert_eq!(log::max_level(), LevelFilter::Info);
    assert_eq!(f.stored().state, expiry::LevelState::STANDARD);
    assert_eq!(f.events().last().unwrap().level, LogLevel::Standard);
    assert!(!expiry_running());
}

#[tokio::test]
async fn the_timer_retries_expiry_after_a_transient_database_failure() {
    let f = Fixture::new().await;
    f.start().await;
    f.switch(LogLevel::Verbose).await;
    f.execute_sql(
        "CREATE TRIGGER fail_diagnostic_log_expiry BEFORE UPDATE \
         ON haex_diagnostic_log_no_sync BEGIN \
         SELECT RAISE(ABORT, 'temporary failure'); END;",
    );

    let until_ms = now_ms() + 50;
    apply_state(expiry::LevelState {
        level: LogLevel::Verbose,
        until_ms: Some(until_ms),
    });
    schedule_expiry(f.db.clone(), &f.gate, until_ms, &f.emit);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(status().level, LogLevel::Verbose);

    f.execute_sql("DROP TRIGGER fail_diagnostic_log_expiry;");

    let mut waited = 0;
    while !f
        .events()
        .last()
        .is_some_and(|event| event.level == LogLevel::Standard)
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
        waited += 20;
        assert!(waited < 7000, "expiry retry did not emit Standard");
    }
    assert_eq!(f.stored().state, expiry::LevelState::STANDARD);
    assert!(!expiry_running());
}

#[tokio::test]
async fn a_log_that_cannot_start_is_not_writable_and_does_not_fail_the_unlock() {
    let f = Fixture::new().await;
    std::fs::write(f.folder(), "a file where the folder belongs").unwrap();

    f.start().await;

    assert!(!status().writable);
    assert_eq!(status().session_id, None);
    assert_eq!(f.events().len(), 1, "the window is told");
}

#[tokio::test]
async fn a_message_queued_for_the_first_vault_does_not_reach_the_second() {
    let f = Fixture::new().await;
    f.start().await;
    let first = status().session_id.unwrap();
    end();
    f.start().await;
    let second = status().session_id.unwrap();
    assert_ne!(first, second, "every unlock has its own token");

    // The webview had queued this message before the first vault closed; it arrives now.
    ui_message(&first, UiLevel::Error, "queued for the first vault");
    ui_message(&second, UiLevel::Error, "for the second vault");
    SINK.flush_sync();

    let text = std::fs::read_to_string(f.folder().join("haex.log")).unwrap();
    assert!(text.contains("for the second vault"));
    assert!(!text.contains("first vault"));
}

#[tokio::test]
async fn lines_of_the_backend_and_of_the_interface_read_in_the_order_they_came() {
    let f = Fixture::new().await;
    f.start().await;
    let token = status().session_id.unwrap();

    for n in 0..20 {
        SINK.write_record(Level::Warn, "haex::sync", &format!("backend {n}"));
        ui_message(&token, UiLevel::Warn, &format!("interface {n}"));
    }
    SINK.flush_sync();

    let text = std::fs::read_to_string(f.folder().join("haex.log")).unwrap();
    let seen: Vec<(&str, usize)> = text
        .lines()
        .filter_map(|line| {
            if line.contains("[ui]") {
                Some(("ui", line.rsplit_once("interface ")?.1.parse().ok()?))
            } else {
                Some(("backend", line.rsplit_once("backend ")?.1.parse().ok()?))
            }
        })
        .collect();
    let expected: Vec<(&str, usize)> = (0..20).flat_map(|n| [("backend", n), ("ui", n)]).collect();
    assert_eq!(seen, expected, "one story, in one order: {text}");
}

fn sources_below(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources_below(&path, extension, out);
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let is_test = name.contains("_tests.") || name.contains("test_support");
        if name.ends_with(extension) && !is_test {
            out.push(path);
        }
    }
}

/// FR-007 and spec 098 T028: the command of the interface's messages is for the app's own window.
/// Extension frames have no IPC of their own (the capabilities name the window `main` only), and
/// no agent action, MCP tool or host method of an extension may name the command, so none of them
/// can write a line into the log.
#[test]
fn only_the_main_window_can_write_a_ui_message() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for file in ["default.json", "mobile.json"] {
        let text = std::fs::read_to_string(root.join("capabilities").join(file)).unwrap();
        let capability: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            capability["windows"],
            serde_json::json!(["main"]),
            "{file} grants its permissions to the main window only"
        );
        assert!(
            capability.get("remote").is_none(),
            "{file} gives no remote origin IPC access"
        );
    }

    let mut files = Vec::new();
    for dir in [
        "src/mcp_server",
        "src/extensions/bridge",
        "src/extensions/agent_tools",
    ] {
        sources_below(&root.join(dir), ".rs", &mut files);
    }
    sources_below(&root.join("../src/lib/actions"), ".ts", &mut files);
    if let Ok(entries) = std::fs::read_dir(root.join("../src/stores")) {
        files.extend(entries.flatten().map(|e| e.path()).filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("ActionHandlers.ts"))
        }));
    }
    assert!(
        files.len() > 10,
        "the registries were found: {}",
        files.len()
    );
    let hits: Vec<String> = files
        .iter()
        .filter(|file| {
            std::fs::read_to_string(file)
                .unwrap_or_default()
                .contains("diagnostic_log_ui_message")
        })
        .map(|file| file.display().to_string())
        .collect();
    assert!(
        hits.is_empty(),
        "agents and extensions must not reach diagnostic_log_ui_message, found at {hits:?}"
    );

    let lib = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert_eq!(
        lib.matches("diagnostic_log::commands::diagnostic_log_ui_message")
            .count(),
        1,
        "registered once, in the handler of the app's window"
    );
}
