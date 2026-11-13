//! Service-level tests of passes (spec 092, T026): a pass is written in one transaction with all its
//! parts, the start has one shape at a time, the conflict token works, the type never changes,
//! and a missing detail row reads as `other`.

// The tests read and write raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use std::sync::Arc;

use haex_crdt::Database;

use super::model::{CodeInput, CodeView, PassInput, PassKind, PassPatch, Start};
use crate::error::HolziError;
use crate::passwords::access::Caller;
use crate::passwords::groups;
use crate::passwords::kind::EntryType;
use crate::passwords::model::{ItemInput, ItemPatch, KeyValueInput, Patch};
use crate::passwords::service::PasswordsService;
use crate::passwords::test_support::open_test_vault;
use crate::vault_gate::VaultGate;

const MARKER: &str = "SECRET-MARKER-pass-service";

fn setup() -> (tempfile::TempDir, Arc<Database>, PasswordsService) {
    let (dir, db) = open_test_vault();
    let db = Arc::new(db);
    let vault = VaultGate::new().vault_db(db.clone()).expect("vault");
    (dir, db, PasswordsService::new(vault))
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn kind_of(db: &Database, id: &str) -> String {
    db.with_connection(|c| {
        Ok(c.query_row(
            "SELECT kind FROM haex_passwords_item_details WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?)
    })
    .expect("kind")
}

fn code(format: &str, value: &str, position: i64) -> CodeInput {
    CodeInput {
        id: None,
        format: format.to_owned(),
        value: value.to_owned(),
        label: None,
        position,
        flag: None,
    }
}

fn as_input(c: &CodeView) -> CodeInput {
    CodeInput {
        id: Some(c.id.clone()),
        format: c.format.clone(),
        value: c.value.clone(),
        label: c.label.clone(),
        position: c.position,
        flag: None,
    }
}

fn concert() -> PassInput {
    PassInput {
        title: Some("Concert".to_owned()),
        note: Some("Block B".to_owned()),
        url: Some("https://tickets.example.org".to_owned()),
        pass_kind: PassKind::Ticket,
        start: Some(Start::At {
            at: "2026-11-14T20:00:00+00:00".to_owned(),
            zone: "Europe/London".to_owned(),
        }),
        expires_at: Some("2026-11-15".to_owned()),
        codes: vec![code("qr", MARKER, 0), code("code128", "ABC-123", 1)],
        key_values: vec![KeyValueInput {
            key: "Seat".to_owned(),
            value: Some("14C".to_owned()),
        }],
        tag_names: vec!["Ticket".to_owned()],
        ..PassInput::default()
    }
}

fn stands(db: &Database, id: &str) -> usize {
    db.with_connection(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM haex_passwords_item_snapshots WHERE item_id = ?1",
            [id],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .expect("stands") as usize
}

fn token(detail: &crate::passwords::model::ItemDetail) -> String {
    detail.header.updated_at.clone().expect("token")
}

fn title(t: &str) -> PassPatch {
    PassPatch {
        title: Patch::Set(t.to_owned()),
        ..PassPatch::default()
    }
}

fn refusal(result: &Result<crate::passwords::model::ItemDetail, HolziError>) -> Option<&str> {
    match result {
        Err(HolziError::PassRefused { code, .. }) => Some(code),
        _ => None,
    }
}

#[tokio::test]
async fn creating_a_pass_writes_every_part_and_one_state() {
    let (_dir, db, service) = setup();
    let folder = db
        .write(|tx| {
            groups::create_group(
                tx,
                "Tickets",
                None,
                None,
                None,
                Some(crate::passwords::WALLET_GROUP_ID),
            )
            .map_err(Into::into)
        })
        .expect("folder");
    let detail = service
        .pass_create(&Caller::User, concert(), Some(folder.clone()))
        .await
        .expect("create");

    assert_eq!(detail.header.kind, EntryType::Pass);
    assert_eq!(detail.header.pass_kind.as_deref(), Some("ticket"));
    assert_eq!(
        detail.header.start_at.as_deref(),
        Some("2026-11-14T20:00:00.000Z")
    );
    assert_eq!(detail.header.start_zone.as_deref(), Some("Europe/London"));
    assert_eq!(detail.header.start_date, None);
    assert_eq!(detail.header.group_id.as_deref(), Some(folder.as_str()));
    assert_eq!(
        detail
            .header
            .tags
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        ["Ticket"]
    );
    assert_eq!(detail.key_values.len(), 1);
    assert_eq!(detail.header.expires_at.as_deref(), Some("2026-11-15"));
    // The header carries no code, in any field.
    assert!(!format!("{detail:?}").contains(MARKER));

    let id = detail.header.id.clone();
    // The login columns of a pass are empty.
    let login_columns: (Option<String>, Option<String>, Option<String>) = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT username, password, otp_secret FROM haex_passwords_item_details \
                 WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .expect("login columns");
    assert_eq!(login_columns, (None, None, None));
    assert_eq!(stands(&db, &id), 1, "one state, already with the codes");

    let codes = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes");
    assert_eq!(
        codes
            .iter()
            .map(|c| (c.format.as_str(), c.position))
            .collect::<Vec<_>>(),
        [("qr", 0), ("code128", 1)]
    );
    assert_eq!(codes[0].value, MARKER);
}

#[tokio::test]
async fn the_start_is_a_date_or_a_moment_never_both() {
    let (_dir, _db, service) = setup();
    let detail = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("create");
    let id = detail.header.id.clone();

    let on_a_date = service
        .pass_update(
            &Caller::User,
            id.clone(),
            token(&detail),
            PassPatch {
                start: Patch::Set(Start::Date {
                    date: "2026-12-24".to_owned(),
                }),
                ..PassPatch::default()
            },
        )
        .await
        .expect("a date");
    assert_eq!(on_a_date.header.start_date.as_deref(), Some("2026-12-24"));
    assert_eq!(
        (&on_a_date.header.start_at, &on_a_date.header.start_zone),
        (&None, &None)
    );

    let at_a_moment = service
        .pass_update(
            &Caller::User,
            id.clone(),
            token(&on_a_date),
            PassPatch {
                start: Patch::Set(Start::At {
                    at: "2026-12-24T18:30:00+01:00".to_owned(),
                    zone: "+01:00".to_owned(),
                }),
                ..PassPatch::default()
            },
        )
        .await
        .expect("a moment");
    assert_eq!(at_a_moment.header.start_date, None);
    assert_eq!(
        at_a_moment.header.start_at.as_deref(),
        Some("2026-12-24T17:30:00.000Z")
    );
    assert_eq!(at_a_moment.header.start_zone.as_deref(), Some("+01:00"));

    let none = service
        .pass_update(
            &Caller::User,
            id,
            token(&at_a_moment),
            PassPatch {
                start: Patch::Clear,
                ..PassPatch::default()
            },
        )
        .await
        .expect("cleared");
    assert_eq!(
        (
            none.header.start_date,
            none.header.start_at,
            none.header.start_zone
        ),
        (None, None, None)
    );
}

#[tokio::test]
async fn a_start_that_is_no_date_or_moment_is_refused() {
    let (_dir, _db, service) = setup();
    for start in [
        Start::Date {
            date: "2026-02-30".to_owned(),
        },
        Start::Date {
            date: "tomorrow".to_owned(),
        },
        Start::At {
            at: "soon".to_owned(),
            zone: "Europe/London".to_owned(),
        },
        // A day that does not exist must not roll over into the next month.
        Start::At {
            at: "2026-02-30T20:00:00Z".to_owned(),
            zone: "Europe/London".to_owned(),
        },
        Start::At {
            at: "2026-11-14T20:00:00Z".to_owned(),
            zone: "  ".to_owned(),
        },
    ] {
        let result = service
            .pass_create(
                &Caller::User,
                PassInput {
                    start: Some(start),
                    ..PassInput::default()
                },
                None,
            )
            .await;
        assert!(
            matches!(result, Err(HolziError::InvalidInput { .. })),
            "{result:?}"
        );
    }
}

#[tokio::test]
async fn an_old_token_is_a_conflict_and_a_gone_entry_is_deleted() {
    let (_dir, db, service) = setup();
    let detail = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("create");
    let id = detail.header.id.clone();
    let first = token(&detail);
    service
        .pass_update(&Caller::User, id.clone(), first.clone(), title("Concert 2"))
        .await
        .expect("first update");
    let stale = service
        .pass_update(&Caller::User, id.clone(), first, title("Concert 3"))
        .await;
    assert!(
        matches!(&stale, Err(HolziError::PasswordsConflict { reason }) if reason == "changed"),
        "{stale:?}"
    );

    db.with_connection(|c| {
        c.execute(
            "DELETE FROM haex_passwords_item_details WHERE id = ?1",
            [&id],
        )?;
        Ok(())
    })
    .expect("delete");
    let gone = service
        .pass_update(&Caller::User, id, "x".to_owned(), title("Concert 4"))
        .await;
    assert!(
        matches!(&gone, Err(HolziError::PasswordsConflict { reason }) if reason == "deleted"),
        "{gone:?}"
    );
}

#[tokio::test]
async fn the_type_of_an_entry_never_changes() {
    let (_dir, db, service) = setup();
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    let login_id = service
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some("Bank".to_owned()),
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("login");
    // The login command makes a login.
    assert_eq!(kind_of(&db, &login_id), "login");

    // A login is not written as a pass, a pass is not written as a login.
    let as_pass = service
        .pass_update(
            &Caller::User,
            login_id,
            "x".to_owned(),
            PassPatch::default(),
        )
        .await;
    assert_eq!(refusal(&as_pass), Some("not_a_pass"), "{as_pass:?}");
    let as_login = service
        .update_item(
            &Caller::User,
            &[],
            pass.header.id.clone(),
            token(&pass),
            ItemPatch {
                title: Patch::Set("changed".to_owned()),
                ..ItemPatch::default()
            },
        )
        .await;
    assert!(
        matches!(&as_login, Err(HolziError::PassRefused { code, .. }) if code == "not_a_login"),
        "{as_login:?}"
    );

    // An entry of a type of a newer build is never written over.
    db.with_connection(|c| {
        c.execute(
            "UPDATE haex_passwords_item_details SET kind = 'payment_card' WHERE id = ?1",
            [&pass.header.id],
        )?;
        Ok(())
    })
    .expect("newer type");
    let unknown = service
        .pass_update(
            &Caller::User,
            pass.header.id.clone(),
            token(&pass),
            PassPatch::default(),
        )
        .await;
    assert_eq!(refusal(&unknown), Some("unknown_kind"), "{unknown:?}");
    assert_eq!(kind_of(&db, &pass.header.id), "payment_card");
}

#[tokio::test]
async fn a_pass_without_a_detail_row_reads_as_other_and_the_next_write_repairs_it() {
    let (_dir, db, service) = setup();
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    let id = pass.header.id.clone();
    db.with_connection(|c| {
        c.execute(
            "DELETE FROM haex_passwords_pass_details WHERE item_id = ?1",
            [&id],
        )?;
        Ok(())
    })
    .expect("lose the row");

    let read = service
        .get_item(&Caller::User, id.clone())
        .await
        .expect("read");
    assert_eq!(read.header.pass_kind.as_deref(), Some("other"));
    assert_eq!(read.header.start_at, None);

    service
        .pass_update(&Caller::User, id.clone(), token(&read), title("Repaired"))
        .await
        .expect("repair");
    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM haex_passwords_pass_details WHERE item_id = '{id}'")
        ),
        1
    );
}

#[tokio::test]
async fn a_kind_this_build_does_not_know_is_kept_and_written_back() {
    let (_dir, _db, service) = setup();
    let pass = service
        .pass_create(
            &Caller::User,
            PassInput {
                pass_kind: PassKind::parse("season_ticket"),
                ..PassInput::default()
            },
            None,
        )
        .await
        .expect("pass");
    assert_eq!(pass.header.pass_kind.as_deref(), Some("season_ticket"));
    let edited = service
        .pass_update(
            &Caller::User,
            pass.header.id.clone(),
            token(&pass),
            title("Renamed"),
        )
        .await
        .expect("edit");
    assert_eq!(edited.header.pass_kind.as_deref(), Some("season_ticket"));
}

#[tokio::test]
async fn a_code_typed_by_hand_must_fit_its_format() {
    let (_dir, db, service) = setup();
    let refused = service
        .pass_create(
            &Caller::User,
            PassInput {
                codes: vec![code("ean13", "4006381333932", 3)],
                ..PassInput::default()
            },
            None,
        )
        .await;
    match refused {
        Err(HolziError::PassRefused {
            code,
            position,
            reason,
        }) => {
            assert_eq!(code, "code_invalid");
            assert_eq!(position, Some(3));
            assert_eq!(reason.as_deref(), Some("bad_check_digit"));
        }
        other => panic!("{other:?}"),
    }
    let unknown = service
        .pass_create(
            &Caller::User,
            PassInput {
                codes: vec![code("code39", "ABC", 0)],
                ..PassInput::default()
            },
            None,
        )
        .await;
    assert!(
        matches!(&unknown, Err(HolziError::PassRefused { reason, .. }) if reason.as_deref() == Some("unknown_format")),
        "{unknown:?}"
    );
    // Nothing was left behind by the refused creations.
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        0
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_barcodes"),
        0
    );
}

#[tokio::test]
async fn the_error_of_a_refused_code_never_holds_its_value() {
    let (_dir, _db, service) = setup();
    let refused = service
        .pass_create(
            &Caller::User,
            PassInput {
                codes: vec![code("ean13", MARKER, 0)],
                ..PassInput::default()
            },
            None,
        )
        .await
        .expect_err("refused");
    assert!(!format!("{refused:?} {refused}").contains(MARKER));
}

#[tokio::test]
async fn codes_are_replaced_by_id_so_a_kept_code_keeps_its_row() {
    let (_dir, db, service) = setup();
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    let id = pass.header.id.clone();
    let before = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes");
    let (first, second) = (before[0].clone(), before[1].clone());

    let patch = PassPatch {
        codes: Some(vec![
            // The first code keeps its row and gets a label; the second is gone; one is new.
            CodeInput {
                label: Some("Entry".to_owned()),
                ..as_input(&first)
            },
            code("aztec", "AZTEC-1", 1),
        ]),
        ..PassPatch::default()
    };
    service
        .pass_update(&Caller::User, id.clone(), token(&pass), patch)
        .await
        .expect("update");

    let after = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes");
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].id, first.id, "the kept code keeps its row");
    assert_eq!(after[0].label.as_deref(), Some("Entry"));
    assert_eq!(after[1].format, "aztec");
    assert!(
        after.iter().all(|c| c.id != second.id),
        "the dropped code is gone"
    );
    assert_eq!(stands(&db, &id), 2, "the edit is one more state");
}

