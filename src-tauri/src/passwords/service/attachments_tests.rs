//! Tests for the attachment methods of the service that serve the attachment view (spec 058): only
//! the user reaches them, and an attachment that is not there is not found.

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
    Setup {
        _dir: dir,
        service: PasswordsService::new(vault),
        attachment,
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
