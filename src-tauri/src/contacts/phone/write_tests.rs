use std::collections::BTreeMap;

use super::*;

fn value(n: usize) -> HolziValue {
    HolziValue {
        id: format!("v{n}"),
        kind: "phone".to_owned(),
        label: "mobile".to_owned(),
        value: format!("0170 {n:07}"),
        millis: None,
    }
}

#[test]
fn a_new_raw_contact_and_its_rows_share_one_call() {
    let mut batches = Batches::default();
    batches.change(
        "c0",
        (0..399)
            .map(|n| (1, RowOp::Delete { data_id: n }))
            .collect(),
    );
    let head = BTreeMap::from([("given_name".to_owned(), "Anna".to_owned())]);
    batches.create(
        "c1",
        ("com.google", "mara@example.org"),
        &head,
        &[value(1), value(2)],
    );
    assert_eq!(
        batches.calls.len(),
        2,
        "the create does not fit into the first call"
    );
    let call = &batches.calls[1];
    assert_eq!(
        call.ops.len(),
        4,
        "the raw contact, its name and two numbers"
    );
    assert!(
        matches!(&call.ops[0], Op::InsertRaw { reference: 0, account_type, .. } if account_type == "com.google")
    );
    assert!(call.ops[1..].iter().all(|op| matches!(
        op,
        Op::InsertData {
            raw: Target::Ref(0),
            ..
        }
    )));
    assert_eq!(
        call.purposes[0],
        Purpose::Create {
            contact_id: "c1".to_owned()
        }
    );
}

#[test]
fn no_call_has_more_than_the_limit() {
    let mut batches = Batches::default();
    batches.change(
        "c0",
        (0..1000)
            .map(|n| (1, RowOp::Delete { data_id: n }))
            .collect(),
    );
    for raw in 0..250 {
        batches.delete(raw);
    }
    assert!(batches.calls.iter().all(|call| call.ops.len() <= MAX_OPS));
    assert_eq!(
        batches
            .calls
            .iter()
            .map(|call| call.ops.len())
            .sum::<usize>(),
        1250
    );
}

#[test]
fn refs_count_within_one_call() {
    let mut batches = Batches::default();
    batches.create("c1", ("", ""), &BTreeMap::new(), &[value(1)]);
    batches.create("c2", ("", ""), &BTreeMap::new(), &[value(2)]);
    let call = &batches.calls[0];
    assert!(matches!(call.ops[0], Op::InsertRaw { reference: 0, .. }));
    assert!(matches!(call.ops[2], Op::InsertRaw { reference: 2, .. }));
    assert!(matches!(
        call.ops[3],
        Op::InsertData {
            raw: Target::Ref(2),
            ..
        }
    ));
}