#[tokio::test]
async fn a_flagged_code_stays_as_it_is_until_it_is_edited() {
    let (_dir, db, service) = setup();
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    let id = pass.header.id.clone();
    // Make the second code arrive the way sync delivers a code that does not fit.
    let stored = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes");
    db.with_connection(|c| {
        c.execute(
            "UPDATE haex_passwords_pass_barcodes SET format = 'ean13', value = '123', \
             flag = 'invalid_value' WHERE id = ?1",
            [&stored[1].id],
        )?;
        Ok(())
    })
    .expect("flag it");
    let bad = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes")[1]
        .clone();
    assert_eq!(bad.flag.as_deref(), Some("invalid_value"));

    // Saving the list with the code unchanged is not refused and keeps the flag.
    let current = service
        .get_item(&Caller::User, id.clone())
        .await
        .expect("get");
    service
        .pass_update(
            &Caller::User,
            id.clone(),
            token(&current),
            PassPatch {
                codes: Some(vec![
                    as_input(&stored[0]),
                    CodeInput {
                        label: Some("kept".to_owned()),
                        ..as_input(&bad)
                    },
                ]),
                ..PassPatch::default()
            },
        )
        .await
        .expect("an unchanged flagged code is kept");
    let kept = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes")[1]
        .clone();
    assert_eq!(kept.flag.as_deref(), Some("invalid_value"));
    assert_eq!(kept.label.as_deref(), Some("kept"));

    // Repairing it checks the new value and clears the flag.
    let current = service
        .get_item(&Caller::User, id.clone())
        .await
        .expect("get");
    service
        .pass_update(
            &Caller::User,
            id.clone(),
            token(&current),
            PassPatch {
                codes: Some(vec![
                    as_input(&stored[0]),
                    CodeInput {
                        value: "4006381333931".to_owned(),
                        ..as_input(&bad)
                    },
                ]),
                ..PassPatch::default()
            },
        )
        .await
        .expect("repair");
    let repaired = service.pass_codes(&Caller::User, id).await.expect("codes")[1].clone();
    assert_eq!(repaired.flag, None);
    assert_eq!(repaired.value, "4006381333931");
}

