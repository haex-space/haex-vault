use super::*;
use crate::sync::device_list::{DeviceList, ListedDevice};

fn listed(tag: u8, role: Role) -> ListedDevice {
    ListedDevice {
        device_pubkey: [tag; 32],
        endpoint_id: [tag; 32],
        role,
        vault_device_uuid: Uuid::from_bytes([tag; 16]),
        name_sealed: Vec::new(),
        added_at: 0,
    }
}

fn list(devices: Vec<ListedDevice>) -> SignedList {
    SignedList {
        hash: [0; 32],
        payload: Vec::new(),
        signature: [0; 64],
        list: DeviceList {
            vault: [0; 32],
            generation: 1,
            devices,
            removed: Vec::new(),
            issued_by: [0; 32],
            issued_at: 0,
            base_list_hash: None,
        },
    }
}

const OWN_SCHEMA: SchemaVersion = SchemaVersion {
    protocol: 2,
    holzi_migration: 40,
    crdt_trigger: 27,
};

fn known(tag: u8, alias: Option<&str>) -> KnownDevice {
    KnownDevice {
        installation_uuid: Uuid::from_bytes([tag + 100; 16]),
        vault_device_uuid: Uuid::from_bytes([tag; 16]),
        alias: alias.map(str::to_owned),
    }
}

fn build_with(
    signed: &SignedList,
    known: &[KnownDevice],
    facts: &HashMap<[u8; 32], Facts>,
    connected: &HashSet<[u8; 32]>,
    own: u8,
) -> Vec<VaultDevice> {
    build(&Inputs {
        list: Some(signed),
        known,
        facts,
        connected,
        own_device: [own; 32],
        own_schema: OWN_SCHEMA,
        key: None,
    })
}

#[test]
fn roles_come_from_the_list_and_this_device_is_marked_first() {
    let signed = list(vec![
        listed(2, Role::Linked),
        listed(1, Role::Main),
        listed(3, Role::Linked),
    ]);
    let names = [
        known(1, Some("Laptop")),
        known(2, Some("desktop")),
        known(3, Some("Alpha")),
    ];

    let devices = build_with(&signed, &names, &HashMap::new(), &HashSet::new(), 2);

    let order: Vec<_> = devices.iter().map(|d| d.alias.as_deref()).collect();
    assert_eq!(order, [Some("desktop"), Some("Alpha"), Some("Laptop")]);
    assert!(devices[0].is_current);
    assert_eq!(devices[0].role, DeviceRole::Linked);
    assert_eq!(devices[2].role, DeviceRole::Main);
    assert_eq!(devices.iter().filter(|d| d.is_current).count(), 1);
}

#[test]
fn a_connected_device_is_online_and_others_show_when_they_were_last_seen() {
    let signed = list(vec![
        listed(1, Role::Main),
        listed(2, Role::Linked),
        listed(3, Role::Linked),
    ]);
    let facts = HashMap::from([(
        [2; 32],
        Facts {
            last_seen_ms: 5_000,
            problem: None,
            schema: None,
        },
    )]);
    let connected = HashSet::from([[3u8; 32]]);

    let devices = build_with(&signed, &[], &facts, &connected, 1);

    let by = |tag: u8| {
        devices
            .iter()
            .find(|d| d.device_pubkey == hex(&[tag; 32]))
            .expect("listed")
    };
    assert!(by(1).online, "this device is always online to itself");
    assert!(by(3).online);
    assert!(!by(2).online);
    assert_eq!(by(2).last_seen, Some(5_000));
    assert_eq!(by(3).last_seen, None, "never seen, though connected now");
}

#[test]
fn a_device_no_time_is_known_of_has_no_last_seen() {
    let signed = list(vec![listed(1, Role::Main), listed(2, Role::Linked)]);

    let devices = build_with(&signed, &[], &HashMap::new(), &HashSet::new(), 1);

    assert_eq!(devices[1].last_seen, None);
    assert!(!devices[1].online);
}

#[test]
fn a_halted_device_shows_why() {
    let signed = list(vec![listed(1, Role::Main), listed(2, Role::Linked)]);
    let facts = HashMap::from([(
        [2; 32],
        Facts {
            last_seen_ms: 0,
            problem: Some(Problem::IncompatibleVersion),
            schema: None,
        },
    )]);

    let devices = build_with(&signed, &[], &facts, &HashSet::new(), 1);

    assert_eq!(devices[1].problem, Some(DeviceProblem::IncompatibleVersion));
    assert_eq!(devices[0].problem, None);
}

