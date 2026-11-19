use serde_json::json;
use uuid::Uuid;

use super::{ChoiceAnswer, PendingChoices};
use crate::error::HaexError;

#[test]
fn the_three_answers_come_from_the_wire_shape() {
    let answer = |value| serde_json::from_value::<ChoiceAnswer>(value).expect("an answer");
    assert_eq!(
        answer(json!({ "kind": "option", "value": "extension.mail" })),
        ChoiceAnswer::Option {
            value: "extension.mail".into()
        }
    );
    assert_eq!(
        answer(json!({ "kind": "text", "text": "Einstellungen" })),
        ChoiceAnswer::Text {
            text: "Einstellungen".into()
        }
    );
    assert_eq!(answer(json!({ "kind": "cancel" })), ChoiceAnswer::Cancel);
}

#[tokio::test]
async fn an_answer_reaches_the_waiting_round_once() {
    let choices = PendingChoices::default();
    let (id, receiver) = choices.open();
    choices
        .resolve(id, ChoiceAnswer::Cancel)
        .expect("the first answer");
    assert_eq!(receiver.await.expect("the answer"), ChoiceAnswer::Cancel);
    assert!(matches!(
        choices.resolve(id, ChoiceAnswer::Cancel),
        Err(HaexError::InvalidInput { .. })
    ));
}

#[test]
fn an_unknown_question_is_an_input_error() {
    let choices = PendingChoices::default();
    assert!(matches!(
        choices.resolve(Uuid::new_v4(), ChoiceAnswer::Cancel),
        Err(HaexError::InvalidInput { .. })
    ));
}

#[tokio::test]
async fn a_late_answer_to_a_cancelled_question_does_nothing() {
    let choices = PendingChoices::default();
    let (all, all_receiver) = choices.open();
    choices.cancel_all();
    assert!(all_receiver.await.is_err(), "the round sees the drop");
    choices
        .resolve(all, ChoiceAnswer::Cancel)
        .expect("a late answer is no error");

    let (one, _receiver) = choices.open();
    choices.cancel(one);
    choices
        .resolve(one, ChoiceAnswer::Cancel)
        .expect("a late answer is no error");
}

#[test]
fn closing_the_vault_forgets_every_question() {
    let choices = PendingChoices::default();
    let (id, _receiver) = choices.open();
    choices.cancel_all();
    choices.reset();
    assert!(matches!(
        choices.resolve(id, ChoiceAnswer::Cancel),
        Err(HaexError::InvalidInput { .. })
    ));
}