#[tokio::test]
async fn a_pass_cannot_be_its_own_parent_or_the_parent_of_its_parent() {
    let (_dir, _db, service) = setup();
    let a = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("a");
    let b = service
        .pass_create(
            &Caller::User,
            PassInput {
                parent_id: Some(a.header.id.clone()),
                ..PassInput::default()
            },
            None,
        )
        .await
        .expect("b under a");
    assert_eq!(b.header.parent_id.as_deref(), Some(a.header.id.as_str()));

    let under = |parent: &str| PassPatch {
        parent_id: Patch::Set(parent.to_owned()),
        ..PassPatch::default()
    };
    let own = service
        .pass_update(
            &Caller::User,
            a.header.id.clone(),
            token(&a),
            under(&a.header.id),
        )
        .await;
    assert_eq!(refusal(&own), Some("parent_loop"), "{own:?}");
    let child = service
        .pass_update(
            &Caller::User,
            a.header.id.clone(),
            token(&a),
            under(&b.header.id),
        )
        .await;
    assert_eq!(refusal(&child), Some("parent_loop"), "{child:?}");

    // A parent that is not in the vault yet is allowed: the link is soft.
    let soft = service
        .pass_update(
            &Caller::User,
            a.header.id.clone(),
            token(&a),
            under("not-synced-yet"),
        )
        .await
        .expect("a soft link");
    assert_eq!(soft.header.parent_id.as_deref(), Some("not-synced-yet"));
}

