//! A vault file from before the rename (spec 054, `fixtures/pre_rename_vault/README.md`) opens
//! through the file import and everything in it is still there: the entries with their secrets, the
//! attachment, the trash, the contact and the provider. The test names no name that is stored in
//! the vault (no table, no kind of contact, no id), so it holds through the rename; only its
//! `use` lines follow the crate. When it fails, a name stored in the vault no longer matches what
//! the code looks for (data-model.md, "Persistierte Namen").
//!
//! Linux only: storage is redirected through `XDG_DATA_HOME`, which Tauri's `AppLocalData`
//! honours only on Linux.
#![cfg(target_os = "linux")]
// The checks read raw vault state (row counts) that the services do not expose.
#![allow(clippy::disallowed_methods)]

#[path = "common/chosen_files.rs"]
mod chosen_files;

use std::path::PathBuf;

use sha2::{Digest, Sha256};
use tauri::test::{mock_builder, mock_context, noop_assets};

use chosen_files::{chosen, Paths};
use holzi_lib::chat::session::ChatState;
use holzi_lib::contacts::service::{Caller, ContactsService};
use holzi_lib::instances::import_instance_core;
use holzi_lib::passwords::service::{Headers, PasswordsService};
use holzi_lib::state::AppState;
use holzi_lib::vault_gate::VaultGate;

const PASSPHRASE: &str = "pre-rename-test-passphrase";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pre_rename_vault/pre_rename_vault.db")
}

fn checksum(path: &PathBuf) -> Vec<u8> {
    Sha256::digest(std::fs::read(path).expect("read")).to_vec()
}

#[tokio::test]
async fn a_vault_from_before_the_rename_opens_with_its_data() {
    let file = fixture();
    let before = checksum(&file);

    let here = tempfile::tempdir().expect("data home");
    std::env::set_var("XDG_DATA_HOME", here.path());
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock app")
        .handle()
        .clone();
    let gate = VaultGate::new();
    let state = AppState::new(gate.clone());
    let chat = ChatState::with_children(gate.children());

    let result = import_instance_core(&app, &state, &chat, Paths, chosen(&file), PASSPHRASE.into())
        .await
        .expect("the old vault opens");
    assert_eq!(result.info.name, "pre_rename_vault");
    let vault = state.database().expect("the vault is the active one");

    // The entries outside the trash, with their secrets.
    let passwords = PasswordsService::new(vault.clone());
    let Headers::Items(headers) = passwords
        .list_headers(&Caller::User, &[])
        .await
        .expect("list the entries")
    else {
        panic!("a user sees the entries, not the agent headers");
    };
    let mut titles: Vec<String> = headers
        .iter()
        .filter_map(|header| header.title.clone())
        .collect();
    titles.sort();
    assert_eq!(titles, ["Mail", "Router"]);

    let mail = headers
        .iter()
        .find(|header| header.title.as_deref() == Some("Mail"))
        .expect("the mail entry");
    let secret = passwords
        .read_secret_item(&Caller::User, &[], mail.id.clone())
        .await
        .expect("read the mail entry");
    assert_eq!(secret.username.as_deref(), Some("alice@example.org"));
    assert_eq!(secret.password.as_deref(), Some("pre-rename-mail-secret"));
    assert_eq!(secret.otp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));
    assert_eq!(secret.note.as_deref(), Some("A note on the mail entry"));
    let mut tags = secret.tags.clone();
    tags.sort();
    assert_eq!(tags, ["mail", "work"]);
    assert_eq!(secret.key_values.len(), 1);
    assert_eq!(secret.key_values[0].key.as_deref(), Some("PIN"));
    assert_eq!(secret.key_values[0].value.as_deref(), Some("4711"));

    let router = headers
        .iter()
        .find(|header| header.title.as_deref() == Some("Router"))
        .expect("the router entry");
    let secret = passwords
        .read_secret_item(&Caller::User, &[], router.id.clone())
        .await
        .expect("read the router entry");
    assert_eq!(secret.password.as_deref(), Some("pre-rename-router-secret"));
    let router_detail = passwords
        .get_item(&Caller::User, router.id.clone())
        .await
        .expect("read the router entry details");
    assert_eq!(router_detail.attachments.len(), 1);
    assert_eq!(router_detail.attachments[0].file_name, "router-notes.txt");
    assert!(router_detail.attachments[0].size > 0);

    // The overview also carries the entry in the trash, and the folder.
    let overview = passwords
        .load_overview(&Caller::User)
        .await
        .expect("the overview");
    assert_eq!(
        overview.headers.len(),
        3,
        "two entries and one in the trash"
    );
    let in_trash: Vec<&str> = overview
        .headers
        .iter()
        .filter(|header| header.group_id.as_deref() == Some("trash"))
        .filter_map(|header| header.title.as_deref())
        .collect();
    assert_eq!(in_trash, ["Old shop"]);
    let old_shop = overview
        .headers
        .iter()
        .find(|header| header.title.as_deref() == Some("Old shop"))
        .expect("the trashed entry");
    let old_shop_detail = passwords
        .get_item(&Caller::User, old_shop.id.clone())
        .await
        .expect("read the trashed entry details");
    assert_eq!(old_shop_detail.header.title.as_deref(), Some("Old shop"));
    let old_shop_secret = passwords
        .read_secret_item(&Caller::User, &[], old_shop.id.clone())
        .await
        .expect("read the trashed entry");
    assert_eq!(old_shop_secret.username.as_deref(), Some("bob"));
    assert_eq!(
        old_shop_secret.password.as_deref(),
        Some("pre-rename-shop-secret")
    );
    assert!(
        overview
            .groups
            .iter()
            .any(|group| group.name.as_deref() == Some("Work")),
        "the folder is there"
    );

    // The contact made in the app, with all of its values.
    let contacts = ContactsService::new(vault.clone());
    let headers = contacts
        .list(&Caller::User, &[])
        .await
        .expect("list the contacts");
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].name, "Carol Example");
    let contact = contacts
        .get(&Caller::User, &[], &headers[0].id)
        .await
        .expect("read the contact");
    assert_eq!(contact.values.len(), 4, "e-mail and the three addresses");
    assert!(contact.values.iter().all(|value| !value.value.is_empty()));
    assert_eq!(
        contact
            .values
            .iter()
            .filter(|value| value.value.contains('@'))
            .count(),
        1,
        "the e-mail value is still present"
    );

    // The attachment and the provider.
    let db = vault.database();
    let (binaries, providers) = db
        .with_connection(|conn| {
            let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).expect(sql) };
            Ok((
                count("SELECT COUNT(*) FROM haex_passwords_binaries"),
                count("SELECT COUNT(*) FROM providers"),
            ))
        })
        .expect("count");
    assert_eq!(binaries, 1, "the attachment of the router entry");
    assert!(providers >= 1, "the chat provider");

    assert_eq!(checksum(&file), before, "the chosen file is only read");
    drop(db);
    drop(vault);
    drop(passwords);
    drop(contacts);
    drop(state.take());
}
