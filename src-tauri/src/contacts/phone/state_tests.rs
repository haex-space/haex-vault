use super::*;
use crate::contacts::phone::test_rig::Rig;

fn account(kind: &str, writable: bool, is_default: bool) -> Account {
    Account {
        account_type: kind.to_owned(),
        name: format!("{kind}-name"),
        writable,
        is_default,
    }
}

#[tokio::test]
async fn off_on_and_off_again() {
    let rig = Rig::new();
    assert_eq!(
        rig.status().state,
        PhoneState::Off,
        "off by default (FR-015)"
    );
    rig.enable();
    assert_eq!(rig.status().state, PhoneState::Review);
    rig.disable();
    let status = rig.status();
    assert_eq!(status.state, PhoneState::Off);
    let device = rig.device.clone();
    let row = rig
        .vault
        .db
        .read_blocking(move |q| store::run_row(q, &device).map_err(haex_crdt::Error::from))
        .expect("read");
    assert!(row.is_none(), "switching off deletes the state");
}

#[tokio::test]
async fn the_target_account_is_a_device_setting() {
    let rig = Rig::new();
    let device = rig.device.clone();
    let chosen = account("com.google", true, false);
    let stored = chosen.clone();
    rig.vault
        .db
        .write_blocking(move |tx| set_target(tx, &device, &stored).map_err(haex_crdt::Error::from))
        .expect("write");
    assert_eq!(rig.status().target, Some(chosen));
}

#[test]
fn the_suggestion_is_the_default_then_a_sync_account_then_the_local_one() {
    let local = account("", true, false);
    let google = account("com.google", true, false);
    let default = account("com.exchange", true, true);
    let read_only = account("com.whatsapp", false, false);
    assert_eq!(
        suggested(&[local.clone(), google.clone(), default.clone()]),
        Some(&default)
    );
    assert_eq!(
        suggested(&[local.clone(), read_only.clone(), google.clone()]),
        Some(&google)
    );
    assert_eq!(suggested(&[read_only.clone(), local.clone()]), Some(&local));
    assert_eq!(suggested(&[read_only]), None);
}

#[test]
fn the_list_of_syncing_vaults_names_another_vault() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join(VAULTS_FILE);
    assert!(!other_vault(&file, "a"));
    mark_vault(&file, "a", true);
    assert!(!other_vault(&file, "a"), "the vault itself is no other one");
    assert!(other_vault(&file, "b"), "FR-027");
    mark_vault(&file, "a", false);
    assert!(!other_vault(&file, "b"));
}
