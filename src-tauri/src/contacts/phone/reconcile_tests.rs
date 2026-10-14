use std::collections::BTreeMap;

use super::*;
use crate::contacts::phone::store::BaseValue;

const RAW: i64 = 7;

fn holzi_value(id: &str, kind: &str, label: &str, value: &str, millis: Option<i64>) -> HolziValue {
    HolziValue {
        id: id.to_owned(),
        kind: kind.to_owned(),
        label: label.to_owned(),
        value: value.to_owned(),
        millis,
    }
}

fn phone_value(data_id: i64, kind: &str, label: &str, value: &str) -> PhoneValue {
    PhoneValue {
        data_id,
        kind: kind.to_owned(),
        label: label.to_owned(),
        value: value.to_owned(),
    }
}

/// The state both sides agreed on: Anna Beck with a mobile number (row 11) and an e-mail (row 12).
fn agreed() -> (HolziSide, PhoneSide, Base) {
    let head = BTreeMap::from([
        ("given_name".to_owned(), "Anna".to_owned()),
        ("family_name".to_owned(), "Beck".to_owned()),
    ]);
    let holzi = HolziSide {
        head: head.clone(),
        head_millis: BTreeMap::new(),
        values: vec![
            holzi_value("v-phone", "phone", "mobile", "0170 1", Some(1_000)),
            holzi_value("v-mail", "email", "home", "anna@example.org", Some(1_000)),
        ],
    };
    let phone = PhoneSide {
        head: head.clone(),
        head_rows: BTreeMap::from([("StructuredName".to_owned(), 10)]),
        values: vec![
            phone_value(11, "phone", "mobile", "0170 1"),
            phone_value(12, "email", "home", "anna@example.org"),
        ],
    };
    let base = Base {
        holzi_updated_at: "t1".to_owned(),
        holzi_head: head,
        holzi_values: BTreeMap::from([
            (
                "v-phone".to_owned(),
                BaseValue {
                    kind: "phone".to_owned(),
                    label: "mobile".to_owned(),
                    value: "0170 1".to_owned(),
                },
            ),
            (
                "v-mail".to_owned(),
                BaseValue {
                    kind: "email".to_owned(),
                    label: "home".to_owned(),
                    value: "anna@example.org".to_owned(),
                },
            ),
        ]),
        phone: phone.clone(),
        mapping: BTreeMap::from([(11, "v-phone".to_owned()), (12, "v-mail".to_owned())]),
    };
    (holzi, phone, base)
}

fn raw(base: Base, current: PhoneSide, last_updated: Option<i64>) -> RawSide {
    RawSide {
        raw_contact_id: RAW,
        writable: true,
        base,
        current: Some(current),
        last_updated,
    }
}

fn set_phone(side: &mut PhoneSide, data_id: i64, value: &str) {
    side.values
        .iter_mut()
        .find(|row| row.data_id == data_id)
        .expect("the row")
        .value = value.to_owned();
}

fn set_holzi(side: &mut HolziSide, id: &str, value: &str, millis: Option<i64>) {
    let found = side
        .values
        .iter_mut()
        .find(|value| value.id == id)
        .expect("the value");
    found.value = value.to_owned();
    found.millis = millis;
}

#[test]
fn nothing_changed_plans_nothing() {
    let (holzi, phone, base) = agreed();
    assert_eq!(
        reconcile(&holzi, &[raw(base, phone, Some(5))], Some(RAW), false),
        Plan::default()
    );
}

