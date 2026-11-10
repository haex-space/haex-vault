//! Unit tests for the tool-column validation rules (data-model.md). Pure
//! function, no I/O — round-tripping actual rows through a real
//! `chat_messages` table needs `current_hlc()`, which only exists on an
//! open `haex_crdt::Database`; that coverage lives in the integration
//! suite (`tests/chat_tool_loop.rs`) alongside the turn-loop tests that
//! already assert persisted `tool_call`/`tool_result` rows.

use uuid::Uuid;

use super::chat_messages::{validate, ChatMessage, ChatMessageValidationError, MessageRole};

fn base(role: MessageRole) -> ChatMessage {
    ChatMessage {
        id: Uuid::new_v4(),
        thread_id: Uuid::new_v4(),
        parent_id: None,
        role,
        content: String::new(),
        provider_id: None,
        model_id: None,
        prompt_tokens: None,
        completion_tokens: None,
        finish_reason: None,
        created_at: 0,
        idempotency_key: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_is_error: None,
        tool_source: None,
        autonomy_mode: None,
        tool_origin: None,
    }
}

#[test]
fn a_valid_tool_call_row_passes() {
    let m = ChatMessage {
        tool_name: Some("cli".to_string()),
        tool_call_id: Some("call-1".to_string()),
        tool_input: Some("{}".to_string()),
        tool_source: Some("cli".to_string()),
        ..base(MessageRole::ToolCall)
    };
    assert_eq!(validate(&m), Ok(()));
}

#[test]
fn a_tool_call_row_missing_any_required_field_is_rejected() {
    let complete = ChatMessage {
        tool_name: Some("cli".to_string()),
        tool_call_id: Some("call-1".to_string()),
        tool_input: Some("{}".to_string()),
        tool_source: Some("cli".to_string()),
        ..base(MessageRole::ToolCall)
    };

    let missing_name = ChatMessage {
        tool_name: None,
        ..complete.clone()
    };
    assert_eq!(
        validate(&missing_name),
        Err(ChatMessageValidationError::ToolCallMissingFields)
    );

    let missing_call_id = ChatMessage {
        tool_call_id: None,
        ..complete.clone()
    };
    assert_eq!(
        validate(&missing_call_id),
        Err(ChatMessageValidationError::ToolCallMissingFields)
    );

    let missing_input = ChatMessage {
        tool_input: None,
        ..complete.clone()
    };
    assert_eq!(
        validate(&missing_input),
        Err(ChatMessageValidationError::ToolCallMissingFields)
    );

    let missing_source = ChatMessage {
        tool_source: None,
        ..complete
    };
    assert_eq!(
        validate(&missing_source),
        Err(ChatMessageValidationError::ToolCallMissingFields)
    );
}

#[test]
fn a_tool_call_row_must_not_carry_tool_is_error() {
    let m = ChatMessage {
        tool_name: Some("cli".to_string()),
        tool_call_id: Some("call-1".to_string()),
        tool_input: Some("{}".to_string()),
        tool_source: Some("cli".to_string()),
        tool_is_error: Some(false),
        ..base(MessageRole::ToolCall)
    };
    assert_eq!(
        validate(&m),
        Err(ChatMessageValidationError::ToolCallHasToolIsError)
    );
}

#[test]
fn a_valid_tool_result_row_passes() {
    let m = ChatMessage {
        tool_call_id: Some("call-1".to_string()),
        tool_is_error: Some(false),
        content: "42".to_string(),
        ..base(MessageRole::ToolResult)
    };
    assert_eq!(validate(&m), Ok(()));
}

#[test]
fn a_tool_result_row_missing_call_id_or_is_error_is_rejected() {
    let missing_call_id = ChatMessage {
        tool_is_error: Some(false),
        ..base(MessageRole::ToolResult)
    };
    assert_eq!(
        validate(&missing_call_id),
        Err(ChatMessageValidationError::ToolResultMissingFields)
    );

    let missing_is_error = ChatMessage {
        tool_call_id: Some("call-1".to_string()),
        ..base(MessageRole::ToolResult)
    };
    assert_eq!(
        validate(&missing_is_error),
        Err(ChatMessageValidationError::ToolResultMissingFields)
    );
}

#[test]
fn a_tool_result_row_must_not_carry_tool_call_fields() {
    let with_name = ChatMessage {
        tool_call_id: Some("call-1".to_string()),
        tool_is_error: Some(false),
        tool_name: Some("cli".to_string()),
        ..base(MessageRole::ToolResult)
    };
    assert_eq!(
        validate(&with_name),
        Err(ChatMessageValidationError::ToolResultHasToolCallFields)
    );

    let with_input = ChatMessage {
        tool_call_id: Some("call-1".to_string()),
        tool_is_error: Some(false),
        tool_input: Some("{}".to_string()),
        ..base(MessageRole::ToolResult)
    };
    assert_eq!(
        validate(&with_input),
        Err(ChatMessageValidationError::ToolResultHasToolCallFields)
    );

    let with_source = ChatMessage {
        tool_call_id: Some("call-1".to_string()),
        tool_is_error: Some(false),
        tool_source: Some("cli".to_string()),
        ..base(MessageRole::ToolResult)
    };
    assert_eq!(
        validate(&with_source),
        Err(ChatMessageValidationError::ToolResultHasToolCallFields)
    );
}

