//! Requests against a serving device over a real connection: the checks on every request (FR-005),
//! the root of a share (FR-006, SC-005), holzi's own places (FR-007) and a revoked grant (FR-024).

use crate::files::peer::client::PeerLink;
use crate::files::peer::test_support::{own_pair, store_card, Device};
use crate::files::peer::wire::{Caller, FailCode, Op, Reply, WireKind};
use crate::files::share::{device_access, FileShareItemKind, FileShareLevel};
use crate::sync::test_support::Member;

async fn ask(link: &PeerLink, caller: Caller, op: Op) -> Reply {
    link.ask(caller, op).await.expect("answer").0
}

fn list(share: Option<&str>, path: &str) -> Op {
    Op::ListDir {
        share: share.map(str::to_owned),
        path: path.to_owned(),
        max: 100,
    }
}

fn names(reply: Reply) -> Vec<String> {
    let Reply::Entries(entries) = reply else {
        panic!("a listing, got {reply:?}");
    };
    let mut names: Vec<String> = entries.into_iter().map(|e| e.name).collect();
    names.sort();
    names
}

/// Anna shares her folder `Fotos` with Ben at `level`; returns both, the share and Ben's link.
async fn shared(level: FileShareLevel) -> (Device, Device, String, PeerLink) {
    let anna = Device::of(Member::genesis()).await;
    let ben = Device::of(Member::genesis()).await;
    store_card(&anna, &ben).await;
    store_card(&ben, &anna).await;
    anna.write("Fotos/a.jpg", b"aaaa");
    anna.write("Fotos/Urlaub/b.jpg", b"bb");
    anna.write("Geheim/c.txt", b"secret");
    let share = anna.share(
        &anna.root().join("Fotos"),
        FileShareItemKind::Folder,
        "Fotos",
    );
    anna.grant(&share, ben.member.vault, level);
    let link = ben.link_to(&anna).await.expect("link");
    (anna, ben, share, link)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_lists_its_share_and_nothing_beside_it() {
    let (_anna, _ben, share, link) = shared(FileShareLevel::Read).await;
    let Reply::Shares(shares) = ask(&link, Caller::User, Op::Shares).await else {
        panic!("shares");
    };
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].id, share);
    assert_eq!(shares[0].level, FileShareLevel::Read);

    assert_eq!(
        names(ask(&link, Caller::User, list(Some(&share), "/")).await),
        ["Urlaub", "a.jpg"]
    );
    for escape in ["/..", "/../Geheim", "/Urlaub/../../Geheim"] {
        assert_eq!(
            ask(&link, Caller::User, list(Some(&share), escape)).await,
            Reply::Failed(FailCode::Refused),
            "{escape}"
        );
    }
    assert_eq!(
        ask(&link, Caller::User, list(None, "/")).await,
        Reply::Failed(FailCode::Refused),
        "a contact names a share"
    );
    assert_eq!(
        ask(&link, Caller::User, list(Some("not-a-share"), "/")).await,
        Reply::Failed(FailCode::Refused)
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_link_out_of_the_share_is_refused() {
    let (anna, _ben, share, link) = shared(FileShareLevel::Read).await;
    std::os::unix::fs::symlink(anna.root().join("Geheim"), anna.root().join("Fotos/out"))
        .expect("link");
    std::os::unix::fs::symlink(
        anna.root().join("Geheim/c.txt"),
        anna.root().join("Fotos/c.txt"),
    )
    .expect("link");
    let listing = names(ask(&link, Caller::User, list(Some(&share), "/")).await);
    assert!(!listing.contains(&"out".to_owned()), "links are not listed");
    for path in ["/out", "/c.txt"] {
        let read = Op::Read {
            share: Some(share.clone()),
            path: path.to_owned(),
            start: 0,
            len: 10,
        };
        assert!(
            matches!(
                ask(&link, Caller::User, read).await,
                Reply::Failed(FailCode::Refused | FailCode::NotFound)
            ),
            "{path} reaches nothing outside the share (SC-005)"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_revoked_grant_refuses_the_next_request() {
    let (anna, ben, share, link) = shared(FileShareLevel::Read).await;
    assert!(matches!(
        ask(&link, Caller::User, list(Some(&share), "/")).await,
        Reply::Entries(_)
    ));
    anna.revoke(&share, ben.member.vault);
    assert_eq!(
        ask(&link, Caller::User, list(Some(&share), "/")).await,
        Reply::Failed(FailCode::Refused),
        "FR-024: on the same connection"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_shared_file_is_all_its_contact_sees() {
    let anna = Device::of(Member::genesis()).await;
    let ben = Device::of(Member::genesis()).await;
    store_card(&anna, &ben).await;
    store_card(&ben, &anna).await;
    let letter = anna.write("Post/Brief.pdf", b"%PDF");
    anna.write("Post/Rechnung.pdf", b"%PDF");
    let share = anna.share(&letter, FileShareItemKind::File, "Brief");
    anna.grant(&share, ben.member.vault, FileShareLevel::Read);
    let link = ben.link_to(&anna).await.expect("link");
    assert_eq!(
        names(ask(&link, Caller::User, list(Some(&share), "/")).await),
        ["Brief.pdf"]
    );
    assert_eq!(
        ask(
            &link,
            Caller::User,
            Op::Stat {
                share: Some(share.clone()),
                path: "/Rechnung.pdf".to_owned()
            }
        )
        .await,
        Reply::Failed(FailCode::NotFound)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn holzis_own_data_is_read_only_for_an_own_device_and_closed_to_its_agents() {
    let (main, linked) = own_pair().await;
    let own = main.own_place();
    std::fs::write(own.join("vault.db"), b"x").expect("own file");
    let link = linked.link_to(&main).await.expect("link");
    let own_path = own.to_string_lossy().into_owned();
    let parent = main.root().to_string_lossy().into_owned();

    assert_eq!(
        names(ask(&link, Caller::User, list(None, &own_path)).await),
        ["vault.db"],
        "the user sees it as the local file browser shows it"
    );
    assert!(names(ask(&link, Caller::User, list(None, &parent)).await)
        .contains(&"holzi-own".to_owned()));
    assert_eq!(
        ask(&link, Caller::Agent, list(None, &own_path)).await,
        Reply::Failed(FailCode::Refused)
    );
    assert!(
        !names(ask(&link, Caller::Agent, list(None, &parent)).await)
            .contains(&"holzi-own".to_owned()),
        "an agent does not even see it listed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn holzis_own_data_stays_closed_inside_a_share() {
    let anna = Device::of(Member::genesis()).await;
    let ben = Device::of(Member::genesis()).await;
    store_card(&anna, &ben).await;
    store_card(&ben, &anna).await;
    std::fs::write(anna.own_place().join("vault.db"), b"x").expect("own file");
    anna.write("visible.txt", b"v");
    let share = anna.share(&anna.root(), FileShareItemKind::Folder, "Alles");
    anna.grant(&share, ben.member.vault, FileShareLevel::Read);
    let link = ben.link_to(&anna).await.expect("link");
    let listing = names(ask(&link, Caller::User, list(Some(&share), "/")).await);
    assert!(listing.contains(&"visible.txt".to_owned()));
    assert!(!listing.contains(&"holzi-own".to_owned()));
    assert_eq!(
        ask(&link, Caller::User, list(Some(&share), "/holzi-own")).await,
        Reply::Failed(FailCode::Refused)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn switching_off_stops_an_own_device_on_its_next_request() {
    let (main, linked) = own_pair().await;
    main.write("doc.txt", b"hello");
    let link = linked.link_to(&main).await.expect("link");
    let root = main.root().to_string_lossy().into_owned();
    let Reply::Entries(entries) = ask(&link, Caller::User, list(None, &root)).await else {
        panic!("a listing");
    };
    assert!(entries
        .iter()
        .any(|e| e.name == "doc.txt" && e.kind == WireKind::File && e.size == 5));
    main.db
        .write_blocking(|tx| Ok(device_access::set_enabled(tx, false, "t")?))
        .expect("off");
    assert_eq!(
        ask(&link, Caller::User, list(None, &root)).await,
        Reply::Failed(FailCode::Refused)
    );
}
