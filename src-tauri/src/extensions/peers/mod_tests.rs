//! The errors of channels as the bridge answers them (contracts/bridge.md §Fehler, research R13).

use super::*;

#[test]
fn every_reason_has_its_fixed_code_and_a_limit_where_one_applies() {
    for (error, code, reason) in [
        (
            PeerError::ContactUnknown,
            ExtensionErrorCode::NotFound,
            "contact_unknown",
        ),
        (
            PeerError::InviteNotOpen,
            ExtensionErrorCode::NotFound,
            "invite_not_open",
        ),
        (
            PeerError::ChannelUnknown,
            ExtensionErrorCode::NotFound,
            "channel_unknown",
        ),
        (
            PeerError::LobbyClosed,
            ExtensionErrorCode::NotFound,
            "lobby_closed",
        ),
        (
            PeerError::NotAdmitted,
            ExtensionErrorCode::PermissionDenied,
            "not_admitted",
        ),
        (
            PeerError::ChannelClosed,
            ExtensionErrorCode::NotAvailable,
            "channel_closed",
        ),
        (
            PeerError::NetworkUnavailable,
            ExtensionErrorCode::NotAvailable,
            "network_unavailable",
        ),
        (
            PeerError::InvalidInput,
            ExtensionErrorCode::Validation,
            "invalid_input",
        ),
    ] {
        let answer = BridgeError::from(error);
        assert_eq!(answer.code, code, "{reason}");
        assert_eq!(answer.details, Some(json!({ "reason": reason })));
    }
    for (error, reason, limit) in [
        (
            PeerError::TooLarge {
                limit: limits::FAST_BYTES,
            },
            "too_large",
            1024,
        ),
        (
            PeerError::Limit {
                limit: limits::CHANNELS_PER_EXTENSION,
            },
            "limit",
            16,
        ),
    ] {
        let answer = BridgeError::from(error);
        assert_eq!(answer.code, ExtensionErrorCode::LimitExceeded);
        assert_eq!(
            answer.details,
            Some(json!({ "reason": reason, "limit": limit }))
        );
    }
}

#[test]
fn an_extension_is_named_on_the_wire_as_it_resolves_back() {
    use crate::extensions::peers::test_support::{install, KEY, NAME};
    use crate::sync::test_support::Member;

    let db = Member::genesis().device.vault_db();
    let id = install(&db, NAME);
    let device = db.device_id();
    let (named, resolved) = db
        .read_blocking(move |q| {
            let named = ExtensionRef::of(q, id).map_err(haex_crdt::Error::from)?;
            let resolved = match &named {
                Some(named) => named.resolve(q, device).map_err(haex_crdt::Error::from)?,
                None => None,
            };
            Ok((named, resolved))
        })
        .expect("read");
    assert_eq!(
        named,
        Some(ExtensionRef {
            public_key: KEY.to_owned(),
            name: NAME.to_owned()
        })
    );
    assert_eq!(resolved, Some(id));
    let unknown = db
        .read_blocking(|q| ExtensionRef::of(q, Uuid::new_v4()).map_err(haex_crdt::Error::from))
        .expect("read");
    assert_eq!(unknown, None);
}
