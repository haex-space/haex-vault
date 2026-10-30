use std::time::Duration;

use super::*;
use crate::error::HolziError;

const IDLE: Duration = Duration::from_secs(600);

fn bundle(tag: &str) -> PendingBundle {
    PendingBundle {
        source_id: "preset-public".into(),
        source_name: "Haex Marketplace".into(),
        slug: tag.into(),
        version: "1.0.0".into(),
        bytes: Arc::new(tag.as_bytes().to_vec()),
    }
}

fn missing(error: HolziError) -> bool {
    matches!(error, HolziError::Marketplace { reason } if reason == kinds::PENDING_MISSING)
}

fn put(slot: &Arc<PendingSlot>, tag: &str, closing: &CancellationToken) -> Uuid {
    let id = Uuid::new_v4();
    slot.put(id, bundle(tag), IDLE, closing.clone());
    id
}

#[tokio::test(start_paused = true)]
async fn a_bundle_is_read_by_its_id_and_a_wrong_id_finds_nothing() {
    let slot = Arc::new(PendingSlot::default());
    let id = put(&slot, "a", &CancellationToken::new());

    assert_eq!(slot.peek(&id.to_string()).unwrap().bytes.as_slice(), b"a");
    assert!(missing(slot.peek(&Uuid::new_v4().to_string()).unwrap_err()));
    assert!(missing(slot.peek("not-an-id").unwrap_err()));
}

#[tokio::test(start_paused = true)]
async fn a_second_bundle_replaces_the_first() {
    let slot = Arc::new(PendingSlot::default());
    let closing = CancellationToken::new();
    let first = put(&slot, "a", &closing);
    let second = put(&slot, "b", &closing);

    assert!(missing(slot.peek(&first.to_string()).unwrap_err()));
    assert_eq!(
        slot.peek(&second.to_string()).unwrap().bytes.as_slice(),
        b"b"
    );
}

#[tokio::test(start_paused = true)]
async fn taking_a_bundle_empties_the_slot() {
    let slot = Arc::new(PendingSlot::default());
    let id = put(&slot, "a", &CancellationToken::new());

    assert_eq!(slot.take(&Uuid::new_v4().to_string()), None);
    assert!(!slot.is_empty(), "another id leaves the bundle alone");
    assert_eq!(slot.take(&id.to_string()).unwrap().bytes.as_slice(), b"a");
    assert!(slot.is_empty());
    assert!(missing(slot.peek(&id.to_string()).unwrap_err()));
}

#[tokio::test(start_paused = true)]
async fn an_unused_bundle_expires_after_the_idle_time() {
    let slot = Arc::new(PendingSlot::default());
    let id = put(&slot, "a", &CancellationToken::new());

    tokio::time::sleep(IDLE - Duration::from_secs(1)).await;
    assert!(!slot.is_empty());
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert!(slot.is_empty(), "the bytes are dropped, not only hidden");
    assert!(missing(slot.peek(&id.to_string()).unwrap_err()));
}

#[tokio::test(start_paused = true)]
async fn using_a_bundle_postpones_its_expiry() {
    let slot = Arc::new(PendingSlot::default());
    let id = put(&slot, "a", &CancellationToken::new());

    tokio::time::sleep(IDLE - Duration::from_secs(60)).await;
    slot.peek(&id.to_string()).unwrap();
    tokio::time::sleep(IDLE - Duration::from_secs(60)).await;
    assert!(
        slot.peek(&id.to_string()).is_ok(),
        "ten minutes after the first use, not after the download"
    );

    tokio::time::sleep(IDLE + Duration::from_secs(1)).await;
    assert!(slot.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_closing_vault_clears_the_slot() {
    let slot = Arc::new(PendingSlot::default());
    let closing = CancellationToken::new();
    put(&slot, "a", &closing);

    closing.cancel();
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;

    assert!(slot.is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_expiry_of_a_replaced_bundle_does_not_touch_its_successor() {
    let slot = Arc::new(PendingSlot::default());
    let closing = CancellationToken::new();
    put(&slot, "a", &closing);
    tokio::time::sleep(IDLE - Duration::from_secs(1)).await;
    let second = put(&slot, "b", &closing);

    tokio::time::sleep(Duration::from_secs(2)).await;

    assert_eq!(
        slot.peek(&second.to_string()).unwrap().bytes.as_slice(),
        b"b",
        "the first one's deadline passed, the second one's has not"
    );
}
