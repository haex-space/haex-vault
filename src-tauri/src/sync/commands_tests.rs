use super::*;
use crate::storage::query;
use crate::sync::device_list::RemovedDevice;
use crate::sync::test_support::Member;

fn status_of(member: &Member) -> ThisDevice {
    query::read(member.device.db(), |r| {
        this_device(r, &member.keys.device_pubkey)
    })
    .expect("read status")
}

#[test]
fn a_device_holding_the_main_role_is_main_and_a_listed_other_is_linked() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    linked.device.pull_from(&main.device);

    assert_eq!(status_of(&main), ThisDevice::Main);
    assert_eq!(status_of(&linked), ThisDevice::Linked);
}

#[test]
fn a_device_the_list_does_not_name_awaits_admission() {
    let main = Member::genesis();
    let stranger = Member::join(&main);

    assert_eq!(status_of(&stranger), ThisDevice::AwaitingAdmission);
}

#[test]
fn a_device_the_list_removes_is_removed() {
    let main = Member::genesis();
    let linked = Member::join(&main);
    main.add(&linked);
    let removed = RemovedDevice {
        device_pubkey: linked.keys.device_pubkey,
        vault_device_uuid: linked.device.db().device_id(),
        limit_hlc: "0/0".to_string(),
        removed_at: 3,
    };
    main.issue_list(|mut list| {
        list.devices
            .retain(|d| d.device_pubkey != removed.device_pubkey);
        list.removed.push(removed);
        list
    });
    linked.device.pull_from(&main.device);

    assert_eq!(status_of(&linked), ThisDevice::Removed);
}

// Raw access to tamper with a stored signature, which the CRDT write path does not offer.
#[allow(clippy::disallowed_methods)]
fn break_every_signature(member: &Member) {
    member
        .device
        .db()
        .with_connection(|conn| {
            conn.execute("UPDATE device_lists SET signature = zeroblob(64)", [])?;
            Ok(())
        })
        .expect("break the signatures");
}

fn unverifiable(member: &Member) -> bool {
    query::read(member.device.db(), |r| device_list_unverifiable(r)).expect("read")
}

#[test]
fn a_vault_with_a_valid_device_list_is_not_unverifiable() {
    let main = Member::genesis();

    assert!(!unverifiable(&main));
}

#[test]
fn a_vault_whose_lists_no_longer_verify_says_so() {
    let main = Member::genesis();
    break_every_signature(&main);

    assert!(unverifiable(&main));
    // The status still answers and the device sees itself as waiting, instead of an error.
    assert_eq!(status_of(&main), ThisDevice::AwaitingAdmission);
}

#[test]
#[allow(clippy::disallowed_methods)]
fn a_vault_without_any_list_is_not_unverifiable() {
    let main = Member::genesis();
    main.device
        .db()
        .with_connection(|conn| {
            conn.execute("DELETE FROM device_lists", [])?;
            Ok(())
        })
        .expect("delete the lists");

    assert!(!unverifiable(&main));
}

#[test]
fn the_identity_is_shown_as_npub_and_hex_of_the_same_key() {
    use nostr::nips::nip19::FromBech32;
    let main = Member::genesis();

    let identity = query::read(main.device.db(), |r| public_identity(r))
        .expect("read")
        .expect("an identity after genesis");

    assert!(identity.npub.starts_with("npub1"));
    assert_eq!(identity.hex, keys::hex(&main.vault));
    let decoded = nostr::key::PublicKey::from_bech32(&identity.npub).expect("a valid npub");
    assert_eq!(decoded.to_bytes(), main.vault);
}

#[test]
fn no_private_key_ever_shows_in_the_identity() {
    let main = Member::genesis();
    let secret = query::read(main.device.db(), |r| keys::vault_secret(r))
        .expect("read")
        .expect("main holds the secret");

    let identity = query::read(main.device.db(), |r| public_identity(r))
        .expect("read")
        .expect("an identity");
    let json = serde_json::to_string(&identity).expect("json");

    assert!(!json.contains(&keys::hex(secret.as_slice())));
}

