use serde_json::json;
use uuid::Uuid;

use super::{ToolAvailability, ToolAvailabilityEvent};
use crate::model_capabilities::{ModelCapabilities, ToolSupport, ToolUse, ToolUseBasis};
use crate::storage::providers::ProviderKind;

fn caps(support: Option<ToolSupport>) -> ModelCapabilities {
    ModelCapabilities {
        tool_use: support.map(|s| ToolUse::new(s, ToolUseBasis::Provider)),
        ..ModelCapabilities::default()
    }
}

#[test]
fn the_state_follows_the_provider_kind_and_the_capability() {
    use ToolAvailability::*;
    let cases = [
        (ProviderKind::Local, Some(ToolSupport::Supported), Offered),
        (
            ProviderKind::Local,
            Some(ToolSupport::Unsupported),
            Unsupported,
        ),
        (ProviderKind::Local, None, OfferedUnverified),
        (ProviderKind::ApiKey, Some(ToolSupport::Supported), Offered),
        (ProviderKind::ApiKey, None, OfferedUnverified),
        (
            ProviderKind::CliDelegate,
            Some(ToolSupport::Supported),
            Delegate,
        ),
        (ProviderKind::CliDelegate, None, Delegate),
    ];
    for (kind, support, expected) in cases {
        assert_eq!(
            ToolAvailability::of(kind, Some(&caps(support))),
            expected,
            "{kind:?} {support:?}"
        );
    }
}

#[test]
fn a_model_with_no_record_at_all_is_unverified_not_unsupported() {
    assert_eq!(
        ToolAvailability::of(ProviderKind::Local, None),
        ToolAvailability::OfferedUnverified
    );
}

#[test]
fn only_a_state_that_offers_tools_hands_them_over() {
    assert!(ToolAvailability::Offered.offers_tools());
    assert!(ToolAvailability::OfferedUnverified.offers_tools());
    assert!(!ToolAvailability::Unsupported.offers_tools());
    assert!(!ToolAvailability::Delegate.offers_tools());
}

#[test]
fn the_event_has_the_documented_wire_shape() {
    let thread_id = Uuid::new_v4();
    let value = serde_json::to_value(ToolAvailabilityEvent {
        thread_id,
        state: ToolAvailability::OfferedUnverified,
        delegate_kind: None,
    })
    .unwrap();
    assert_eq!(
        value,
        json!({ "threadId": thread_id.to_string(), "state": "offeredUnverified" })
    );
}

/// Spec 021 US6: a delegate turn names its delegate, so the chat can ask for its MCP rights.
#[test]
fn a_delegate_turn_names_its_delegate() {
    let thread_id = Uuid::new_v4();
    let value = serde_json::to_value(ToolAvailabilityEvent {
        thread_id,
        state: ToolAvailability::Delegate,
        delegate_kind: Some(crate::mcp_server::registry::AgentKind::CodexDelegate),
    })
    .unwrap();
    assert_eq!(
        value,
        json!({
            "threadId": thread_id.to_string(),
            "state": "delegate",
            "delegateKind": "codexDelegate",
        })
    );
}