#[test]
fn changes_of_different_fields_on_both_sides_both_stay() {
    let (mut holzi, mut phone, base) = agreed();
    set_phone(&mut phone, 11, "0170 2");
    set_holzi(&mut holzi, "v-mail", "anna@beck.example", Some(2_000));
    let plan = reconcile(&holzi, &[raw(base, phone, Some(9_000))], Some(RAW), false);
    assert_eq!(
        plan.updates,
        vec![(
            "v-phone".to_owned(),
            "mobile".to_owned(),
            "0170 2".to_owned()
        )]
    );
    assert_eq!(plan.ops.len(), 1);
    let (
        raw_id,
        RowOp::Update {
            data_id, fields, ..
        },
    ) = &plan.ops[0]
    else {
        panic!("an update")
    };
    assert_eq!((*raw_id, *data_id), (RAW, 12));
    assert_eq!(
        fields.get("value").and_then(|v| v.as_str()),
        Some("anna@beck.example")
    );
}

#[test]
fn the_same_field_changed_on_both_sides_goes_to_the_later_change() {
    // The phone changed later: its value comes to holzi as a change of this device.
    let (mut holzi, mut phone, base) = agreed();
    set_phone(&mut phone, 11, "0170 1111111");
    set_holzi(&mut holzi, "v-phone", "0170 2222222", Some(2_000));
    let plan = reconcile(
        &holzi,
        &[raw(base.clone(), phone.clone(), Some(3_000))],
        Some(RAW),
        false,
    );
    assert_eq!(
        plan.updates,
        vec![(
            "v-phone".to_owned(),
            "mobile".to_owned(),
            "0170 1111111".to_owned()
        )]
    );
    assert!(
        plan.ops.is_empty(),
        "the phone already holds the winning value"
    );

    // holzi changed later, at the same time, or the phone has no time: holzi's value goes there.
    for last_updated in [Some(1_500), Some(2_000), None] {
        let plan = reconcile(
            &holzi,
            &[raw(base.clone(), phone.clone(), last_updated)],
            Some(RAW),
            false,
        );
        assert!(plan.updates.is_empty(), "{last_updated:?}");
        assert_eq!(plan.ops.len(), 1, "{last_updated:?}");
    }
}

#[test]
fn a_deletion_beats_a_change() {
    let (mut holzi, mut phone, base) = agreed();
    phone.values.retain(|row| row.data_id != 11);
    set_holzi(&mut holzi, "v-phone", "0170 holzi", Some(9_000));
    let plan = reconcile(
        &holzi,
        &[raw(base.clone(), phone, Some(1))],
        Some(RAW),
        false,
    );
    assert_eq!(plan.deletes, vec!["v-phone".to_owned()]);

    let (mut holzi, mut phone, _) = agreed();
    holzi.values.retain(|value| value.id != "v-mail");
    set_phone(&mut phone, 12, "anna@new.example");
    let plan = reconcile(&holzi, &[raw(base, phone, Some(9_000))], Some(RAW), false);
    assert_eq!(plan.ops, vec![(RAW, RowOp::Delete { data_id: 12 })]);
}

#[test]
fn holzi_only_kinds_are_neither_written_nor_removed() {
    let (mut holzi, phone, base) = agreed();
    // The run loads only kinds that go to a phone; a custom value never reaches the comparison,
    // and a phone change cannot delete what the comparison does not see.
    holzi.values.push(holzi_value(
        "v-own",
        "custom",
        "custom:Kundennummer",
        "4711",
        None,
    ));
    holzi
        .values
        .retain(|value| crate::contacts::kinds::goes_to_phone(&value.kind));
    let plan = reconcile(&holzi, &[raw(base, phone, None)], Some(RAW), false);
    assert_eq!(plan, Plan::default());
}

#[test]
fn rows_the_phone_has_and_holzi_does_not_map_stay_untouched() {
    let (holzi, phone, base) = agreed();
    // A WhatsApp row is not in the phone side at all; the plan only names rows holzi maps.
    let mut holzi = holzi;
    set_holzi(&mut holzi, "v-phone", "0170 9", Some(2_000));
    let plan = reconcile(&holzi, &[raw(base, phone, Some(1))], Some(RAW), false);
    for (_, op) in &plan.ops {
        match op {
            RowOp::Update { data_id, .. } | RowOp::Delete { data_id } => {
                assert!([10, 11, 12].contains(data_id))
            }
            RowOp::Insert { .. } => {}
        }
    }
}

