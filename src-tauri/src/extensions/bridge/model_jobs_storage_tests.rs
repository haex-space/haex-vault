//! Nothing of a job is kept (spec 055 T070, FR-028, SC-011): after a completed, a failed and a
//! cancelled job every table of the vault holds what it held before. The permissions are granted
//! before the first look, so the comparison covers them too. No log line of `model_jobs` names
//! the content of a job.

use std::collections::BTreeMap;

use haex_crdt::rusqlite::types::Value as SqlValue;
use serde_json::{json, Value};

use super::dispatch::call;
use super::dispatch::tests::{setup, Setup};
use super::events::FRAME_EVENT;
use super::model_jobs_run_tests::{allow, image_step, job, models, scripted, start, GOOD};
use crate::model_jobs::bridge::COMPLETED_EVENT;
use crate::model_jobs::test_support::{FakeSource, Script};
use crate::storage::query::Query;

/// Every row of every table, as text, by table name.
type Tables = BTreeMap<String, Vec<String>>;

fn tables(s: &Setup) -> Tables {
    s.ctx
        .db
        .read_blocking(|q| {
            let names: Vec<String> = q.query_map(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                &[],
                |r| r.get(0),
            )?;
            let mut all = BTreeMap::new();
            for name in names {
                let mut rows = q.query_map(&format!("SELECT * FROM \"{name}\""), &[], |row| {
                    (0..row.as_ref().column_count())
                        .map(|i| row.get::<_, SqlValue>(i))
                        .collect::<Result<Vec<_>, _>>()
                        .map(|values| format!("{values:?}"))
                })?;
                rows.sort();
                all.insert(name, rows);
            }
            Ok(all)
        })
        .expect("tables")
}

/// The end of the job `id`.
fn end_of(s: &Setup, id: &Value) -> Value {
    s.recorded.wait_for(|(name, payload)| {
        name == FRAME_EVENT && payload["type"] == COMPLETED_EVENT && payload["data"]["jobId"] == *id
    })["data"]
        .clone()
}

/// Runs one image job on a granted local model that does what `script` says, and returns the
/// tables before and after with the job's status. `cancel` cancels the job once it was sent.
fn around(script: Script, cancel: bool) -> (Tables, Tables, Value) {
    let s = setup();
    let (_, rows) = models(&s, "http://localhost:11434/v1", &["vision"], true);
    let source: std::sync::Arc<FakeSource> = scripted(&s, &[(&rows[0].id, script)]);
    allow(&s, "model", "medium", &rows[0].id, "granted");
    let before = tables(&s);
    assert!(
        before
            .values()
            .flatten()
            .any(|row| row.contains(rows[0].id.as_str())),
        "the look covers the grant"
    );

    let id = start(&s, &job(vec![image_step()])).expect("started")["jobId"].clone();
    if cancel {
        source.wait_until_asked(1);
        let answer = call(
            &s.ctx,
            "extension_model_job_cancel",
            &json!({ "jobId": id }),
        )
        .unwrap();
        assert_eq!(answer["cancelled"], true);
    }
    let status = end_of(&s, &id)["status"].clone();
    (before, tables(&s), status)
}

#[test]
fn a_completed_job_leaves_every_table_as_it_was() {
    let (before, after, status) = around(Script::Answer(GOOD.into()), false);
    assert_eq!(status, "completed");
    assert_eq!(before, after);
}

#[test]
fn a_failed_job_leaves_every_table_as_it_was() {
    let (before, after, status) = around(Script::Answer("kein JSON".into()), false);
    assert_eq!(status, "failed");
    assert_eq!(before, after);
}

#[test]
fn a_cancelled_job_leaves_every_table_as_it_was() {
    let (before, after, status) = around(Script::Hang, true);
    assert_eq!(status, "cancelled");
    assert_eq!(before, after);
}

/// A log call in the code of model jobs names none of a job's content.
#[test]
fn no_log_line_of_model_jobs_names_the_content_of_a_job() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = std::fs::read_dir(root.join("model_jobs"))
        .expect("model_jobs")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| !path.to_string_lossy().ends_with("_tests.rs"))
        .collect();
    files.push(root.join("extensions/bridge/model_jobs.rs"));
    let logs = [
        "tracing::",
        "log::",
        "info!(",
        "debug!(",
        "warn!(",
        "error!(",
        "trace!(",
        "println!(",
    ];
    let content = [
        "instruction",
        "image",
        "input",
        "answer",
        "output",
        "text",
        "result",
        "content",
    ];
    for file in files
        .iter()
        .filter(|path| !path.ends_with("test_support.rs"))
    {
        let source = std::fs::read_to_string(file).expect("source");
        for (number, line) in source.lines().enumerate() {
            if logs.iter().any(|call| line.contains(call)) {
                let named = content
                    .iter()
                    .find(|word| line.to_lowercase().contains(*word));
                assert!(
                    named.is_none(),
                    "{}:{} logs {named:?}: {line}",
                    file.display(),
                    number + 1
                );
            }
        }
    }
}
