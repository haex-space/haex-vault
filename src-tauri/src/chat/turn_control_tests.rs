//! Spec 068 T005/T006: starting and ending turns on the chat state.

use std::collections::HashSet;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::TurnPermit;
use crate::chat::choices::ChoiceAnswer;
use crate::chat::session::ChatState;
use crate::chat::tools::ApprovalDecision;

fn begin(chat: &ChatState, thread: Uuid) -> (Uuid, CancellationToken, TurnPermit) {
    let (turn_id, cancel) = (Uuid::new_v4(), CancellationToken::new());
    let permit = chat
        .begin_turn(turn_id, Some(thread), cancel.clone())
        .expect("the turn gets its slot");
    (turn_id, cancel, permit)
}

fn open_approval(chat: &ChatState, turn_id: Uuid) -> (Uuid, oneshot::Receiver<ApprovalDecision>) {
    let id = Uuid::new_v4();
    let (sender, receiver) = oneshot::channel();
    chat.pending_tool_approvals
        .lock()
        .unwrap()
        .insert(id, sender);
    chat.turns.own_prompt(turn_id, id);
    (id, receiver)
}

#[tokio::test]
async fn turns_of_two_conversations_hold_the_lock_side_by_side() {
    let chat = ChatState::new();
    let (_a, _ca, _pa) = begin(&chat, Uuid::new_v4());
    let (_b, _cb, _pb) = begin(&chat, Uuid::new_v4());
    assert_eq!(chat.turns.running().len(), 2);
    assert!(
        !chat.operation_busy(),
        "turns do not make the chat busy for a send to another conversation"
    );
}

#[tokio::test]
async fn a_load_or_vault_transition_waits_for_every_turn() {
    let chat = ChatState::new();
    let (_a, _ca, pa) = begin(&chat, Uuid::new_v4());
    let (_b, _cb, pb) = begin(&chat, Uuid::new_v4());
    assert!(chat.acquire_operation().is_err());
    drop(pa);
    assert!(
        chat.acquire_operation().is_err(),
        "one turn is still running"
    );
    drop(pb);
    assert!(chat.acquire_operation().is_ok());
}

#[tokio::test]
async fn a_turn_is_refused_while_an_exclusive_operation_runs() {
    let chat = ChatState::new();
    let _load = chat.acquire_operation().unwrap();
    assert!(chat
        .begin_turn(
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            CancellationToken::new()
        )
        .is_err());
    assert!(chat.operation_busy());
    assert!(chat.busy_for(None));
}

#[tokio::test]
async fn a_conversation_is_busy_only_for_its_own_turn() {
    let chat = ChatState::new();
    let (busy_thread, other_thread) = (Uuid::new_v4(), Uuid::new_v4());
    let (_turn, _cancel, _permit) = begin(&chat, busy_thread);
    assert!(chat.busy_for(Some(busy_thread)));
    assert!(!chat.busy_for(Some(other_thread)));
    assert!(!chat.busy_for(None), "a new conversation does not wait");
}

#[tokio::test]
async fn a_retried_send_of_a_running_turn_leaves_the_live_token_alone() {
    let chat = ChatState::new();
    let thread = Uuid::new_v4();
    let (turn_id, live, _permit) = begin(&chat, thread);
    let replay = CancellationToken::new();
    assert!(chat
        .begin_turn(turn_id, Some(thread), replay.clone())
        .is_err());
    assert!(!live.is_cancelled());
    chat.abort_turn_by_id(turn_id);
    assert!(live.is_cancelled(), "the slot still holds the live token");
    assert!(
        !replay.is_cancelled(),
        "the refused send's token was never registered"
    );
}

