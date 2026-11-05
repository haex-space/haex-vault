//! Tests for the resolver (spec 083 T009, data-model.md "Resolution").

use haex_crdt::rusqlite::params;
use uuid::Uuid;

use super::resolve::{normalized, rights_of};
use super::store;
use super::test_support::vault;
use super::{member_id, Subject, STANDARD_PROFILE_ID};
use crate::mcp_server::grants::{default_rights, OperateLevel, Rights};

fn writing(scope: &str) -> Rights {
    Rights {
        operate: [(scope.to_owned(), OperateLevel::Write)]
            .into_iter()
            .collect(),
        commands: true,
        ..Rights::default()
    }
}

#[tokio::test]
async fn standard_is_the_factory_rights_until_it_is_edited() {
    let v = vault();
    let subject = Subject::Agent(Uuid::new_v4());
    let s = subject.clone();
    let factory = v.db.read(move |q| rights_of(q, &s)).await.expect("read");
    assert_eq!(factory, default_rights());

    v.db.write(|tx| store::set_rights(tx, STANDARD_PROFILE_ID, &writing("chat.write")))
        .await
        .expect("edit Standard");
    let edited =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert_eq!(normalized(&edited), normalized(&writing("chat.write")));
}

#[tokio::test]
async fn an_edited_standard_with_nothing_granted_gives_nothing() {
    let v = vault();
    v.db.write(|tx| store::set_rights(tx, STANDARD_PROFILE_ID, &Rights::default()))
        .await
        .expect("edit Standard");
    let rights =
        v.db.read(|q| rights_of(q, &Subject::Model("m".to_owned())))
            .await
            .expect("read");
    assert_eq!(rights, Rights::default());
}

#[tokio::test]
async fn an_assigned_subject_has_the_rights_of_its_profile_and_others_do_not() {
    let v = vault();
    let (profile, member, other) = (
        Uuid::new_v4(),
        Subject::Agent(Uuid::new_v4()),
        Subject::Agent(Uuid::new_v4()),
    );
    let m = member.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Streng")?;
        store::set_rights(tx, profile, &Rights::default())?;
        store::assign(tx, &m, profile)?;
        Ok(())
    })
    .await
    .expect("set up");
    let (a, b) =
        v.db.read(move |q| Ok((rights_of(q, &member)?, rights_of(q, &other)?)))
            .await
            .expect("read");
    assert_eq!(a, Rights::default(), "nothing granted in the profile");
    assert_eq!(b, default_rights(), "an agent in no profile is in Standard");
}

#[tokio::test]
async fn a_dangling_profile_means_standard() {
    let v = vault();
    let subject = Subject::Agent(Uuid::new_v4());
    let s = subject.clone();
    v.db.write(move |tx| {
        // The member row points at a profile that has no row (deleted elsewhere, not synced yet).
        store::assign(tx, &s, Uuid::new_v4())
    })
    .await
    .expect("assign");
    let rights =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert_eq!(rights, default_rights());
}

#[tokio::test]
async fn a_member_row_this_build_cannot_read_gives_no_right() {
    let v = vault();
    let subject = Subject::Agent(Uuid::new_v4());
    let (id, key) = (member_id(&subject), subject.key());
    v.db.write(move |tx| {
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES (?1, ?2, 'no-uuid')",
            params![id, key],
        )?;
        Ok(())
    })
    .await
    .expect("write");
    let rights =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert_eq!(
        rights,
        Rights::default(),
        "never a wider right than the row says"
    );
}

#[tokio::test]
async fn a_member_row_with_a_mismatched_subject_fails_closed() {
    let v = vault();
    let subject = Subject::Agent(Uuid::new_v4());
    let id = member_id(&subject);
    v.db.write(move |tx| {
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES (?1, 'agent:someone-else', 'bad')",
            params![id],
        )?;
        Ok(())
    })
    .await
    .expect("write");
    let rights =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert_eq!(rights, Rights::default());
}

#[tokio::test]
async fn rights_rows_this_build_cannot_read_are_skipped() {
    let v = vault();
    let (profile, subject) = (Uuid::new_v4(), Subject::Agent(Uuid::new_v4()));
    let s = subject.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Neu")?;
        store::assign(tx, &s, profile)?;
        for (id, group, key, level) in [
            ("1", "operate", "wm.read", "read"),
            ("2", "from-the-future", "x", "on"),
            ("3", "operate", "wm.layout", "from-the-future"),
            ("4", "operate", "files.shares", "write"),
            ("5", "commands", "shell", "on"),
        ] {
            tx.execute(
                "INSERT INTO haex_agent_profile_grants (id, profile_id, grant_group, grant_key, level, scope_tags) \
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![id, profile.to_string(), group, key, level],
            )?;
        }
        Ok(())
    })
    .await
    .expect("write");
    let rights =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert_eq!(
        rights.operate,
        [("wm.read".to_owned(), OperateLevel::Read)]
            .into_iter()
            .collect(),
        "only the scope and level this build knows"
    );
    assert!(!rights.commands, "a commands row with a key is no right");
}

#[tokio::test]
async fn a_blob_scope_tag_skips_only_its_own_grant_row() {
    let v = vault();
    let (profile, subject) = (Uuid::new_v4(), Subject::Agent(Uuid::new_v4()));
    let s = subject.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Neu")?;
        store::assign(tx, &s, profile)?;
        tx.execute(
            "INSERT INTO haex_agent_profile_grants (id, profile_id, grant_group, grant_key, level, scope_tags) \
             VALUES ('valid', ?1, 'operate', 'chat.read', 'read', NULL)",
            params![profile.to_string()],
        )?;
        tx.execute(
            "INSERT INTO haex_agent_profile_grants (id, profile_id, grant_group, grant_key, level, scope_tags) \
             VALUES ('bad', ?1, 'passwords', '', 'read', x'00')",
            params![profile.to_string()],
        )?;
        Ok(())
    })
    .await
    .expect("write");
    let rights =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert_eq!(
        rights.operate,
        [("chat.read".to_owned(), OperateLevel::Read)]
            .into_iter()
            .collect()
    );
    assert!(rights.passwords.is_none());
}
