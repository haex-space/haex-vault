//! Tests for the resolver (spec 083 T009, data-model.md "Resolution"): the active profile applies to
//! the subjects that are members of it, "Standard" to everyone else.

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
async fn a_member_has_the_rights_of_the_active_profile_and_others_have_those_of_standard() {
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
        store::add_member(tx, &m, profile)?;
        store::set_active(tx, profile)?;
        Ok(())
    })
    .await
    .expect("set up");
    let (a, b) =
        v.db.read(move |q| Ok((rights_of(q, &member)?, rights_of(q, &other)?)))
            .await
            .expect("read");
    assert_eq!(
        a,
        Rights::default(),
        "nothing granted in the active profile"
    );
    assert_eq!(
        b,
        default_rights(),
        "a non-member has the rights of Standard"
    );
}

#[tokio::test]
async fn a_profile_that_is_not_active_gives_its_members_the_rights_of_standard() {
    let v = vault();
    let (strict, open, subject) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Subject::Model("m".to_owned()),
    );
    let s = subject.clone();
    v.db.write(move |tx| {
        store::create_profile(tx, strict, "Streng")?;
        store::set_rights(tx, strict, &Rights::default())?;
        store::create_profile(tx, open, "Offen")?;
        store::set_rights(tx, open, &writing("chat.write"))?;
        // The same model is a member of both; only the active one counts.
        store::add_member(tx, &s, strict)?;
        store::add_member(tx, &s, open)?;
        Ok(())
    })
    .await
    .expect("set up");
    let in_standard = {
        let s = subject.clone();
        v.db.read(move |q| rights_of(q, &s)).await.expect("read")
    };
    assert_eq!(in_standard, default_rights());
    for (active, expected) in [(strict, Rights::default()), (open, writing("chat.write"))] {
        v.db.write(move |tx| store::set_active(tx, active))
            .await
            .expect("activate");
        let s = subject.clone();
        let rights = v.db.read(move |q| rights_of(q, &s)).await.expect("read");
        assert_eq!(normalized(&rights), normalized(&expected));
    }
}

#[tokio::test]
async fn an_active_profile_that_is_gone_means_standard() {
    let v = vault();
    let (gone, subject) = (Uuid::new_v4(), Subject::Agent(Uuid::new_v4()));
    let s = subject.clone();
    v.db.write(move |tx| {
        // The active value and the member row point at a profile that has no row (deleted
        // elsewhere, not synced yet).
        store::add_member(tx, &s, gone)?;
        store::set_active(tx, gone)
    })
    .await
    .expect("write");
    let rights =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert_eq!(rights, default_rights());
}

#[tokio::test]
async fn a_member_row_this_build_cannot_read_gives_no_right_while_another_profile_is_active() {
    let v = vault();
    let (profile, subject) = (Uuid::new_v4(), Subject::Agent(Uuid::new_v4()));
    let (s, key) = (subject.clone(), subject.key());
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Streng")?;
        store::set_rights(tx, profile, &writing("chat.write"))?;
        store::set_active(tx, profile)?;
        // A row of the subject that names no profile and whose id is not the one a membership has.
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES ('x', ?1, 'no-uuid')",
            params![key],
        )?;
        let _ = s;
        Ok(())
    })
    .await
    .expect("write");
    let rights =
        v.db.read({
            let subject = subject.clone();
            move |q| rights_of(q, &subject)
        })
        .await
        .expect("read");
    assert_eq!(
        rights,
        Rights::default(),
        "never a wider right than the row says"
    );
    // With "Standard" active the unreadable row still fails closed.
    v.db.write(|tx| store::set_active(tx, STANDARD_PROFILE_ID))
        .await
        .expect("activate Standard");
    let rights =
        v.db.read(move |q| rights_of(q, &subject))
            .await
            .expect("read");
    assert_eq!(rights, Rights::default());
}

