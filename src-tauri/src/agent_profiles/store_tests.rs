//! Tests for the three tables of agent profiles (spec 083 T009): profiles, memberships, the active
//! profile, deletes.

use uuid::Uuid;

use super::store;
use super::test_support::vault;
use super::{member_id, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grants::{OperateLevel, Rights};
use crate::storage::query::Query;

fn agent() -> Subject {
    Subject::Agent(Uuid::new_v4())
}

fn reading(scope: &str) -> Rights {
    Rights {
        operate: [(scope.to_owned(), OperateLevel::Read)]
            .into_iter()
            .collect(),
        ..Rights::default()
    }
}

#[test]
fn a_subject_is_written_and_read_back_as_stored() {
    let id = Uuid::new_v4();
    for subject in [Subject::Agent(id), Subject::Model("p1:gpt".to_owned())] {
        assert_eq!(Subject::parse(&subject.key()), Some(subject));
    }
    assert_eq!(Subject::parse("agent:not-a-uuid"), None);
    assert_eq!(Subject::parse("model:"), None);
    assert_eq!(Subject::parse("device:x"), None);
}

#[test]
fn the_member_id_is_a_function_of_the_subject_and_the_profile() {
    let (subject, one, two) = (agent(), Uuid::new_v4(), Uuid::new_v4());
    assert_eq!(member_id(&subject, one), member_id(&subject.clone(), one));
    assert_ne!(member_id(&subject, one), member_id(&subject, two));
    assert_ne!(member_id(&subject, one), member_id(&agent(), one));
}

#[tokio::test]
async fn a_subject_is_a_member_of_several_profiles_and_adding_twice_leaves_one_row_each() {
    let v = vault();
    let (one, two, subject) = (Uuid::new_v4(), Uuid::new_v4(), agent());
    let s = subject.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, one, "Eins")?;
        store::create_profile(tx, two, "Zwei")?;
        store::add_member(tx, &s, one)?;
        store::add_member(tx, &s, two)?;
        store::add_member(tx, &s, two)?;
        Ok(())
    })
    .await
    .expect("add");
    let key = subject.key();
    let (in_one, in_two, own) =
        v.db.read(move |q| {
            Ok((
                store::members(q, one)?,
                store::members(q, two)?,
                store::memberships(q, &subject)?,
            ))
        })
        .await
        .expect("read");
    assert_eq!(
        in_one,
        vec![key.clone()],
        "adding to the second keeps the first"
    );
    assert_eq!(in_two, vec![key]);
    let mut profiles = own.profiles;
    profiles.sort();
    let mut expected = vec![one, two];
    expected.sort();
    assert_eq!(profiles, expected);
    assert!(!own.unreadable);
}

#[tokio::test]
async fn removing_one_membership_keeps_the_others_and_standard_has_no_row() {
    let v = vault();
    let (one, two, subject) = (Uuid::new_v4(), Uuid::new_v4(), agent());
    let s = subject.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, one, "Eins")?;
        store::create_profile(tx, two, "Zwei")?;
        store::add_member(tx, &s, one)?;
        store::add_member(tx, &s, two)?;
        store::add_member(tx, &s, STANDARD_PROFILE_ID)?;
        store::remove_member(tx, &s, one)
    })
    .await
    .expect("change");
    let own =
        v.db.read(move |q| store::memberships(q, &subject))
            .await
            .expect("read");
    assert_eq!(own.profiles, vec![two], "Standard is implicit, never a row");
}

#[tokio::test]
async fn a_legacy_membership_row_can_be_removed_by_its_subject_and_profile() {
    let v = vault();
    let (profile, subject) = (Uuid::new_v4(), agent());
    let key = subject.key();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Alt")?;
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES ('legacy-id', ?1, ?2)",
            haex_crdt::rusqlite::params![key, profile.to_string()],
        )?;
        store::remove_member(tx, &subject, profile)
    })
    .await
    .expect("remove stale row");
    let members =
        v.db.read(move |q| store::members(q, profile))
            .await
            .expect("read");
    assert!(members.is_empty());
}

