//! Spec 092, US5 (FR-032, FR-036): the history of a pass shows a moved start as a changed field, a
//! code only through `reveal`, a restore brings back start and codes and leaves the parent and the
//! folder where they are, and a PDF attached to a pass opens through the viewer of spec 058.

// These tests write raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};

use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, HolziBootstrap, HOLZI_TRIGGER_VERSION,
};
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::binaries;
use holzi_lib::passwords::model::{HistorySecret, ItemInput, Patch};
use holzi_lib::passwords::pass::model::{CodeInput, PassInput, PassPatch, Start};
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::vault_gate::{VaultDb, VaultGate};

const CODE: &str = "TICKET-CODE-0815";

struct Fixture {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    service: PasswordsService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("passwords-pass-history"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let vault = VaultGate::new()
        .vault_db(Arc::new(db))
        .expect("open the gate");
    Fixture {
        _dir: dir,
        service: PasswordsService::new(vault.clone()),
        vault,
    }
}

fn input(date: &str, parent: Option<String>) -> PassInput {
    PassInput {
        title: Some("ICE Berlin".to_owned()),
        start: Some(Start::Date {
            date: date.to_owned(),
        }),
        parent_id: parent,
        codes: vec![CodeInput {
            id: None,
            format: "qr".to_owned(),
            value: CODE.to_owned(),
            label: Some("Outbound".to_owned()),
            position: 0,
            flag: None,
        }],
        ..PassInput::default()
    }
}

fn move_start(date: &str) -> PassPatch {
    PassPatch {
        start: Patch::Set(Start::Date {
            date: date.to_owned(),
        }),
        ..PassPatch::default()
    }
}

#[tokio::test]
async fn a_moved_start_is_a_changed_field_and_a_code_is_revealed_only_on_request() {
    let f = fixture();
    let created = f
        .service
        .pass_create(&Caller::User, input("2030-11-14", None), None)
        .await
        .expect("create");
    let id = created.header.id.clone();
    f.service
        .pass_update(
            &Caller::User,
            id.clone(),
            created.header.updated_at.clone().unwrap_or_default(),
            move_start("2030-11-15"),
        )
        .await
        .expect("move the start");

    let states = f
        .service
        .history_list(&Caller::User, id.clone())
        .await
        .expect("list");
    assert!(
        states
            .iter()
            .any(|s| s.changed_fields == vec!["start".to_owned()]),
        "one state names the start as the changed field: {states:?}"
    );

    let newest = f
        .service
        .history_get(&Caller::User, states[0].id.clone())
        .await
        .expect("get");
    let block = newest.pass.as_ref().expect("a pass block");
    assert_eq!(block.start_date.as_deref(), Some("2030-11-15"));
    assert_eq!(block.codes.len(), 1);
    assert_eq!(block.codes[0].label.as_deref(), Some("Outbound"));
    assert!(
        !serde_json::to_string(&newest)
            .expect("serialise")
            .contains(CODE),
        "the state as the window reads it holds no code value"
    );

    let revealed = f
        .service
        .history_reveal(
            &Caller::User,
            newest.id.clone(),
            HistorySecret::Code { index: 0 },
        )
        .await
        .expect("reveal");
    assert_eq!(&*revealed.value, CODE);
}

#[tokio::test]
async fn a_restore_brings_back_start_and_codes_and_leaves_parent_and_folder() {
    let f = fixture();
    let account = f
        .service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Train account".to_owned()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("parent");
    let folder = f
        .service
        .create_group(
            &Caller::User,
            "Tickets".to_owned(),
            None,
            None,
            None,
            Some(holzi_lib::passwords::WALLET_GROUP_ID.to_owned()),
        )
        .await
        .expect("folder");
    let created = f
        .service
        .pass_create(
            &Caller::User,
            input("2030-11-14", Some(account.clone())),
            Some(folder.clone()),
        )
        .await
        .expect("create");
    let id = created.header.id.clone();
    let moved = f
        .service
        .pass_update(
            &Caller::User,
            id.clone(),
            created.header.updated_at.clone().unwrap_or_default(),
            PassPatch {
                codes: Some(Vec::new()),
                ..move_start("2030-11-20")
            },
        )
        .await
        .expect("move the start and drop the code");

    let states = f
        .service
        .history_list(&Caller::User, id.clone())
        .await
        .expect("list");
    // The oldest state still has the first date and its code.
    let oldest = states.last().expect("a state").id.clone();
    let old = f
        .service
        .history_get(&Caller::User, oldest.clone())
        .await
        .expect("get");
    assert_eq!(
        old.pass.as_ref().and_then(|p| p.start_date.as_deref()),
        Some("2030-11-14")
    );

    f.service
        .history_restore(
            &Caller::User,
            id.clone(),
            oldest,
            moved.header.updated_at.clone().unwrap_or_default(),
        )
        .await
        .expect("restore");

    let after = f
        .service
        .get_item(&Caller::User, id.clone())
        .await
        .expect("read");
    assert_eq!(after.header.start_date.as_deref(), Some("2030-11-14"));
    assert_eq!(
        after.header.parent_id.as_deref(),
        Some(account.as_str()),
        "the link stays"
    );
    assert_eq!(
        after.header.group_id.as_deref(),
        Some(folder.as_str()),
        "the folder stays"
    );
    let codes = f
        .service
        .pass_codes(&Caller::User, id)
        .await
        .expect("codes");
    assert_eq!(codes.len(), 1);
    assert_eq!(codes[0].value, CODE);
}

#[tokio::test]
async fn a_pdf_attached_to_a_pass_opens_through_the_viewer() {
    let f = fixture();
    let created = f
        .service
        .pass_create(&Caller::User, input("2030-11-14", None), None)
        .await
        .expect("create");
    let id = created.header.id.clone();
    let pdf = b"%PDF-1.4\n%fake\n".to_vec();
    let attachment = f
        .vault
        .write_blocking({
            let (id, pdf) = (id.clone(), pdf.clone());
            move |tx| Ok(binaries::add_bytes(tx, &id, "ticket.pdf", &pdf)?.id)
        })
        .expect("attach");

    let opened = f
        .service
        .attachment_pdf(&Caller::User, attachment)
        .await
        .expect("the viewer reads it");
    assert_eq!(opened, pdf);
    let detail = f.service.get_item(&Caller::User, id).await.expect("read");
    assert_eq!(detail.attachments.len(), 1);
    assert_eq!(detail.attachments[0].file_name, "ticket.pdf");
}
