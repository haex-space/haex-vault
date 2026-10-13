use uuid::Uuid;

use super::{
    state::{TaskOwner, TaskRegistry, TaskRunStatus, MAX_ACTIVE_TASKS_PER_FRAME},
    TaskErrorCode,
};

fn owner() -> TaskOwner {
    TaskOwner {
        extension_id: Uuid::new_v4(),
        frame: "frame-1".to_owned(),
    }
}

#[test]
fn limits_active_tasks_per_frame() {
    let registry = TaskRegistry::default();
    let owner = owner();
    for _ in 0..MAX_ACTIVE_TASKS_PER_FRAME {
        registry.start(owner.clone()).expect("within frame limit");
    }
    let error = registry.start(owner).expect_err("limit must be enforced");
    assert_eq!(error.code, TaskErrorCode::LimitExceeded);
}

#[test]
fn cancellation_is_owner_scoped_and_idempotent() {
    let registry = TaskRegistry::default();
    let owner = owner();
    let other = TaskOwner {
        extension_id: Uuid::new_v4(),
        frame: "frame-2".to_owned(),
    };
    let record = registry.start(owner.clone()).unwrap();

    assert!(!registry.cancel(record.id, &other));
    assert!(registry.cancel(record.id, &owner));
    assert_eq!(
        registry.get(record.id).unwrap().status,
        TaskRunStatus::Cancelled
    );
    assert!(!registry.cancel(record.id, &owner));
}

#[test]
fn closing_a_frame_cancels_all_owned_tasks() {
    let registry = TaskRegistry::default();
    let owner = owner();
    let first = registry.start(owner.clone()).unwrap();
    let second = registry.start(owner.clone()).unwrap();

    assert_eq!(registry.cancel_owned_by(&owner), 2);
    assert_eq!(
        registry.get(first.id).unwrap().status,
        TaskRunStatus::Cancelled
    );
    assert_eq!(
        registry.get(second.id).unwrap().status,
        TaskRunStatus::Cancelled
    );
}