#[test]
fn a_device_off_the_list_is_not_shown() {
    let signed = list(vec![listed(1, Role::Main)]);
    let names = [known(1, Some("Laptop")), known(2, Some("Removed"))];

    let devices = build_with(&signed, &names, &HashMap::new(), &HashSet::new(), 1);

    assert_eq!(devices.len(), 1);
}

#[test]
fn a_device_without_an_alias_is_named_from_its_sealed_name_when_the_key_opens_it() {
    let key = ContentKey::generate(1);
    let mut device = listed(2, Role::Linked);
    device.name_sealed = content_keys::seal_name(&key, &device.device_pubkey, "Work laptop");
    let signed = list(vec![listed(1, Role::Main), device]);

    let devices = build(&Inputs {
        list: Some(&signed),
        known: &[],
        facts: &HashMap::new(),
        connected: &HashSet::new(),
        own_device: [1; 32],
        own_schema: OWN_SCHEMA,
        key: Some(&key),
    });

    assert_eq!(devices[1].alias.as_deref(), Some("Work laptop"));
    assert_eq!(devices[0].alias, None, "this device sealed no name here");
}

#[test]
fn an_empty_alias_counts_as_no_name_and_sorts_last() {
    let signed = list(vec![
        listed(1, Role::Main),
        listed(2, Role::Linked),
        listed(3, Role::Linked),
    ]);
    let names = [known(2, Some("  ")), known(3, Some("Named"))];

    let devices = build_with(&signed, &names, &HashMap::new(), &HashSet::new(), 1);

    let order: Vec<_> = devices.iter().map(|d| d.alias.as_deref()).collect();
    assert_eq!(order, [None, Some("Named"), None]);
    assert!(devices[0].is_current);
}

#[test]
fn it_serializes_camel_case_with_nulls() {
    let signed = list(vec![listed(1, Role::Main)]);

    let devices = build_with(&signed, &[], &HashMap::new(), &HashSet::new(), 1);
    let json = serde_json::to_value(&devices[0]).expect("serialize");

    assert_eq!(
        json,
        serde_json::json!({
            "vaultDeviceUuid": Uuid::from_bytes([1; 16]).to_string(),
            "devicePubkey": hex(&[1; 32]),
            "alias": null,
            "role": "main",
            "isCurrent": true,
            "online": true,
            "lastSeen": null,
            "problem": null,
            "schema": {
                "protocol": OWN_SCHEMA.protocol,
                "holziMigration": OWN_SCHEMA.holzi_migration,
                "crdtTrigger": OWN_SCHEMA.crdt_trigger,
            },
        })
    );
}

fn device_of(devices: &[VaultDevice], tag: u8) -> &VaultDevice {
    devices
        .iter()
        .find(|d| d.device_pubkey == hex(&[tag; 32]))
        .expect("listed")
}

#[test]
fn this_device_shows_its_own_state_and_the_others_the_state_they_showed() {
    let signed = list(vec![
        listed(1, Role::Main),
        listed(2, Role::Linked),
        listed(3, Role::Linked),
    ]);
    let older = SchemaVersion {
        protocol: 2,
        holzi_migration: 39,
        crdt_trigger: 27,
    };
    let facts = HashMap::from([(
        [2; 32],
        Facts {
            last_seen_ms: 1,
            problem: Some(Problem::IncompatibleVersion),
            schema: Some(older),
        },
    )]);

    let devices = build_with(&signed, &[], &facts, &HashSet::new(), 1);

    assert_eq!(device_of(&devices, 1).schema, Some(OWN_SCHEMA.into()));
    assert_eq!(device_of(&devices, 2).schema, Some(older.into()));
    assert_eq!(
        device_of(&devices, 3).schema,
        None,
        "a device this one never saw a state of shows none (FR-007)"
    );
}

#[test]
fn a_vault_with_only_this_device_still_shows_its_state() {
    let signed = list(vec![listed(1, Role::Main)]);

    let devices = build_with(&signed, &[], &HashMap::new(), &HashSet::new(), 1);

    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].schema, Some(OWN_SCHEMA.into()));
}

#[test]
fn the_state_serializes_with_camel_case_names() {
    let json = serde_json::to_value(DeviceSchema::from(OWN_SCHEMA)).expect("json");
    assert_eq!(
        json,
        serde_json::json!({ "protocol": 2, "holziMigration": 40, "crdtTrigger": 27 })
    );
}
