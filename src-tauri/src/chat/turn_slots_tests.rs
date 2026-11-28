//! Spec 068 T005: the registry of running turns.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Refused, TurnSlots};

fn slots() -> Arc<TurnSlots> {
    Arc::new(TurnSlots::default())
}

#[test]
fn two_conversations_each_get_a_slot() {
    let slots = slots();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let (thread_a, thread_b) = (Uuid::new_v4(), Uuid::new_v4());
    let _first = slots
        .begin(a, Some(thread_a), CancellationToken::new())
        .expect("first turn");
    let _second = slots
        .begin(b, Some(thread_b), CancellationToken::new())
        .expect("a turn in another conversation runs beside it");
    assert_eq!(slots.running().len(), 2);
    assert_eq!(slots.turn_of_thread(thread_a), Some(a));
    assert_eq!(slots.turn_of_thread(thread_b), Some(b));
}

#[test]
fn a_second_turn_in_one_conversation_is_refused_and_touches_nothing() {
    let slots = slots();
    let thread = Uuid::new_v4();
    let live = CancellationToken::new();
    let _first = slots
        .begin(Uuid::new_v4(), Some(thread), live.clone())
        .expect("first turn");
    let refused = slots.begin(Uuid::new_v4(), Some(thread), CancellationToken::new());
    assert_eq!(refused.err(), Some(Refused::ThreadBusy));
    assert!(!live.is_cancelled(), "the running turn is left alone");
    assert_eq!(slots.running().len(), 1);
}

#[test]
fn a_retried_send_of_a_running_turn_finds_its_id_taken() {
    let slots = slots();
    let id = Uuid::new_v4();
    let _first = slots
        .begin(id, None, CancellationToken::new())
        .expect("first turn");
    let refused = slots.begin(id, None, CancellationToken::new());
    assert_eq!(refused.err(), Some(Refused::AlreadyRunning));
}

#[test]
fn a_new_conversation_is_named_once_it_is_stored() {
    let slots = slots();
    let id = Uuid::new_v4();
    let thread = Uuid::new_v4();
    let _guard = slots
        .begin(id, None, CancellationToken::new())
        .expect("first turn");
    assert!(!slots.thread_busy(thread));
    slots.set_thread(id, thread);
    assert!(slots.thread_busy(thread));
    assert!(
        slots
            .begin(Uuid::new_v4(), Some(thread), CancellationToken::new())
            .is_err(),
        "the named conversation is busy now"
    );
}

#[test]
fn dropping_the_guard_frees_the_slot_and_the_conversation() {
    let slots = slots();
    let thread = Uuid::new_v4();
    let guard = slots
        .begin(Uuid::new_v4(), Some(thread), CancellationToken::new())
        .expect("first turn");
    assert!(slots.thread_busy(thread));
    drop(guard);
    assert!(!slots.thread_busy(thread));
    assert!(!slots.any_running());
}

#[test]
fn aborting_one_slot_leaves_the_others_running() {
    let slots = slots();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let (token_a, token_b) = (CancellationToken::new(), CancellationToken::new());
    let _ga = slots
        .begin(a, Some(Uuid::new_v4()), token_a.clone())
        .expect("a");
    let _gb = slots
        .begin(b, Some(Uuid::new_v4()), token_b.clone())
        .expect("b");

    let taken = slots.take_for_abort(a).expect("a is running");
    taken.cancel.cancel();

    assert!(token_a.is_cancelled());
    assert!(!token_b.is_cancelled(), "the other turn is not cancelled");
    assert_eq!(
        slots.running().len(),
        2,
        "the stopped turn counts as running until its guard drops"
    );
}

#[test]
fn an_abort_hands_out_only_the_prompts_of_its_own_turn() {
    let slots = slots();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let _ga = slots
        .begin(a, Some(Uuid::new_v4()), CancellationToken::new())
        .expect("a");
    let _gb = slots
        .begin(b, Some(Uuid::new_v4()), CancellationToken::new())
        .expect("b");
    let (prompt_a, prompt_b) = (Uuid::new_v4(), Uuid::new_v4());
    slots.own_prompt(a, prompt_a);
    slots.own_prompt(b, prompt_b);

    let taken = slots.take_for_abort(a).expect("a is running");
    assert!(taken.open_prompts.contains(&prompt_a));
    assert!(!taken.open_prompts.contains(&prompt_b));
    // What was handed out is gone from the slot: a second abort finds nothing left to drop.
    assert!(slots
        .take_for_abort(a)
        .expect("still there")
        .open_prompts
        .is_empty());
}

#[test]
fn a_delegate_prompt_is_owned_through_its_conversation() {
    let slots = slots();
    let thread = Uuid::new_v4();
    let id = Uuid::new_v4();
    let _guard = slots
        .begin(id, Some(thread), CancellationToken::new())
        .expect("turn");
    let prompt = Uuid::new_v4();
    slots.own_prompt_of_thread(thread, prompt);
    // A prompt of a conversation that runs nothing has no owner and is not recorded anywhere.
    slots.own_prompt_of_thread(Uuid::new_v4(), Uuid::new_v4());
    let taken = slots.take_for_abort(id).expect("running");
    assert_eq!(taken.open_prompts.len(), 1);
    assert!(taken.open_prompts.contains(&prompt));
}

#[test]
fn an_answered_prompt_is_no_longer_dropped_by_an_abort() {
    let slots = slots();
    let id = Uuid::new_v4();
    let _guard = slots
        .begin(id, None, CancellationToken::new())
        .expect("turn");
    let prompt = Uuid::new_v4();
    slots.own_prompt(id, prompt);
    slots.disown_prompt(id, prompt);
    assert!(slots
        .take_for_abort(id)
        .expect("running")
        .open_prompts
        .is_empty());
}

#[test]
fn aborting_an_unknown_turn_is_a_no_op() {
    assert!(slots().take_for_abort(Uuid::new_v4()).is_none());
}

#[test]
fn clearing_forgets_every_slot_and_a_late_guard_drop_is_harmless() {
    let slots = slots();
    let guard = slots
        .begin(
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            CancellationToken::new(),
        )
        .expect("turn");
    slots.clear();
    assert!(!slots.any_running());
    drop(guard);
    assert!(!slots.any_running());
}
