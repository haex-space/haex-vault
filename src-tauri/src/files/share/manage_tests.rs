//! Managing shares (spec 056 US2, FR-015–FR-019): only this device's own files, grants only for
//! vaults with a stored card, one share per root, and edits from another own device.

use super::{FileShareGrantInput, FileShareSaveArgs, FileShareState, Here};
use crate::contacts::card::received;
use crate::files::local::OwnPlaces;
use crate::files::peer::test_support::{store_card, Device};
use crate::files::share::{FileShareItemKind, FileShareLevel};
use crate::files::{FilesErrorCode, SourceRef};
use crate::sync::test_support::Member;

fn grant(vault: [u8; 32], level: FileShareLevel) -> FileShareGrantInput {
    FileShareGrantInput {
        vault_pubkey: crate::sync::keys::hex(&vault),
        level,
    }
}

fn args(path: &std::path::Path, name: &str, grants: Vec<FileShareGrantInput>) -> FileShareSaveArgs {
    FileShareSaveArgs {
        share_id: None,
        source: SourceRef::Device,
        path: path.to_string_lossy().into_owned(),
        name: name.to_owned(),
        grants,
    }
}

/// Anna's device, with Ben's card stored on a contact.
async fn anna_and_ben() -> (Device, Device) {
    let anna = Device::of(Member::genesis()).await;
    let ben = Device::of(Member::genesis()).await;
    store_card(&ben, &anna).await;
    (anna, ben)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_folder_of_this_device_is_shared_once_with_a_contact() {
    let (anna, ben) = anna_and_ben().await;
    let fotos = anna.root().join("Fotos");
    std::fs::create_dir(&fotos).expect("folder");
    let own = OwnPlaces::new(vec![anna.own_place()]);
    let here = Here {
        db: &anna.db,
        device: &anna.uuid,
        own: &own,
    };

    let share = super::save(
        &here,
        args(
            &fotos,
            "Fotos",
            vec![grant(ben.member.vault, FileShareLevel::Read)],
        ),
    )
    .await
    .expect("save");
    assert_eq!(share.item_kind, FileShareItemKind::Folder);
    assert!(share.serving_device.is_current);
    assert_eq!(share.state, FileShareState::Active, "served here at once");
    assert_eq!(share.grants.len(), 1);
    assert!(share.grants[0].contact_id.is_some());
    assert!(share.grants[0].contact_name.is_some());

    let again = super::save(
        &here,
        args(
            &fotos,
            "Urlaub",
            vec![grant(ben.member.vault, FileShareLevel::ReadWrite)],
        ),
    )
    .await
    .expect("save again");
    assert_eq!(again.id, share.id, "one share per root (FR-017)");
    assert_eq!(again.name, "Urlaub");
    assert_eq!(again.grants[0].level, FileShareLevel::ReadWrite);
    assert_eq!(super::list(&here).await.expect("list").len(), 1);
    let found = super::of(
        &here,
        SourceRef::Device,
        fotos.to_string_lossy().into_owned(),
    )
    .await
    .expect("of");
    assert_eq!(found.map(|s| s.id), Some(share.id.clone()));

    let contact = anna
        .db
        .read(move |q| Ok(received::stored(q, &ben.member.vault)?))
        .await
        .expect("card")
        .expect("stored")
        .0;
    let of_contact = super::of_contact(&here, contact).await.expect("of contact");
    assert_eq!(of_contact.len(), 1);

    let revoked = super::revoke(
        &here,
        share.id.clone(),
        crate::sync::keys::hex(&ben.member.vault),
    )
    .await
    .expect("revoke");
    assert!(revoked.grants.is_empty());
    super::delete(&here, share.id).await.expect("delete");
    assert!(super::list(&here).await.expect("list").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_vaults_with_a_stored_card_get_a_grant() {
    let (anna, _ben) = anna_and_ben().await;
    let stranger = Member::genesis();
    let own = OwnPlaces::new(vec![anna.own_place()]);
    let here = Here {
        db: &anna.db,
        device: &anna.uuid,
        own: &own,
    };
    let refused = super::save(
        &here,
        args(
            &anna.root(),
            "Alles",
            vec![grant(stranger.vault, FileShareLevel::Read)],
        ),
    )
    .await
    .expect_err("no card (FR-016)");
    assert_eq!(refused.code, FilesErrorCode::NotFound);
    assert!(
        super::list(&here).await.expect("list").is_empty(),
        "nothing saved"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn holzis_own_data_and_other_sources_are_never_shared_from_here() {
    let (anna, _ben) = anna_and_ben().await;
    let own = OwnPlaces::new(vec![anna.own_place()]);
    let here = Here {
        db: &anna.db,
        device: &anna.uuid,
        own: &own,
    };
    let refused = super::save(&here, args(&anna.own_place(), "holzi", Vec::new()))
        .await
        .expect_err("own data");
    assert_eq!(refused.code, FilesErrorCode::HolziOwned);

    let mut storage = args(&anna.root(), "Cloud", Vec::new());
    storage.source = SourceRef::Storage {
        storage_id: "s1".to_owned(),
    };
    let refused = super::save(&here, storage).await.expect_err("storage");
    assert_eq!(refused.code, FilesErrorCode::Unsupported);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn another_own_device_changes_name_and_grants_but_not_the_root() {
    let (anna, ben) = anna_and_ben().await;
    let own = OwnPlaces::new(vec![anna.own_place()]);
    let here = Here {
        db: &anna.db,
        device: &anna.uuid,
        own: &own,
    };
    let share = super::save(&here, args(&anna.root(), "Alles", Vec::new()))
        .await
        .expect("save");

    let elsewhere = uuid::Uuid::new_v4().to_string();
    let phone = Here {
        db: &anna.db,
        device: &elsewhere,
        own: &own,
    };
    let mut change = args(
        &anna.root().join("woanders"),
        "Neu",
        vec![grant(ben.member.vault, FileShareLevel::Read)],
    );
    change.share_id = Some(share.id.clone());
    let changed = super::save(&phone, change).await.expect("change (FR-019)");
    assert_eq!(changed.id, share.id);
    assert_eq!(changed.name, "Neu");
    assert_eq!(changed.path, share.path, "the root stays");
    assert_eq!(
        changed.serving_device.id, anna.uuid,
        "the serving device stays"
    );
    assert!(!changed.serving_device.is_current);
    assert_eq!(
        changed.state,
        FileShareState::Pending,
        "the serving device has not seen it yet (FR-026)"
    );
}