#[tokio::test]
async fn only_the_user_reaches_the_pass_functions() {
    let (_dir, _db, service) = setup();
    let agent = Caller::ExternalAgent {
        id: "agent".to_owned(),
    };
    assert!(service.pass_create(&agent, concert(), None).await.is_err());
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    assert!(service
        .pass_codes(&agent, pass.header.id.clone())
        .await
        .is_err());
    assert!(service
        .pass_code_svg(&agent, pass.header.id, "x".to_owned())
        .await
        .is_err());
}

#[tokio::test]
async fn one_code_is_drawn_and_a_flagged_one_comes_back_as_text() {
    let (_dir, db, service) = setup();
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    let id = pass.header.id.clone();
    let codes = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes");
    let drawn = service
        .pass_code_svg(&Caller::User, id.clone(), codes[0].id.clone())
        .await
        .expect("svg");
    assert!(drawn.svg.as_deref().is_some_and(|s| s.starts_with("<svg")));
    assert!(drawn.text.is_none() && drawn.quiet_zone == 4);

    db.with_connection(|c| {
        c.execute(
            "UPDATE haex_passwords_pass_barcodes SET flag = 'invalid_value' WHERE id = ?1",
            [&codes[1].id],
        )?;
        Ok(())
    })
    .expect("flag");
    let text = service
        .pass_code_svg(&Caller::User, id.clone(), codes[1].id.clone())
        .await
        .expect("text");
    assert!(text.svg.is_none());
    assert_eq!(text.text.as_deref(), Some("ABC-123"));

    let missing = service
        .pass_code_svg(&Caller::User, id, "nope".to_owned())
        .await;
    assert!(matches!(missing, Err(HolziError::PasswordsNotFound)));
}