#[tokio::test]
async fn the_active_profile_is_one_vault_value_and_standard_unsets_it() {
    let v = vault();
    let profile = Uuid::new_v4();
    assert_eq!(
        v.db.read(|q| store::active(q)).await.expect("read"),
        store::Active::Unset
    );
    v.db.write(move |tx| store::set_active(tx, profile))
        .await
        .expect("set");
    assert_eq!(
        v.db.read(|q| store::active(q)).await.expect("read"),
        store::Active::Profile(profile)
    );
    v.db.write(|tx| store::set_active(tx, STANDARD_PROFILE_ID))
        .await
        .expect("set");
    assert_eq!(
        v.db.read(|q| store::active(q)).await.expect("read"),
        store::Active::Unset
    );
}

async fn memberships_of(
    v: &super::test_support::Vault,
    subject: &Subject,
) -> crate::error::Result<store::Memberships> {
    let subject = subject.clone();
    v.db.read(move |q| store::memberships(q, &subject)).await
}

async fn member_ids(v: &super::test_support::Vault) -> crate::error::Result<Vec<String>> {
    v.db.read(|q| {
        q.query_map(
            "SELECT id FROM haex_agent_profile_members ORDER BY id",
            haex_crdt::rusqlite::params![],
            |row| row.get::<_, String>(0),
        )
    })
    .await
}

#[tokio::test]
async fn a_member_row_with_the_id_of_the_subject_alone_stays_a_membership_and_is_never_rewritten() {
    use haex_crdt::rusqlite::params;
    let v = vault();
    let (profile, other, subject) = (Uuid::new_v4(), Uuid::new_v4(), agent());
    let (s, legacy) = (subject.clone(), super::legacy_member_id(&subject));
    let legacy_id = legacy.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Alt")?;
        store::create_profile(tx, other, "Neu")?;
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES (?1, ?2, ?3)",
            params![legacy_id, s.key(), profile.to_string()],
        )?;
        Ok(())
    })
    .await
    .expect("a row from before");
    let before = memberships_of(&v, &subject).await.expect("read");
    assert_eq!(
        before.profiles,
        vec![profile],
        "the old row is a membership"
    );
    assert!(!before.unreadable);

    // Adding what the subject has already writes nothing; adding another profile writes one row.
    let s = subject.clone();
    v.db.write(move |tx| store::add_member(tx, &s, profile))
        .await
        .expect("add again");
    assert_eq!(member_ids(&v).await.expect("rows"), vec![legacy.clone()]);
    let s = subject.clone();
    v.db.write(move |tx| store::add_member(tx, &s, other))
        .await
        .expect("add another");
    assert_eq!(member_ids(&v).await.expect("rows").len(), 2);

    // Removing it deletes the row that holds it, so the removal is an ordinary delete that syncs.
    let s = subject.clone();
    v.db.write(move |tx| store::remove_member(tx, &s, profile))
        .await
        .expect("remove");
    let after = memberships_of(&v, &subject).await.expect("read");
    assert_eq!(after.profiles, vec![other]);
    assert!(!member_ids(&v).await.expect("rows").contains(&legacy));
}

#[tokio::test]
async fn the_first_edit_of_standard_writes_its_row_and_it_stays_fixed() {
    let v = vault();
    let before =
        v.db.read(|q| store::profile(q, STANDARD_PROFILE_ID))
            .await
            .expect("read");
    assert!(before.is_none(), "Standard has no row until it is edited");
    v.db.write(|tx| store::set_rights(tx, STANDARD_PROFILE_ID, &reading("wm.read")))
        .await
        .expect("edit");
    let after =
        v.db.read(|q| store::profile(q, STANDARD_PROFILE_ID))
            .await
            .expect("read");
    assert_eq!(after.map(|p| p.id), Some(STANDARD_PROFILE_ID));
    let renamed = v
        .db
        .write(|tx| store::rename_profile(tx, STANDARD_PROFILE_ID, "Anders").map_err(Into::into))
        .await;
    assert!(renamed.is_err(), "Standard keeps its name");
    let deleted =
        v.db.write(|tx| store::delete_profile(tx, STANDARD_PROFILE_ID).map_err(Into::into))
            .await;
    assert!(deleted.is_err(), "Standard cannot be deleted");
}

