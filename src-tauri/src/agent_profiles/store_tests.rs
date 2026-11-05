//! Tests for the three tables of agent profiles (spec 083 T009): profiles, assignments, deletes.

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
fn the_member_id_is_a_function_of_the_subject_alone() {
    let subject = agent();
    assert_eq!(member_id(&subject), member_id(&subject.clone()));
    assert_ne!(member_id(&subject), member_id(&agent()));
}

#[tokio::test]
async fn assigning_a_subject_twice_leaves_one_member_row() {
    let v = vault();
    let (one, two, subject) = (Uuid::new_v4(), Uuid::new_v4(), agent());
    let s = subject.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, one, "Eins")?;
        store::create_profile(tx, two, "Zwei")?;
        store::assign(tx, &s, one)?;
        store::assign(tx, &s, two)?;
        Ok(())
    })
    .await
    .expect("assign");
    let (in_one, in_two) =
        v.db.read(move |q| Ok((store::members(q, one)?, store::members(q, two)?)))
            .await
            .expect("read");
    assert!(in_one.is_empty(), "the subject moved out of the first");
    assert_eq!(in_two, vec![subject.key()]);
}

#[tokio::test]
async fn assigning_to_standard_removes_the_member_row() {
    let v = vault();
    let (profile, subject) = (Uuid::new_v4(), agent());
    let s = subject.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Eins")?;
        store::assign(tx, &s, profile)?;
        store::assign(tx, &s, STANDARD_PROFILE_ID)?;
        Ok(())
    })
    .await
    .expect("assign");
    let row =
        v.db.read(move |q| store::member_profile(q, &subject))
            .await
            .expect("read");
    assert_eq!(row, None, "no row means Standard");
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
async fn deleting_a_profile_moves_its_members_and_leaves_markers() {
    let v = vault();
    let (profile, a, b) = (Uuid::new_v4(), agent(), Subject::Model("m".to_owned()));
    let (sa, sb) = (a.clone(), b.clone());
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Weg")?;
        store::set_rights(tx, profile, &reading("chat.read"))?;
        store::assign(tx, &sa, profile)?;
        store::assign(tx, &sb, profile)?;
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
                store::member_profile(q, &a)?,
                markers,
            ))
        })
        .await
        .expect("read");
    assert!(row.is_none() && grants.is_empty() && member.is_none());
    for table in [
        "haex_agent_profiles",
        "haex_agent_profile_grants",
        "haex_agent_profile_members",
    ] {
        assert!(markers.iter().any(|m| m == table), "{table} left a marker");
    }
}
