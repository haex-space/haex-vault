use std::path::Path;

use serde_json::json;

use super::cdp::Event;
use super::downloads::{
    folder, frames_of, move_into, numbered, report_of, safe_name, upload_path, Report, State, Step,
    Tracker, UploadRefusal, LIMIT,
};
use crate::caller::Caller;
use crate::files::access::{AgentGrants, DeviceGrant};
use crate::files::local::OwnPlaces;

fn event(method: &str, params: serde_json::Value) -> Event {
    Event {
        method: method.to_owned(),
        params,
        session_id: None,
    }
}

fn began(guid: &str) -> Report {
    Report::Began {
        guid: guid.to_owned(),
        name: "Rechnung.pdf".to_owned(),
    }
}

fn progress(guid: &str, total: u64, received: u64, state: State) -> Report {
    Report::Progress {
        guid: guid.to_owned(),
        total,
        received,
        state,
        file: None,
    }
}

#[test]
fn names_keep_no_folder_and_no_forbidden_character() {
    assert_eq!(safe_name("../../etc/passwd"), "passwd");
    assert_eq!(safe_name("C:\\Windows\\win.ini"), "win.ini");
    assert_eq!(safe_name("a<b>:c|d?.pdf"), "a_b__c_d_.pdf");
    assert_eq!(safe_name("line\nbreak.txt"), "line_break.txt");
    assert_eq!(safe_name(" .hidden. "), "hidden");
    assert_eq!(safe_name(""), "download");
    assert_eq!(safe_name("..."), "download");
}

#[test]
fn names_avoid_the_device_names_of_windows_and_stay_short() {
    assert_eq!(safe_name("CON.txt"), "_CON.txt");
    assert_eq!(safe_name("lpt1.tar.gz"), "_lpt1.tar.gz");
    assert_eq!(safe_name("console.txt"), "console.txt");

    let long = format!("{}.pdf", "x".repeat(400));
    let cut = safe_name(&long);
    assert_eq!(cut.chars().count(), 150);
    assert!(cut.ends_with(".pdf"));
}

#[test]
fn a_taken_name_gets_a_number_before_its_extension() {
    assert_eq!(numbered("Rechnung.pdf", 1), "Rechnung.pdf");
    assert_eq!(numbered("Rechnung.pdf", 2), "Rechnung (2).pdf");
    assert_eq!(numbered("README", 3), "README (3)");
}

#[test]
fn each_vault_has_its_own_folder_of_agent_downloads() {
    let downloads = Path::new("/home/anna/Downloads");
    assert_eq!(
        folder(downloads, "Privat"),
        downloads.join("Agenten-Downloads").join("Privat")
    );
    assert_eq!(
        folder(downloads, "../x"),
        downloads.join("Agenten-Downloads").join("x")
    );
}

#[test]
fn a_download_moves_under_a_name_that_is_still_free() {
    let work = tempfile::tempdir().unwrap();
    let target = work.path().join("Agenten-Downloads").join("Privat");
    for (guid, content) in [("g1", "first"), ("g2", "second!")] {
        std::fs::write(work.path().join(guid), content).unwrap();
    }

    let (first, first_size) = move_into(&work.path().join("g1"), &target, "Rechnung.pdf").unwrap();
    let (second, second_size) =
        move_into(&work.path().join("g2"), &target, "Rechnung.pdf").unwrap();

    assert_eq!(first, target.join("Rechnung.pdf"));
    assert_eq!(second, target.join("Rechnung (2).pdf"));
    assert_eq!((first_size, second_size), (5, 7));
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "first");
    assert!(!work.path().join("g1").exists() && !work.path().join("g2").exists());
}

#[test]
fn only_a_download_of_the_tab_counts() {
    let frames = frames_of(&json!({
        "frameTree": {
            "frame": { "id": "main" },
            "childFrames": [{ "frame": { "id": "inner" } }]
        }
    }));
    assert_eq!(frames, ["main", "inner"]);

    let ours = event(
        "Browser.downloadWillBegin",
        json!({ "frameId": "inner", "guid": "g1", "suggestedFilename": "a.pdf" }),
    );
    let other = event(
        "Browser.downloadWillBegin",
        json!({ "frameId": "elsewhere", "guid": "g2", "suggestedFilename": "b.pdf" }),
    );
    assert_eq!(
        report_of(&ours, &frames),
        Some(Report::Began {
            guid: "g1".to_owned(),
            name: "a.pdf".to_owned()
        })
    );
    assert_eq!(report_of(&other, &frames), None);

    let done = event(
        "Browser.downloadProgress",
        json!({ "guid": "g1", "totalBytes": 10.0, "receivedBytes": 10.0, "state": "completed",
                "filePath": "/tmp/work/g1" }),
    );
    assert_eq!(
        report_of(&done, &frames),
        Some(Report::Progress {
            guid: "g1".to_owned(),
            total: 10,
            received: 10,
            state: State::Completed,
            file: Some("/tmp/work/g1".to_owned()),
        })
    );
}