#[test]
fn new_values_on_either_side_reach_the_other() {
    let (mut holzi, mut phone, base) = agreed();
    phone.values.push(phone_value(13, "phone", "work", "030 1"));
    holzi.values.push(holzi_value(
        "v-site",
        "website",
        "home",
        "https://anna.example",
        None,
    ));
    let plan = reconcile(&holzi, &[raw(base, phone, Some(1))], Some(RAW), false);
    assert_eq!(plan.inserts.len(), 1);
    assert_eq!(
        (
            plan.inserts[0].kind.as_str(),
            plan.inserts[0].value.as_str()
        ),
        ("phone", "030 1")
    );
    assert!(
        matches!(&plan.ops[..], [(RAW, RowOp::Insert { mimetype, .. })] if mimetype == "Website")
    );
}

#[test]
fn a_change_a_read_only_raw_contact_cannot_take_is_blocked() {
    let (mut holzi, phone, base) = agreed();
    set_holzi(&mut holzi, "v-phone", "0170 9", Some(2_000));
    let mut side = raw(base, phone, Some(1));
    side.writable = false;
    let plan = reconcile(&holzi, &[side], None, false);
    assert!(plan.ops.is_empty());
    assert!(
        plan.blocked,
        "the change stays in holzi and is shown as not arrived (FR-025)"
    );
}

#[test]
fn head_fields_follow_the_side_that_changed() {
    let (mut holzi, mut phone, base) = agreed();
    holzi
        .head
        .insert("given_name".to_owned(), "Anne".to_owned());
    phone.head.insert("nickname".to_owned(), "Anni".to_owned());
    phone.head_rows.insert("Nickname".to_owned(), 14);
    let plan = reconcile(&holzi, &[raw(base, phone, Some(1))], Some(RAW), false);
    assert_eq!(
        plan.head,
        vec![("nickname".to_owned(), Some("Anni".to_owned()))]
    );
    let (
        _,
        RowOp::Update {
            data_id, fields, ..
        },
    ) = &plan.ops[0]
    else {
        panic!("an update")
    };
    assert_eq!(*data_id, 10);
    assert_eq!(fields.get("given").and_then(|v| v.as_str()), Some("Anne"));
}

#[test]
fn a_first_run_fills_what_holzi_lacks_and_leaves_the_phone_head_alone() {
    let (_, phone, _) = agreed();
    let mut base = Base::default();
    base.phone.head = phone.head.clone();
    let holzi = HolziSide {
        head: BTreeMap::from([("given_name".to_owned(), "Annette".to_owned())]),
        ..HolziSide::default()
    };
    let plan = reconcile(&holzi, &[raw(base, phone, None)], Some(RAW), true);
    assert_eq!(
        plan.head,
        vec![("family_name".to_owned(), Some("Beck".to_owned()))]
    );
    assert_eq!(plan.inserts.len(), 2, "both phone values come to holzi");
    assert!(plan.ops.is_empty());
}

#[test]
fn the_base_after_a_run_maps_rows_to_the_values_they_hold() {
    let (holzi, phone, base) = agreed();
    let mut values = holzi.values.clone();
    values.push(holzi_value("v-new", "phone", "work", "030 1", None));
    let mut after = phone.clone();
    after.values.push(phone_value(20, "phone", "work", "030 1"));
    let next = next_base("t2", &holzi.head, &values, after, &base.mapping);
    assert_eq!(next.holzi_updated_at, "t2");
    assert_eq!(next.mapping.get(&11).map(String::as_str), Some("v-phone"));
    assert_eq!(next.mapping.get(&20).map(String::as_str), Some("v-new"));
    assert_eq!(next.holzi_values.len(), 3);
}
