use uuid::Uuid;

use super::*;

const ANNA: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const BEN: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

#[test]
fn both_sides_derive_the_same_direct_conversation() {
    assert_eq!(direct_thread_id(ANNA, BEN), direct_thread_id(BEN, ANNA));
    assert_ne!(direct_thread_id(ANNA, BEN), direct_thread_id(ANNA, ANNA));
}

#[test]
fn derived_ids_never_change() {
    // Pinned values: a change here means old and new devices would write two rows for one thing.
    assert_eq!(
        direct_thread_id(ANNA, BEN),
        Uuid::new_v5(&CHAT_NS, format!("direct:{ANNA}:{BEN}").as_bytes())
    );
    assert_eq!(
        CHAT_NS,
        Uuid::parse_str("1310b1ae-dcb7-4464-9046-12c0c2a2f290").expect("uuid")
    );
}

#[test]
fn ids_of_different_kinds_never_collide() {
    let thread = direct_thread_id(ANNA, BEN);
    let message = message_id(thread, ANNA, "e1");
    let ids = [
        member_id(thread, ANNA),
        member_id(thread, BEN),
        message,
        message_id(thread, BEN, "e1"),
        message_id(thread, ANNA, "e2"),
        attachment_id(message, 0),
        attachment_id(message, 1),
        delivery_id(message, BEN),
        delivery_id(message, ANNA),
    ];
    for (i, a) in ids.iter().enumerate() {
        for b in &ids[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn the_same_message_on_two_devices_is_one_row() {
    let thread = direct_thread_id(ANNA, BEN);
    assert_eq!(message_id(thread, BEN, "e1"), message_id(thread, BEN, "e1"));
}
