//! The identity of a data action (spec 083 FR-032, FR-033): whose profile decides, and where an
//! answer of the user is written.

use uuid::Uuid;

use super::identity::Identity;
use super::{store, test_support, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grants::{AccessLevel, FolderRight};

#[test]
fn only_the_chat_agent_and_registered_agents_have_an_identity() {
    let device = Uuid::new_v4();
    assert!(matches!(
        Identity::of("builtin", device),
        Some(Identity::Local)
    ));
    let agent = Uuid::new_v4();
    assert!(matches!(
        Identity::of(&agent.to_string(), device),
        Some(Identity::Registered { .. })
    ));
    assert!(Identity::of("someone", device).is_none());
}

#[tokio::test]
async fn two_agents_in_different_profiles_get_different_file_grants() {
    let v = test_support::vault();
    let (member, other) = (
        test_support::registered(&v, "member").await,
        test_support::registered(&v, "other").await,
    );
    let profile = Uuid::new_v4();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Mit Dateien")?;
        let mut rights = crate::mcp_server::grants::Rights::default();
        rights.files.device = true;
        rights
            .files
            .storages
            .insert("s1".to_owned(), AccessLevel::Read);
        store::set_rights(tx, profile, &rights)?;
        store::add_member(tx, &Subject::Agent(member), profile)?;
        store::set_active(tx, profile)
    })
    .await
    .expect("profile");
    let of = |agent: Uuid| Identity::Registered {
        agent,
        device: v.device,
    };
    let (rights, from) = of(member).rights(&v.db).await.expect("rights");
    assert_eq!(from, profile);
    assert!(rights.files.device);
    let (rights, from) = of(other).rights(&v.db).await.expect("rights");
    assert_eq!(from, STANDARD_PROFILE_ID, "not a member: Standard applies");
    assert!(!rights.files.device, "Standard gives no file access");
    assert!(rights.files.storages.is_empty());
}

#[tokio::test]
async fn an_answer_is_written_into_the_profile_it_was_asked_in_and_nowhere_else() {
    let v = test_support::vault();
    let agent = test_support::registered(&v, "agent").await;
    let profile = Uuid::new_v4();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Frage")?;
        store::add_member(tx, &Subject::Agent(agent), profile)?;
        store::set_active(tx, profile)
    })
    .await
    .expect("profile");
    let identity = Identity::Registered {
        agent,
        device: v.device,
    };
    let right = FolderRight {
        level: AccessLevel::ReadWrite,
        cloud: true,
    };
    let written = identity
        .remember(&v.db, profile, move |rights| {
            rights.files.folders.insert("s1/f1".to_owned(), right);
        })
        .await
        .expect("remember");
    assert!(written);
    let (rights, _) = identity.rights(&v.db).await.expect("rights");
    assert_eq!(rights.files.folders["s1/f1"], right);

    // The user switches the active profile while a question is open: the answer is not written.
    v.db.write(|tx| store::set_active(tx, STANDARD_PROFILE_ID))
        .await
        .expect("switch");
    let written = identity
        .remember(&v.db, profile, |rights| {
            rights.files.device = true;
        })
        .await
        .expect("remember");
    assert!(!written);
    let standard =
        v.db.read(|q| super::resolve::standard_rights(q))
            .await
            .expect("standard");
    assert!(!standard.files.device);
}

#[tokio::test]
async fn removing_a_storage_removes_its_rights_from_every_profile() {
    let v = test_support::vault();
    let (one, two) = (Uuid::new_v4(), Uuid::new_v4());
    v.db.write(move |tx| {
        for profile in [one, two] {
            store::create_profile(tx, profile, "P")?;
            let mut rights = crate::mcp_server::grants::Rights::default();
            rights.files.device = true;
            rights
                .files
                .storages
                .insert("s1".to_owned(), AccessLevel::Read);
            rights
                .files
                .storages
                .insert("s10".to_owned(), AccessLevel::Read);
            rights.files.folders.insert(
                "s1/f1".to_owned(),
                FolderRight {
                    level: AccessLevel::Read,
                    cloud: false,
                },
            );
            rights.files.folders.insert(
                "s10/f1".to_owned(),
                FolderRight {
                    level: AccessLevel::Read,
                    cloud: false,
                },
            );
            store::set_rights(tx, profile, &rights)?;
        }
        store::remove_storage_rights(tx, "s1")
    })
    .await
    .expect("removed");
    for profile in [one, two] {
        let rights =
            v.db.read(move |q| super::resolve::profile_rights(q, profile))
                .await
                .expect("rights");
        assert!(rights.files.device, "the rest stays");
        assert!(!rights.files.storages.contains_key("s1"));
        assert!(!rights.files.folders.contains_key("s1/f1"));
        assert!(
            rights.files.storages.contains_key("s10"),
            "another id is not a prefix match"
        );
        assert!(rights.files.folders.contains_key("s10/f1"));
    }
}
