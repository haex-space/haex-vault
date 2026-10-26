//! Tests for the attachment methods of the service that serve the attachment view and the system's
//! app (spec 058): only the user reaches them, an attachment that is not there is not found, and
//! "Nicht mehr fragen" holds on its own device alone.

use std::sync::Arc;

use super::*;
use crate::passwords::items;
use crate::passwords::model::ItemInput;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

struct Setup {
    _dir: tempfile::TempDir,
    service: PasswordsService,
    attachment: String,
    device: uuid::Uuid,
}

/// A vault with one entry that has `file_name` with `bytes` attached.
fn setup(file_name: &str, bytes: &[u8]) -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db)).unwrap();
    let attachment = vault
        .write_blocking(|tx| {
            let item = items::create_item(tx, &ItemInput::default(), None)?;
            Ok(binaries::add_bytes(tx, &item, file_name, bytes)?.id)
        })
        .unwrap();
    let device = vault
        .read_blocking(|q| crate::storage::known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    Setup {
        _dir: dir,
        service: PasswordsService::new(vault),
        attachment,
        device,
    }
}

#[tokio::test]
async fn the_user_reads_the_text_of_a_text_attachment() {
    let s = setup("notiz.txt", "Grüße".as_bytes());
    let text = s
        .service
        .attachment_text(&Caller::User, s.attachment.clone())
        .await
        .unwrap();
    assert_eq!(text.text, "Grüße");
    assert!(!text.truncated);
}

#[tokio::test]
async fn only_the_user_reads_the_text_of_an_attachment() {
    let s = setup("notiz.txt", b"geheim");
    let refused = s
        .service
        .attachment_text(&Caller::BuiltinAgent, s.attachment.clone())
        .await;
    assert!(matches!(refused, Err(HolziError::PasswordsForbidden)));
}

#[tokio::test]
async fn the_text_of_an_unknown_attachment_is_not_found() {
    let s = setup("notiz.txt", b"abc");
    let missing = s
        .service
        .attachment_text(&Caller::User, "unknown".to_owned())
        .await;
    assert!(matches!(missing, Err(HolziError::PasswordsNotFound)));
}

#[tokio::test]
async fn an_image_attachment_has_no_text() {
    let s = setup("bild.png", b"\x89PNG");
    let refused = s
        .service
        .attachment_text(&Caller::User, s.attachment.clone())
        .await;
    assert!(matches!(
        refused,
        Err(HolziError::InvalidInput { reason }) if reason == "not_text"
    ));
}

#[tokio::test]
async fn the_user_gets_the_bytes_of_a_pdf_attachment() {
    let s = setup("police.pdf", b"%PDF-1.4\n%%EOF\n");
    let bytes = s
        .service
        .attachment_pdf(&Caller::User, s.attachment.clone())
        .await
        .unwrap();
    assert_eq!(bytes, b"%PDF-1.4\n%%EOF\n");
}

#[tokio::test]
async fn only_the_user_gets_the_bytes_of_a_pdf_attachment() {
    let s = setup("police.pdf", b"%PDF-1.4");
    let refused = s
        .service
        .attachment_pdf(&Caller::BuiltinAgent, s.attachment.clone())
        .await;
    assert!(matches!(refused, Err(HolziError::PasswordsForbidden)));
}

#[tokio::test]
async fn a_text_attachment_is_no_pdf() {
    let s = setup("notiz.txt", b"abc");
    let refused = s
        .service
        .attachment_pdf(&Caller::User, s.attachment.clone())
        .await;
    assert!(matches!(
        refused,
        Err(HolziError::InvalidInput { reason }) if reason == "not_pdf"
    ));
}

/// The files below `root`, as paths relative to it.
fn files_below(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(dirs) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    dirs.flatten()
        .flat_map(|dir| std::fs::read_dir(dir.path()).unwrap().flatten())
        .map(|file| file.path().strip_prefix(root).unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn without_the_notice_nothing_is_copied() {
    let s = setup("tabelle.ods", b"geheim");
    let root = tempfile::tempdir().unwrap();
    let copy = s
        .service
        .attachment_copy_for_app(
            &Caller::User,
            s.attachment.clone(),
            root.path().to_owned(),
            s.device,
            false,
            false,
        )
        .await
        .unwrap();
    assert_eq!(copy, None);
    assert!(files_below(root.path()).is_empty());
}

#[tokio::test]
async fn a_confirmed_notice_copies_without_remembering() {
    let s = setup("tabelle.ods", b"geheim");
    let root = tempfile::tempdir().unwrap();
    let copy = s
        .service
        .attachment_copy_for_app(
            &Caller::User,
            s.attachment.clone(),
            root.path().to_owned(),
            s.device,
            true,
            false,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(std::fs::read(&copy).unwrap(), b"geheim");
    let again = s
        .service
        .attachment_copy_for_app(
            &Caller::User,
            s.attachment.clone(),
            root.path().to_owned(),
            s.device,
            false,
            false,
        )
        .await
        .unwrap();
    assert_eq!(again, None, "the notice comes again");
}

#[tokio::test]
async fn dont_ask_again_holds_on_this_device_alone() {
    let s = setup("tabelle.ods", b"geheim");
    let root = tempfile::tempdir().unwrap();
    let open = |device, confirmed, dont_ask_again| {
        s.service.attachment_copy_for_app(
            &Caller::User,
            s.attachment.clone(),
            root.path().to_owned(),
            device,
            confirmed,
            dont_ask_again,
        )
    };
    assert!(open(s.device, true, true).await.unwrap().is_some());
    let ack = s
        .service
        .db()
        .read({
            let device = s.device;
            move |q| {
                crate::storage::preferences::get(
                    q,
                    crate::storage::preferences::PrefScope::Device(device),
                    crate::passwords::attachment_view::OPEN_ACK_KEY,
                )
            }
        })
        .await
        .unwrap();
    assert_eq!(ack.as_deref(), Some("1"));
    assert!(
        open(s.device, false, false).await.unwrap().is_some(),
        "no notice any more"
    );
    assert_eq!(
        open(uuid::Uuid::new_v4(), false, false).await.unwrap(),
        None,
        "another device still gets the notice"
    );
    assert_eq!(files_below(root.path()).len(), 2);
}

#[tokio::test]
async fn only_the_user_copies_an_attachment() {
    let s = setup("tabelle.ods", b"geheim");
    let root = tempfile::tempdir().unwrap();
    let refused = s
        .service
        .attachment_copy_for_app(
            &Caller::BuiltinAgent,
            s.attachment.clone(),
            root.path().to_owned(),
            s.device,
            true,
            true,
        )
        .await;
    assert!(matches!(refused, Err(HolziError::PasswordsForbidden)));
    assert!(files_below(root.path()).is_empty());
}
