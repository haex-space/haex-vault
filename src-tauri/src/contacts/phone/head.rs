//! The head fields in the three-way comparison (spec 051, research R6): per field, like a value.
//! A change from holzi goes to the raw contacts that held the old value and to the primary one; a
//! first run only fills what holzi lacks and never changes the phone's head.

use std::collections::{BTreeMap, BTreeSet};

use super::mapping::{self, HEAD_FIELDS};
use super::reconcile::{later, HolziSide, Plan, RawSide, RowOp};

pub fn head(
    holzi: &HolziSide,
    raws: &[RawSide],
    primary: Option<i64>,
    writable: &BTreeSet<i64>,
    fill: bool,
    plan: &mut Plan,
) {
    let agreed = raws.first().map(|raw| &raw.base.holzi_head);
    // Raw contact → mimetype → fields to write.
    let mut writes: BTreeMap<i64, BTreeMap<&str, BTreeMap<&str, Option<&str>>>> = BTreeMap::new();
    for field in &HEAD_FIELDS {
        let column = field.column;
        let now = holzi.head.get(column).map(String::as_str);
        let before = agreed.and_then(|head| head.get(column)).map(String::as_str);
        let mut phone = None;
        let mut phone_time = None;
        for raw in raws {
            let was = raw.base.phone.head.get(column).map(String::as_str);
            let is = raw.now().head.get(column).map(String::as_str);
            if was != is && phone.is_none() {
                phone = Some(is);
                phone_time = raw.last_updated;
            }
        }
        let holzi_changed = now != before;
        let target = match (holzi_changed, phone) {
            (false, None) => now,
            (false, Some(phone)) => phone,
            (true, None) => now,
            (true, Some(phone)) if phone == now => now,
            (true, Some(phone)) => {
                if later(phone_time, holzi.head_millis.get(column).copied()) {
                    phone
                } else {
                    now
                }
            }
        };
        let target = match target {
            None if fill => first_on_phone(raws, primary, column),
            other => other,
        };
        if target != now {
            plan.head
                .push((column.to_owned(), target.map(str::to_owned)));
        }
        if fill || (!holzi_changed && phone.is_none()) {
            // A first run never changes the phone's head fields, and nothing changed otherwise.
            continue;
        }
        for raw in raws {
            let is = raw.now().head.get(column).map(String::as_str);
            let holds_old = is == before && is.is_some();
            if is == target || !(holds_old || Some(raw.raw_contact_id) == primary) {
                continue;
            }
            if !writable.contains(&raw.raw_contact_id) {
                plan.blocked = true;
                continue;
            }
            writes
                .entry(raw.raw_contact_id)
                .or_default()
                .entry(field.mimetype)
                .or_default()
                .insert(column, target);
        }
    }
    for (raw_id, by_mimetype) in writes {
        let raw = raws.iter().find(|raw| raw.raw_contact_id == raw_id);
        for (mimetype, columns) in by_mimetype {
            let fields = mapping::head_fields(mimetype, &columns);
            match raw.and_then(|raw| raw.now().head_rows.get(mimetype)) {
                Some(data_id) => plan.ops.push((
                    raw_id,
                    RowOp::Update {
                        data_id: *data_id,
                        mimetype: mimetype.to_owned(),
                        fields,
                    },
                )),
                None if columns.values().any(Option::is_some) => {
                    plan.ops.push((
                        raw_id,
                        RowOp::Insert {
                            mimetype: mimetype.to_owned(),
                            fields,
                        },
                    ));
                }
                None => {}
            }
        }
    }
}

fn first_on_phone<'a>(raws: &'a [RawSide], primary: Option<i64>, column: &str) -> Option<&'a str> {
    let preferred = raws
        .iter()
        .filter(|raw| Some(raw.raw_contact_id) == primary);
    let others = raws
        .iter()
        .filter(|raw| Some(raw.raw_contact_id) != primary);
    preferred
        .chain(others)
        .find_map(|raw| raw.now().head.get(column).map(String::as_str))
}
