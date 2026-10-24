//! Received shares (FR-030, FR-031): an answer replaces only what its device said before.

// Rows are written directly; the CRDT write path is what `apply_answer` uses anyway.
#![allow(clippy::disallowed_methods)]

use super::{apply_answer, get, of_vault, received_id, Answerer};
use crate::files::peer::wire::WireShare;
use crate::files::share::{FileShareItemKind, FileShareLevel};
use crate::remote_storage::test_support::vault;

const ANNA: &str = "aa00000000000000000000000000000000000000000000000000000000000000";
const LAPTOP: &str = "1100000000000000000000000000000000000000000000000000000000000000";
const PHONE: &str = "2200000000000000000000000000000000000000000000000000000000000000";

fn share(id: &str, name: &str) -> WireShare {
    WireShare {
        id: id.to_owned(),
        name: name.to_owned(),
        item_kind: FileShareItemKind::Folder,
        level: FileShareLevel::Read,
    }
}

fn from(device: &'static str) -> Answerer<'static> {
    Answerer {
        vault: ANNA,
        device,
        endpoint: device,
    }
}

#[test]
fn an_answer_replaces_only_what_its_device_said_before() {
    let (_dir, db) = vault();
    db.write_blocking(|tx| {
        apply_answer(
            tx,
            from(LAPTOP),
            &[share("s1", "Fotos"), share("s2", "Musik")],
            "t1",
        )?;
        apply_answer(tx, from(PHONE), &[share("s3", "Notizen")], "t1")?;
        Ok(())
    })
    .expect("first answers");

    db.write_blocking(|tx| {
        let mut renamed = share("s1", "Urlaub");
        renamed.level = FileShareLevel::ReadWrite;
        Ok(apply_answer(tx, from(LAPTOP), &[renamed], "t2")?)
    })
    .expect("the laptop answers again");

    let rows = db.read_blocking(|q| Ok(of_vault(q, ANNA)?)).expect("rows");
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        ["Notizen", "Urlaub"],
        "s2 gone, the phone's share stays"
    );
    let fotos = db
        .read_blocking(|q| Ok(get(q, &received_id(ANNA, "s1"))?))
        .expect("row")
        .expect("s1");
    assert_eq!(fotos.level, FileShareLevel::ReadWrite);
    assert_eq!(fotos.last_reached_at.as_deref(), Some("t2"));
}