#[tokio::test]
async fn profiles_may_share_a_name_and_an_empty_name_is_refused() {
    let v = vault();
    let (one, two) = (Uuid::new_v4(), Uuid::new_v4());
    v.db.write(move |tx| {
        store::create_profile(tx, one, "Vorsichtig")?;
        store::create_profile(tx, two, "Vorsichtig")?;
        Ok(())
    })
    .await
    .expect("two profiles, one name");
    let names =
        v.db.read(|q| {
            Ok(store::profiles(q)?
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>())
        })
        .await
        .expect("read");
    assert_eq!(names, vec!["Vorsichtig", "Vorsichtig"]);
    let empty =
        v.db.write(|tx| store::create_profile(tx, Uuid::new_v4(), "  ").map_err(Into::into))
            .await;
    assert!(empty.is_err());
}

#[tokio::test]
async fn deleting_a_profile_removes_its_memberships_and_leaves_markers() {
    let v = vault();
    let (profile, a, b) = (Uuid::new_v4(), agent(), Subject::Model("m".to_owned()));
    let (sa, sb) = (a.clone(), b.clone());
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Weg")?;
        store::set_rights(tx, profile, &reading("chat.read"))?;
        store::add_member(tx, &sa, profile)?;
        store::add_member(tx, &sb, profile)?;
        Ok(())
    })
    .await
    .expect("set up");
    let moved =
        v.db.write(move |tx| store::delete_profile(tx, profile).map_err(Into::into))
            .await
            .expect("delete");
    assert_eq!(moved, 2, "the page names this number before it deletes");
    let (row, grants, member, markers) = v
        .db
        .read(move |q| {
            let markers: Vec<String> = q.query_map(
                "SELECT table_name FROM haex_deleted_rows WHERE table_name LIKE 'haex_agent_profile%'",
                &[],
                |r| r.get(0),
            )?;
            Ok((
                store::profile(q, profile)?,
                store::rights_rows(q, profile)?,
                store::members(q, profile)?,
                markers,
            ))
        })
        .await
        .expect("read");
    assert!(row.is_none() && grants.is_empty() && member.is_empty());
    for table in [
        "haex_agent_profiles",
        "haex_agent_profile_grants",
        "haex_agent_profile_members",
    ] {
        assert!(markers.iter().any(|m| m == table), "{table} left a marker");
    }
}

#[tokio::test]
async fn revoking_an_agent_removes_its_memberships_and_keeps_the_profile() {
    let v = super::test_support::vault();
    let (device, agent, other) = (
        v.device,
        super::test_support::registered(&v, "claude-code").await,
        super::test_support::registered(&v, "codex").await,
    );
    let profile = Uuid::new_v4();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Geteilt")?;
        store::add_member(tx, &Subject::Agent(agent), profile)?;
        store::add_member(tx, &Subject::Agent(other), profile)
    })
    .await
    .expect("two agents share a profile");
    let removed =
        v.db.write(move |tx| crate::mcp_server::manage::revoke(tx, device, agent))
            .await
            .expect("revoke");
    assert!(removed);
    let (members, row) =
        v.db.read(move |q| Ok((store::members(q, profile)?, store::profile(q, profile)?)))
            .await
            .expect("read");
    assert_eq!(
        members,
        vec![Subject::Agent(other).key()],
        "only the other agent is left"
    );
    assert!(row.is_some(), "a profile outlives an agent");
}