#[tokio::test]
async fn an_unreadable_membership_overrides_a_valid_active_membership() {
    let v = vault();
    let (profile, subject) = (Uuid::new_v4(), Subject::Agent(Uuid::new_v4()));
    let (s, key) = (subject.clone(), subject.key());
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Aktiv")?;
        store::set_rights(tx, profile, &writing("chat.write"))?;
        store::add_member(tx, &s, profile)?;
        store::set_active(tx, profile)?;
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES ('bad-row', ?1, 'future-profile')",
            params![key],
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
async fn an_active_membership_id_that_names_another_subject_fails_closed() {
    let v = vault();
    let (profile, subject) = (Uuid::new_v4(), Subject::Agent(Uuid::new_v4()));
    let expected_id = member_id(&subject, profile);
    let wrong_subject = Subject::Agent(Uuid::new_v4());
    let wrong_key = wrong_subject.key();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Aktiv")?;
        store::set_rights(tx, profile, &writing("chat.write"))?;
        store::set_active(tx, profile)?;
        tx.execute(
            "INSERT INTO haex_agent_profile_members (id, subject, profile_id) VALUES (?1, ?2, ?3)",
            params![expected_id, wrong_key, profile.to_string()],
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
async fn an_active_value_this_build_cannot_read_gives_no_right() {
    let v = vault();
    v.db.write(|tx| {
        crate::storage::preferences::insert_or_update(
            tx,
            crate::storage::preferences::PrefScope::Vault,
            store::ACTIVE_KEY,
            "from-the-future",
        )?;
        Ok(())
    })
    .await
    .expect("write");
    let rights =
        v.db.read(|q| rights_of(q, &Subject::Model("m".to_owned())))
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
        store::add_member(tx, &s, profile)?;
        store::set_active(tx, profile)?;
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
        store::add_member(tx, &s, profile)?;
        store::set_active(tx, profile)?;
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

/// Whether the chat offers the model `id` now.
async fn available(v: &super::test_support::Vault, id: &str) -> bool {
    let id = id.to_owned();
    v.db.read(move |q| super::resolve::model_available(q, &id))
        .await
        .expect("read")
}

#[tokio::test]
async fn the_chat_offers_every_model_but_those_taken_out_of_standard_while_standard_is_active() {
    let v = vault();
    let left = Subject::Model("left".to_owned());
    let l = left.clone();
    v.db.write(move |tx| store::set_in_standard(tx, &l, false))
        .await
        .expect("take a model out of Standard");
    assert!(
        available(&v, "kept").await,
        "a model is in Standard without a row"
    );
    assert!(!available(&v, "left").await);
    assert!(
        available(&v, "brand-new").await,
        "a model that is added later is in Standard"
    );

    let l = left.clone();
    v.db.write(move |tx| store::set_in_standard(tx, &l, true))
        .await
        .expect("put it back");
    assert!(available(&v, "left").await);
}

#[tokio::test]
async fn while_another_profile_is_active_the_chat_offers_only_its_models() {
    let v = vault();
    let profile = Uuid::new_v4();
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Sensibel")?;
        store::add_member(tx, &Subject::Model("member".to_owned()), profile)?;
        // Out of Standard, in the profile: the profile alone decides while it is active.
        store::set_in_standard(tx, &Subject::Model("member".to_owned()), false)?;
        store::set_active(tx, profile)
    })
    .await
    .expect("setup");
    assert!(available(&v, "member").await);
    assert!(
        !available(&v, "other").await,
        "a model that is in Standard only is not offered in another profile"
    );

    // Back to Standard: the model that left it is gone, the other one is back.
    v.db.write(|tx| store::set_active(tx, STANDARD_PROFILE_ID))
        .await
        .expect("activate Standard");
    assert!(!available(&v, "member").await);
    assert!(available(&v, "other").await);

    // A profile that has no row any more counts as Standard, as for the rights.
    v.db.write(move |tx| {
        store::set_active(tx, profile)?;
        tx.execute(
            "DELETE FROM haex_agent_profiles WHERE id = ?1",
            params![profile.to_string()],
        )?;
        Ok(())
    })
    .await
    .expect("delete the active profile's row");
    assert!(available(&v, "other").await);
}

#[tokio::test]
async fn how_an_abo_agents_tools_run_is_the_right_of_the_profile_of_its_model() {
    use crate::adapters::cli_delegate::autonomy::{AutonomyMode, DenyCategory};
    use crate::mcp_server::grants::DelegateRights;
    let v = vault();
    let (profile, member) = (Uuid::new_v4(), "provider:sonnet");
    let strict = Rights {
        delegate: Some(DelegateRights {
            autonomy: AutonomyMode::GatedPermissive,
            deny: [DenyCategory::NetworkAccess].into_iter().collect(),
        }),
        ..Rights::default()
    };
    v.db.write(move |tx| {
        store::create_profile(tx, profile, "Streng")?;
        store::set_rights(tx, profile, &strict)?;
        store::add_member(tx, &Subject::Model(member.to_owned()), profile)?;
        store::set_active(tx, profile)
    })
    .await
    .expect("setup");
    let of = |model: &'static str| {
        let db = v.db.clone();
        async move {
            db.read(move |q| super::resolve::delegate_rights(q, model))
                .await
                .expect("read")
        }
    };
    let strict = of(member).await;
    assert_eq!(strict.autonomy, AutonomyMode::GatedPermissive);
    assert!(strict.deny.contains(&DenyCategory::NetworkAccess));
    // A model that is not in the active profile has the value of Standard: its factory autonomy.
    assert_eq!(of("provider:opus").await.autonomy, AutonomyMode::Ungated);

    // A profile without the group asks for every call.
    v.db.write(move |tx| store::set_rights(tx, profile, &Rights::default()))
        .await
        .expect("a profile without the group");
    assert_eq!(of(member).await, DelegateRights::asking());
}
