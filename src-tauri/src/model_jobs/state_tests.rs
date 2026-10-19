//! Tests for the registry of running model jobs (spec 055 FR-022, FR-028, R9).

use std::time::Duration;

use uuid::Uuid;

use super::limits::MAX_ACTIVE_JOBS_PER_FRAME;
use super::state::{JobOwner, JobRegistry, TooManyJobs};

const LONG: Duration = Duration::from_secs(300);

fn owner(frame: &str) -> JobOwner {
    JobOwner {
        extension_id: Uuid::new_v4(),
        frame: frame.to_owned(),
    }
}

#[test]
fn a_frame_runs_at_most_two_jobs() {
    let registry = JobRegistry::default();
    let frame = owner("frame-1");
    for _ in 0..MAX_ACTIVE_JOBS_PER_FRAME {
        registry
            .start(frame.clone(), LONG)
            .expect("within the limit");
    }
    assert_eq!(
        registry.start(frame.clone(), LONG).unwrap_err(),
        TooManyJobs
    );
    // Another frame has its own two.
    registry
        .start(owner("frame-2"), LONG)
        .expect("another frame");
}

#[test]
fn a_finished_job_is_gone_and_frees_its_place() {
    let registry = JobRegistry::default();
    let frame = owner("frame-1");
    let first = registry.start(frame.clone(), LONG).unwrap();
    registry.start(frame.clone(), LONG).unwrap();
    assert!(registry.start(frame.clone(), LONG).is_err());

    assert!(registry.finish(first.id));
    assert!(registry.get(first.id).is_none(), "nothing of it is kept");
    assert!(!registry.finish(first.id), "a second finish finds nothing");
    registry
        .start(frame, LONG)
        .expect("the place is free again");
}

#[test]
fn a_job_is_cancelled_by_its_own_frame_only_and_only_once() {
    let registry = JobRegistry::default();
    let frame = owner("frame-1");
    let other = JobOwner {
        extension_id: frame.extension_id,
        frame: "frame-2".to_owned(),
    };
    let record = registry.start(frame.clone(), LONG).unwrap();

    assert!(
        !registry.cancel(record.id, &other),
        "another frame of the same extension"
    );
    assert!(!record.cancel.is_cancelled());
    assert!(registry.cancel(record.id, &frame));
    assert!(record.cancel.is_cancelled());
    assert!(registry.get(record.id).is_none());
    assert!(
        !registry.cancel(record.id, &frame),
        "an unknown id is not cancelled"
    );
}

#[test]
fn closing_a_frame_cancels_all_its_jobs_and_no_other() {
    let registry = JobRegistry::default();
    let frame = owner("frame-1");
    let first = registry.start(frame.clone(), LONG).unwrap();
    let second = registry.start(frame.clone(), LONG).unwrap();
    let elsewhere = registry.start(owner("frame-2"), LONG).unwrap();

    assert_eq!(registry.cancel_owned_by(&frame), 2);
    assert!(first.cancel.is_cancelled() && second.cancel.is_cancelled());
    assert!(!elsewhere.cancel.is_cancelled());
    assert!(registry.get(first.id).is_none() && registry.get(second.id).is_none());
    assert!(registry.get(elsewhere.id).is_some());
    assert_eq!(registry.cancel_owned_by(&frame), 0);
}

#[test]
fn a_record_holds_no_more_than_what_cancels_and_limits_it() {
    // The record's fields are the whole of what is kept; this fails to compile when one is added
    // without a decision (FR-028: no input, result or provenance in the registry).
    let registry = JobRegistry::default();
    let record = registry.start(owner("frame-1"), LONG).unwrap();
    let super::state::JobRecord {
        id: _,
        owner: _,
        cancel: _,
        deadline: _,
    } = record;
}

#[test]
fn closing_the_vault_cancels_every_job_of_every_frame() {
    let registry = JobRegistry::default();
    let first = registry.start(owner("frame-1"), LONG).unwrap();
    let second = registry.start(owner("frame-2"), LONG).unwrap();

    assert_eq!(registry.cancel_all(), 2);

    assert!(first.cancel.is_cancelled() && second.cancel.is_cancelled());
    assert!(registry.get(first.id).is_none() && registry.get(second.id).is_none());
    assert_eq!(registry.cancel_all(), 0);
}