#[test]
fn servers_are_checked_by_scheme_and_count() {
    let ok = |n: &[&str], i: &[&str]| {
        servers::validate(&servers::ServerConfig {
            nostr_relays: n.iter().map(|s| s.to_string()).collect(),
            iroh_relays: i.iter().map(|s| s.to_string()).collect(),
            disabled: Vec::new(),
        })
    };

    assert!(ok(
        &["wss://relay.example", "ws://127.0.0.1:7777"],
        &["https://iroh.example"]
    )
    .is_ok());
    assert!(ok(&[], &[]).is_ok(), "none added is fine");
    assert!(
        ok(&["https://relay.example"], &[]).is_err(),
        "a Nostr server is ws or wss"
    );
    assert!(
        ok(&[], &["wss://iroh.example"]).is_err(),
        "an iroh server is http or https"
    );
    assert!(ok(&["wss://"], &[]).is_err());
    assert!(ok(&["not a url"], &[]).is_err());
    assert!(ok(&["wss://relay example"], &[]).is_err());
    assert!(ok(&[], &["https://iroh.example:abc"]).is_err());
    assert!(ok(&[], &["https://[::1]:443"]).is_ok());
    assert!(ok(&[], &["https://[::1"]).is_err());
    assert!(
        ok(&["wss://a.example"; 11], &[]).is_err(),
        "at most ten of one kind"
    );
    assert!(ok(&["wss://a.example"; 10], &[]).is_ok());
}

#[test]
fn stored_servers_read_back() {
    let main = Member::genesis();
    let config = servers::ServerConfig {
        nostr_relays: vec!["wss://relay.example".to_string()],
        iroh_relays: vec!["https://iroh.example".to_string()],
        disabled: vec!["wss://nos.lol".to_string()],
    };

    let stored = config.clone();
    main.device
        .db()
        .write(move |tx| servers::write(tx, &stored))
        .expect("write");
    let read = query::read(main.device.db(), |r| servers::read(r)).expect("read");
    assert_eq!(read, config);
    assert!(!read
        .effective_nostr_relays()
        .contains(&"wss://nos.lol".to_string()));
}

#[test]
fn the_defaults_the_settings_show_are_the_ones_in_use() {
    let defaults = sync_servers_defaults();
    assert_eq!(defaults.nostr_relays, servers::default_nostr_relays());
    assert!(!defaults.iroh_relays.is_empty());
    // What is shown must be accepted when typed back in.
    servers::validate(&servers::ServerConfig {
        nostr_relays: defaults.nostr_relays.clone(),
        iroh_relays: defaults.iroh_relays.clone(),
        disabled: Vec::new(),
    })
    .expect("valid");
    assert!(defaults
        .iroh_relays
        .iter()
        .all(|url| url.starts_with("https://") && !url.ends_with(['/', '.'])));
}

#[test]
fn a_join_uses_the_built_in_servers_besides_the_ones_it_is_given_unless_switched_off() {
    use crate::sync::link::join_task::JoinConfig;
    use crate::sync::servers::{default_nostr_relays, ServerConfig};

    let mut besides = default_nostr_relays();
    besides.push("wss://nostr.example.org".to_string());
    let own = JoinConfig::with_servers(ServerConfig {
        nostr_relays: vec!["wss://nostr.example.org".to_string()],
        ..ServerConfig::default()
    });
    assert_eq!(own.nostr_relays, besides);

    let only = JoinConfig::with_servers(ServerConfig::only(
        vec!["wss://nostr.example.org".to_string()],
        Vec::new(),
    ));
    assert_eq!(
        only.nostr_relays,
        vec!["wss://nostr.example.org".to_string()]
    );

    let empty = JoinConfig::with_servers(ServerConfig::default());
    assert_eq!(empty.nostr_relays, default_nostr_relays());
    assert_eq!(
        JoinConfig::production().nostr_relays,
        default_nostr_relays()
    );
}