#[tokio::test]
async fn one_stored_code_cannot_be_given_two_values_in_one_list() {
    let (_dir, _db, service) = setup();
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    let id = pass.header.id.clone();
    let stored = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes");
    let twice = PassPatch {
        codes: Some(vec![
            CodeInput {
                value: "ABC-1".to_owned(),
                ..as_input(&stored[1])
            },
            CodeInput {
                value: "ABC-2".to_owned(),
                position: 1,
                ..as_input(&stored[1])
            },
        ]),
        ..PassPatch::default()
    };
    let result = service
        .pass_update(&Caller::User, id.clone(), token(&pass), twice)
        .await;
    assert_eq!(refusal(&result), Some("duplicate_code"), "{result:?}");
    // Nothing was written.
    let after = service.pass_codes(&Caller::User, id).await.expect("codes");
    assert_eq!(after.len(), 2);
    assert_eq!(after[1].value, "ABC-123");
}

#[tokio::test]
async fn a_flagged_code_can_be_copied_into_a_new_pass_but_a_flag_cannot_be_forged() {
    let (_dir, db, service) = setup();
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    let id = pass.header.id.clone();
    let stored = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes");
    db.with_connection(|c| {
        c.execute(
            "UPDATE haex_passwords_pass_barcodes SET format = 'ean13', value = '123', \
             flag = 'invalid_value' WHERE id = ?1",
            [&stored[1].id],
        )?;
        Ok(())
    })
    .expect("flag it");
    let flagged = service
        .pass_codes(&Caller::User, id.clone())
        .await
        .expect("codes")[1]
        .clone();

    // "Save as new": the same code, with its id, keeps what it was, flag included.
    let copy = service
        .pass_create(
            &Caller::User,
            PassInput {
                codes: vec![as_input(&flagged)],
                ..PassInput::default()
            },
            None,
        )
        .await
        .expect("a flagged code can be copied");
    let copied = service
        .pass_codes(&Caller::User, copy.header.id.clone())
        .await
        .expect("codes");
    assert_eq!(copied[0].flag.as_deref(), Some("invalid_value"));
    assert_ne!(copied[0].id, flagged.id, "the copy is a row of its own");

    // The same id with another value is a new code and has to fit.
    let forged = service
        .pass_create(
            &Caller::User,
            PassInput {
                codes: vec![CodeInput {
                    value: "456".to_owned(),
                    ..as_input(&flagged)
                }],
                ..PassInput::default()
            },
            None,
        )
        .await;
    assert_eq!(refusal(&forged), Some("code_invalid"), "{forged:?}");
}

#[tokio::test]
async fn the_codes_come_with_the_drawing_of_the_value_beside_them() {
    let (_dir, _db, service) = setup();
    let pass = service
        .pass_create(&Caller::User, concert(), None)
        .await
        .expect("pass");
    let codes = service
        .pass_codes(&Caller::User, pass.header.id.clone())
        .await
        .expect("codes");
    assert!(codes.iter().all(|c| c
        .drawing
        .as_ref()
        .is_some_and(|d| d.svg.starts_with("<svg"))));
}
