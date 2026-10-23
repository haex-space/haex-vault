//! The mode a connection gets (contracts/files-protocol.md, SC-007): the list and endpoint checks
//! themselves are `foreign_auth`'s and tested there.

use crate::files::peer::handshake::DialError;
use crate::files::peer::test_support::{own_pair, store_card, Device};
use crate::files::peer::wire::Mode;
use crate::files::share::{device_access, FileShareItemKind, FileShareLevel};
use crate::sync::test_support::Member;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_own_device_gets_the_device_and_none_once_switched_off() {
    let (main, linked) = own_pair().await;
    let link = linked.link_to(&main).await.expect("link");
    assert_eq!(link.welcome.mode, Mode::OwnDevice);

    main.db
        .write_blocking(|tx| Ok(device_access::set_enabled(tx, false, "t")?))
        .expect("off");
    let link = linked.link_to(&main).await.expect("link");
    assert_eq!(
        link.welcome.mode,
        Mode::AccessOff,
        "the other side learns why it gets nothing (FR-014)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_gets_in_only_with_a_grant_on_this_device() {
    let anna = Device::of(Member::genesis()).await;
    let ben = Device::of(Member::genesis()).await;
    store_card(&anna, &ben).await;
    store_card(&ben, &anna).await;

    assert!(
        matches!(ben.link_to(&anna).await, Err(DialError::Refused)),
        "a contact without a grant is closed out"
    );

    let fotos = anna.root().join("Fotos");
    std::fs::create_dir_all(&fotos).expect("folder");
    let share = anna.share(&fotos, FileShareItemKind::Folder, "Fotos");
    anna.grant(&share, ben.member.vault, FileShareLevel::Read);
    let link = ben.link_to(&anna).await.expect("link");
    assert_eq!(link.welcome.mode, Mode::Contact);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_vault_without_a_stored_card_is_closed_out_despite_a_grant() {
    let anna = Device::of(Member::genesis()).await;
    let stranger = Device::of(Member::genesis()).await;
    store_card(&anna, &stranger).await;
    let fotos = anna.root().join("Fotos");
    std::fs::create_dir_all(&fotos).expect("folder");
    let share = anna.share(&fotos, FileShareItemKind::Folder, "Fotos");
    anna.grant(&share, stranger.member.vault, FileShareLevel::Read);
    assert!(matches!(
        stranger.link_to(&anna).await,
        Err(DialError::Refused)
    ));
}
