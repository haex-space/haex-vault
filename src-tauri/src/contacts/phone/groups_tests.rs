//! Groups between the phone and holzi in whole runs against the fake phone (spec 051 T096).

use crate::contacts::model::{ContactInput, Head, TargetInput};
use crate::contacts::phone::test_rig::Rig;
use crate::contacts::phone::test_support::{member, name, GOOGLE};
use crate::contacts::phone::{Account, Mode};
use crate::contacts::service::Caller;

fn group_names(rig: &Rig, id: &str) -> Vec<String> {
    rig.contact(id)
        .groups
        .into_iter()
        .map(|group| group.name)
        .collect()
}

async fn group_id(rig: &Rig, name: &str) -> String {
    let groups = rig
        .vault
        .service
        .groups(&Caller::User)
        .await
        .expect("groups");
    match groups.into_iter().find(|group| group.name == name) {
        Some(group) => group.id,
        None => {
            rig.vault
                .service
                .group_save(&Caller::User, None, name.to_owned())
                .await
                .expect("a group")
                .id
        }
    }
}

async fn join(rig: &Rig, contact: &str, group: &str) {
    let group = group_id(rig, group).await;
    rig.vault
        .service
        .group_members(&Caller::User, group, vec![contact.to_owned()], vec![])
        .await
        .expect("join");
}

#[tokio::test]
async fn a_group_of_the_phone_becomes_a_group_in_holzi_but_system_groups_do_not() {
    let rig = Rig::new();
    let family = rig.book.add_group(GOOGLE, "Familie", false);
    let mine = rig.book.add_group(GOOGLE, "My Contacts", true);
    rig.book.add(
        GOOGLE,
        vec![name("Anna", "Beck"), member(family), member(mine)],
    );
    rig.start();
    let anna = rig.named("Anna Beck");
    assert_eq!(group_names(&rig, &anna.id), ["Familie"]);
    let all: Vec<String> = rig
        .vault
        .service
        .groups(&Caller::User)
        .await
        .expect("groups")
        .into_iter()
        .map(|group| group.name)
        .collect();
    assert_eq!(all, ["Familie"]);
}

#[tokio::test]
async fn a_group_set_in_holzi_is_made_on_the_phone_and_joined() {
    let rig = Rig::new();
    let raw = rig.book.add(GOOGLE, vec![name("Anna", "Beck")]);
    rig.start();
    let anna = rig.named("Anna Beck").id;
    join(&rig, &anna, "Arbeit").await;
    rig.run(Mode::Normal).expect("run");
    assert_eq!(rig.book.group_titles(GOOGLE), ["Arbeit"]);
    assert_eq!(rig.book.memberships(raw), ["Arbeit"]);

    rig.run(Mode::Normal).expect("again");
    assert_eq!(rig.book.group_titles(GOOGLE), ["Arbeit"], "made once");
}

#[tokio::test]
async fn leaving_a_group_in_holzi_leaves_it_on_the_phone_and_system_groups_stay() {
    let rig = Rig::new();
    let family = rig.book.add_group(GOOGLE, "Familie", false);
    let mine = rig.book.add_group(GOOGLE, "My Contacts", true);
    let raw = rig.book.add(
        GOOGLE,
        vec![name("Anna", "Beck"), member(family), member(mine)],
    );
    rig.start();
    let anna = rig.named("Anna Beck").id;
    let group = group_id(&rig, "Familie").await;
    rig.vault
        .service
        .group_members(&Caller::User, group, vec![], vec![anna.clone()])
        .await
        .expect("leave");
    rig.run(Mode::Normal).expect("run");
    assert_eq!(rig.book.memberships(raw), ["My Contacts"]);
}

#[tokio::test]
async fn leaving_a_group_on_the_phone_leaves_it_in_holzi() {
    let rig = Rig::new();
    let family = rig.book.add_group(GOOGLE, "Familie", false);
    let raw = rig
        .book
        .add(GOOGLE, vec![name("Anna", "Beck"), member(family)]);
    rig.start();
    let anna = rig.named("Anna Beck").id;
    let row = rig
        .book
        .raw(raw)
        .expect("raw")
        .rows
        .iter()
        .find(|row| row.mimetype == "GroupMembership")
        .expect("membership")
        .data_id;
    rig.book.remove_row(raw, row);
    rig.run(Mode::Normal).expect("run");
    assert!(group_names(&rig, &anna).is_empty());
}

#[tokio::test]
async fn a_new_contact_goes_with_its_groups_into_the_targets_account() {
    let rig = Rig::new();
    rig.start();
    let group = group_id(&rig, "Arbeit").await;
    rig.vault
        .service
        .create(
            &Caller::User,
            &[],
            ContactInput {
                head: Head {
                    given_name: Some("Ben".to_owned()),
                    ..Head::default()
                },
                groups: vec![group],
                targets: vec![TargetInput {
                    device: rig.device.clone(),
                    account: Some(Account {
                        account_type: GOOGLE.0.to_owned(),
                        name: GOOGLE.1.to_owned(),
                        writable: true,
                        is_default: false,
                    }),
                }],
                ..ContactInput::default()
            },
        )
        .await
        .expect("create");
    rig.run(Mode::Normal).expect("run");
    let live = rig.book.live();
    assert_eq!(live.len(), 1);
    assert_eq!(rig.book.memberships(live[0].index.id), ["Arbeit"]);
}

#[tokio::test]
async fn renaming_a_group_in_holzi_moves_the_membership_on_the_phone() {
    let rig = Rig::new();
    let family = rig.book.add_group(GOOGLE, "Familie", false);
    let raw = rig
        .book
        .add(GOOGLE, vec![name("Anna", "Beck"), member(family)]);
    rig.start();
    let group = group_id(&rig, "Familie").await;
    rig.vault
        .service
        .group_save(&Caller::User, Some(group), "Verwandte".into())
        .await
        .expect("rename");
    rig.run(Mode::Normal).expect("run");
    assert_eq!(rig.book.memberships(raw), ["Verwandte"]);
}