#[test]
fn user_assistant_and_system_rows_reject_any_tool_column() {
    for role in [
        MessageRole::User,
        MessageRole::Assistant,
        MessageRole::System,
    ] {
        let clean = base(role);
        assert_eq!(validate(&clean), Ok(()), "clean {role:?} row must pass");

        let tainted = ChatMessage {
            tool_call_id: Some("call-1".to_string()),
            ..base(role)
        };
        assert_eq!(
            validate(&tainted),
            Err(ChatMessageValidationError::ToolColumnsOnNonToolRole { role })
        );
    }
}

#[test]
fn a_pre_migration_row_with_every_new_field_none_is_valid() {
    // Rows inserted before migration 0014 read back with all five new
    // columns `None` — validate() must accept that shape unchanged for
    // every pre-existing role.
    for role in [
        MessageRole::User,
        MessageRole::Assistant,
        MessageRole::System,
    ] {
        assert_eq!(validate(&base(role)), Ok(()));
    }
}

/// Spec 018: an extension's tool call keeps its origin on the call row only.
#[test]
fn only_a_tool_call_row_carries_the_tool_origin() {
    let origin = Some(r#"{"extensionId":"e","extensionName":"Mail"}"#.to_string());
    let call = ChatMessage {
        tool_name: Some("x_mail_count".to_string()),
        tool_call_id: Some("call-1".to_string()),
        tool_input: Some("{}".to_string()),
        tool_source: Some("haextension".to_string()),
        tool_origin: origin.clone(),
        ..base(MessageRole::ToolCall)
    };
    assert_eq!(validate(&call), Ok(()));

    let result = ChatMessage {
        tool_call_id: Some("call-1".to_string()),
        tool_is_error: Some(false),
        tool_origin: origin.clone(),
        ..base(MessageRole::ToolResult)
    };
    assert_eq!(
        validate(&result),
        Err(ChatMessageValidationError::ToolResultHasToolCallFields)
    );

    let assistant = ChatMessage {
        tool_origin: origin,
        ..base(MessageRole::Assistant)
    };
    assert_eq!(
        validate(&assistant),
        Err(ChatMessageValidationError::ToolColumnsOnNonToolRole {
            role: MessageRole::Assistant
        })
    );
}

#[test]
fn a_summary_row_names_the_last_row_it_covers_and_carries_no_tool_column() {
    let covered = base(MessageRole::Assistant);
    let summary = ChatMessage {
        parent_id: Some(covered.id),
        content: "older messages, summarised".to_owned(),
        ..base(MessageRole::Summary)
    };
    assert_eq!(validate(&summary), Ok(()));
    assert_eq!(
        validate(&base(MessageRole::Summary)),
        Err(ChatMessageValidationError::SummaryWithoutCoveredRow)
    );
    let with_tool_column = ChatMessage {
        tool_name: Some("x".to_owned()),
        ..summary
    };
    assert_eq!(
        validate(&with_tool_column),
        Err(ChatMessageValidationError::ToolColumnsOnNonToolRole {
            role: MessageRole::Summary
        })
    );
}

/// A summary row goes into a real thread directly after the row it covers, even when that row is not
/// the last one, and the rows after it keep their order.
#[test]
fn a_summary_row_sorts_directly_after_the_row_it_covers() {
    use super::chat_messages::{insert_message, list_messages};
    use crate::remote_storage::test_support::vault;

    let (_dir, db) = vault();
    let thread = Uuid::new_v4();
    let at = |role, created_at| ChatMessage {
        thread_id: thread,
        content: format!("{created_at}"),
        created_at,
        ..base(role)
    };
    let rows = [
        at(MessageRole::User, 100),
        at(MessageRole::Assistant, 200),
        at(MessageRole::User, 300),
        at(MessageRole::Assistant, 400),
    ];
    for row in rows.clone() {
        db.write_blocking(move |tx| insert_message(tx, &row))
            .unwrap();
    }
    let summary = ChatMessage {
        thread_id: thread,
        parent_id: Some(rows[1].id),
        created_at: rows[1].created_at,
        content: "summary".to_owned(),
        ..base(MessageRole::Summary)
    };
    db.write_blocking(move |tx| insert_message(tx, &summary))
        .unwrap();

    let listed = db.read_blocking(move |q| list_messages(q, thread)).unwrap();
    let order: Vec<&str> = listed.iter().map(|row| row.content.as_str()).collect();
    assert_eq!(order, ["100", "200", "summary", "300", "400"]);
    assert_eq!(listed[2].role, MessageRole::Summary);
}