#[tokio::test]
async fn aborting_one_turn_cancels_only_it_and_its_prompts() {
    let chat = ChatState::new();
    let (a, cancel_a, _pa) = begin(&chat, Uuid::new_v4());
    let (b, cancel_b, _pb) = begin(&chat, Uuid::new_v4());
    let (id_a, mut rx_a) = open_approval(&chat, a);
    let (id_b, mut rx_b) = open_approval(&chat, b);
    let (choice_b, mut choice_rx_b) = chat.pending_choices.open();
    chat.turns.own_prompt(b, choice_b);
    let (choice_a, mut choice_rx_a) = chat.pending_choices.open();
    chat.turns.own_prompt(a, choice_a);

    assert!(chat.abort_turn_by_id(a));

    assert!(cancel_a.is_cancelled());
    assert!(!cancel_b.is_cancelled(), "the other turn goes on");
    // A's approval and question were dropped; B's are still open.
    assert!(matches!(
        rx_a.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert!(matches!(
        choice_rx_a.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert!(matches!(
        rx_b.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    assert!(matches!(
        choice_rx_b.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    assert!(chat
        .pending_tool_approvals
        .lock()
        .unwrap()
        .contains_key(&id_b));
    assert!(chat
        .cancelled_tool_approvals
        .lock()
        .unwrap()
        .contains(&id_a));
    assert!(!chat
        .cancelled_tool_approvals
        .lock()
        .unwrap()
        .contains(&id_b));
    // B's question can still be answered.
    chat.pending_choices
        .resolve(choice_b, ChoiceAnswer::Cancel)
        .expect("B's question is open");
}

#[tokio::test]
async fn an_abort_by_conversation_finds_the_turn_of_that_conversation() {
    let chat = ChatState::new();
    let (thread_a, thread_b) = (Uuid::new_v4(), Uuid::new_v4());
    let (_a, cancel_a, _pa) = begin(&chat, thread_a);
    let (_b, cancel_b, _pb) = begin(&chat, thread_b);
    assert!(chat.abort_turn_of_thread(thread_a));
    assert!(cancel_a.is_cancelled());
    assert!(!cancel_b.is_cancelled());
    assert!(
        !chat.abort_turn_of_thread(Uuid::new_v4()),
        "a conversation with no turn is not an error"
    );
}

#[tokio::test]
async fn aborting_every_turn_drops_even_prompts_no_slot_owns() {
    let chat = ChatState::new();
    let (_a, cancel_a, _pa) = begin(&chat, Uuid::new_v4());
    let (_b, cancel_b, _pb) = begin(&chat, Uuid::new_v4());
    let orphan = Uuid::new_v4();
    let (sender, mut receiver) = oneshot::channel();
    chat.pending_tool_approvals
        .lock()
        .unwrap()
        .insert(orphan, sender);

    chat.abort_all_turns();

    assert!(cancel_a.is_cancelled() && cancel_b.is_cancelled());
    assert!(matches!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert!(chat
        .cancelled_tool_approvals
        .lock()
        .unwrap()
        .contains(&orphan));
}

#[tokio::test]
async fn a_stopped_turn_still_counts_until_it_has_unwound() {
    let chat = ChatState::new();
    let thread = Uuid::new_v4();
    let (turn_id, _cancel, permit) = begin(&chat, thread);
    chat.abort_turn_by_id(turn_id);
    assert!(
        chat.busy_for(Some(thread)),
        "persistence and clean-up are still running"
    );
    assert!(chat.acquire_operation().is_err());
    drop(permit);
    assert!(!chat.busy_for(Some(thread)));
    assert!(chat.acquire_operation().is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_send_waits_for_its_own_conversation_but_not_for_another() {
    let chat = std::sync::Arc::new(ChatState::new());
    let (thread, other) = (Uuid::new_v4(), Uuid::new_v4());
    let (_turn, _cancel, permit) = begin(&chat, thread);
    let release = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        drop(permit);
    });

    let started = tokio::time::Instant::now();
    chat.wait_until_free_for(Some(other), Duration::from_secs(10))
        .await;
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "another conversation's turn is no reason to wait"
    );

    chat.wait_until_free_for(Some(thread), Duration::from_secs(10))
        .await;
    assert!(!chat.busy_for(Some(thread)));
    assert!(started.elapsed() >= Duration::from_secs(2));
    release.await.unwrap();
}

#[tokio::test]
async fn the_close_forgets_every_slot_and_nothing_is_left_open() {
    let chat = ChatState::new();
    let (turn_id, _cancel, _permit) = begin(&chat, Uuid::new_v4());
    let (_id, _receiver) = open_approval(&chat, turn_id);
    chat.abort_all_turns();
    chat.reset_for_close();
    assert!(!chat.turns.any_running());
    assert!(chat.pending_tool_approvals.lock().unwrap().is_empty());
    assert_eq!(
        chat.turns
            .running()
            .into_iter()
            .map(|t| t.turn_id)
            .collect::<HashSet<_>>(),
        HashSet::new()
    );
}