#[test]
fn a_small_download_completes_without_a_question() {
    let mut tracker = Tracker::default();

    assert_eq!(tracker.feed(began("g1")), Step::Began);
    assert_eq!(tracker.feed(began("g2")), Step::Wait);
    assert_eq!(
        tracker.feed(progress("g2", 10, 10, State::Completed)),
        Step::Wait
    );
    assert_eq!(
        tracker.feed(progress("g1", 2000, 1000, State::InProgress)),
        Step::Progress
    );
    assert_eq!(
        tracker.feed(progress("g1", 2000, 2000, State::Completed)),
        Step::Completed
    );
    assert!(tracker.completed && !tracker.asked);
}

#[test]
fn a_large_download_asks_once_by_its_announced_size() {
    let mut tracker = Tracker::default();
    tracker.feed(began("g1"));
    let size = 150 * 1000 * 1000;

    assert_eq!(
        tracker.feed(progress("g1", size, 0, State::InProgress)),
        Step::Ask(Some(size))
    );
    assert_eq!(
        tracker.feed(progress("g1", size, 1, State::InProgress)),
        Step::Progress
    );
    assert_eq!(
        tracker.feed(progress("g1", size, size, State::Completed)),
        Step::Completed
    );
}

#[test]
fn a_download_of_unknown_size_asks_once_it_passes_the_limit() {
    let mut tracker = Tracker::default();
    tracker.feed(began("g1"));

    assert_eq!(
        tracker.feed(progress("g1", 0, LIMIT, State::InProgress)),
        Step::Progress
    );
    assert_eq!(
        tracker.feed(progress("g1", 0, LIMIT + 1, State::Completed)),
        Step::Ask(Some(LIMIT + 1))
    );
    assert!(tracker.completed);
}

#[test]
fn a_download_the_website_ended_fails() {
    let mut tracker = Tracker::default();
    tracker.feed(began("g1"));

    assert_eq!(
        tracker.feed(progress("g1", 0, 5, State::Canceled)),
        Step::Canceled
    );
}

fn external() -> Caller {
    Caller::ExternalAgent {
        id: "a1".to_owned(),
    }
}

fn device_granted() -> AgentGrants {
    AgentGrants {
        device: Some(DeviceGrant::Granted),
        ..AgentGrants::default()
    }
}

#[test]
fn an_upload_without_file_rights_says_the_same_for_any_file() {
    let dir = tempfile::tempdir().unwrap();
    let there = dir.path().join("Lebenslauf.pdf");
    std::fs::write(&there, "pdf").unwrap();
    let missing = dir.path().join("missing.pdf");
    let own = OwnPlaces::new(Vec::new());

    for path in [&there, &missing] {
        assert_eq!(
            upload_path(
                &external(),
                &path.to_string_lossy(),
                &own,
                &AgentGrants::default()
            ),
            Err(UploadRefusal::NotAllowed)
        );
    }
    assert_eq!(
        upload_path(
            &external(),
            &missing.to_string_lossy(),
            &own,
            &device_granted()
        ),
        Err(UploadRefusal::NotAllowed)
    );
}

#[test]
fn an_upload_takes_a_file_the_device_grant_reaches() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Lebenslauf.pdf");
    std::fs::write(&file, "pdf").unwrap();
    let own = OwnPlaces::new(Vec::new());

    let taken = upload_path(
        &external(),
        &file.to_string_lossy(),
        &own,
        &device_granted(),
    );

    assert_eq!(taken, Ok(std::fs::canonicalize(&file).unwrap()));
    assert_eq!(
        upload_path(&external(), "Lebenslauf.pdf", &own, &device_granted()),
        Err(UploadRefusal::Invalid)
    );
    assert_eq!(
        upload_path(
            &external(),
            &dir.path().to_string_lossy(),
            &own,
            &device_granted()
        ),
        Err(UploadRefusal::NotAllowed)
    );
}

#[test]
fn an_upload_never_takes_holzis_own_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("vault.db");
    std::fs::write(&file, "secret").unwrap();
    let own = OwnPlaces::new(vec![std::fs::canonicalize(dir.path()).unwrap()]);

    assert_eq!(
        upload_path(
            &Caller::BuiltinAgent,
            &file.to_string_lossy(),
            &own,
            &device_granted()
        ),
        Err(UploadRefusal::NotAllowed)
    );
}
