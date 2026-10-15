//! The fake phone as the run sees it: the [`PhoneBook`] calls on the in-memory address book.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json};
use tauri_plugin_holzi_android::contacts::{
    ApplyResult, DataRow, Op, OpResult, PhoneAccount, PhoneGroup, RawData, RawIndex, Target,
};

use super::{Book, FakeGroup, FakePhoneBook, FakeRaw};
use crate::contacts::phone::write::MAX_OPS;
use crate::contacts::phone::{PhoneBook, MAX_READ};
use crate::error::{HolziError, Result};

pub(super) fn merge_fields(target: &mut Map<String, Json>, fields: &Map<String, Json>) {
    for (key, value) in fields {
        if value.is_null() {
            target.remove(key);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

impl PhoneBook for FakePhoneBook {
    fn permitted(&self) -> Result<bool> {
        Ok(self.book().permitted)
    }

    fn accounts(&self) -> Result<Vec<PhoneAccount>> {
        Ok(self.book().accounts.clone())
    }

    fn index(&self, after_id: i64, limit: u32) -> Result<Vec<RawIndex>> {
        Ok(self
            .book()
            .raws
            .values()
            .filter(|raw| raw.index.id > after_id)
            .take(limit as usize)
            .map(|raw| raw.index.clone())
            .collect())
    }

    fn read(&self, ids: &[i64]) -> Result<Vec<RawData>> {
        assert!(
            ids.len() <= MAX_READ,
            "at most {MAX_READ} raw contacts per read"
        );
        let book = self.book();
        Ok(ids
            .iter()
            .filter_map(|id| book.raws.get(id))
            .map(|raw| RawData {
                id: raw.index.id,
                data: raw.rows.iter().map(|row| book.named(row)).collect(),
                has_photo: raw.photo.is_some(),
            })
            .collect())
    }

    fn apply(&self, ops: &[Op]) -> Result<ApplyResult> {
        assert!(ops.len() <= MAX_OPS, "at most {MAX_OPS} ops per call");
        let mut book = self.book();
        book.ops.extend(ops.iter().cloned());
        if std::mem::take(&mut book.fail_next) {
            return Ok(ApplyResult {
                results: Vec::new(),
                error: Some("OperationApplicationException".to_owned()),
            });
        }
        let mut refs: BTreeMap<u32, i64> = BTreeMap::new();
        let mut touched = Vec::new();
        for op in ops {
            let raw = match op {
                Op::InsertRaw {
                    account_type,
                    account_name,
                    reference,
                } => {
                    book.next_raw += 1;
                    book.next_aggregate += 1;
                    let (id, aggregate) = (book.next_raw, book.next_aggregate);
                    let index = RawIndex {
                        id,
                        version: 0,
                        deleted: false,
                        account_type: account_type.clone(),
                        account_name: account_name.clone(),
                        source_id: None,
                        contact_id: aggregate,
                        lookup_key: format!("lookup-{aggregate}"),
                        last_updated: Some(book.clock),
                        read_only: false,
                    };
                    book.raws.insert(
                        id,
                        FakeRaw {
                            index,
                            rows: Vec::new(),
                            photo: None,
                        },
                    );
                    refs.insert(*reference, id);
                    Some(id)
                }
                Op::InsertData {
                    raw,
                    mimetype,
                    fields,
                } => {
                    let id = match raw {
                        Target::Id(id) => *id,
                        Target::Ref(reference) => refs[reference],
                    };
                    book.next_data += 1;
                    let data_id = book.next_data;
                    let mut clean = Map::new();
                    merge_fields(&mut clean, fields);
                    let target = book
                        .raws
                        .get_mut(&id)
                        .ok_or_else(|| missing("raw contact"))?;
                    target.rows.push(DataRow {
                        data_id,
                        mimetype: mimetype.clone(),
                        is_read_only: false,
                        fields: clean,
                    });
                    Some(id)
                }
                Op::UpdateData {
                    data_id, fields, ..
                } => {
                    let found = book.raws.values_mut().find_map(|raw| {
                        let row = raw.rows.iter_mut().find(|row| row.data_id == *data_id)?;
                        merge_fields(&mut row.fields, fields);
                        Some(raw.index.id)
                    });
                    Some(found.ok_or_else(|| missing("data row"))?)
                }
                Op::DeleteData { data_id } => {
                    let found = book.raws.values_mut().find_map(|raw| {
                        let before = raw.rows.len();
                        raw.rows.retain(|row| row.data_id != *data_id);
                        (raw.rows.len() != before).then_some(raw.index.id)
                    });
                    Some(found.ok_or_else(|| missing("data row"))?)
                }
                Op::DeleteRaw { raw_contact_id } => {
                    book.raws.remove(raw_contact_id);
                    Some(*raw_contact_id)
                }
                Op::InsertGroup {
                    account_type,
                    account_name,
                    title,
                } => {
                    book.next_group += 1;
                    let group = FakeGroup {
                        id: book.next_group,
                        account: (account_type.clone(), account_name.clone()),
                        title: title.clone(),
                        read_only: false,
                    };
                    book.groups.push(group);
                    None
                }
            };
            touched.push(raw);
        }
        let changed: std::collections::BTreeSet<i64> = touched.iter().flatten().copied().collect();
        for raw in &changed {
            if book.raws.contains_key(raw) {
                book.raws.get_mut(raw).expect("present").index.version += 1;
            }
        }
        let results = touched
            .into_iter()
            .map(|raw| OpResult {
                raw_contact_id: raw,
                version: raw
                    .and_then(|raw| book.raws.get(&raw))
                    .map(|raw| raw.index.version),
            })
            .collect();
        Ok(ApplyResult {
            results,
            error: None,
        })
    }

    fn photo(&self, id: i64) -> Result<Option<Vec<u8>>> {
        Ok(self.book().raws.get(&id).and_then(|raw| raw.photo.clone()))
    }

    fn write_photo(&self, id: i64, jpeg: &[u8]) -> Result<Option<i64>> {
        let mut book = self.book();
        book.photo_writes.push(id);
        let raw = book
            .raws
            .get_mut(&id)
            .ok_or_else(|| missing("raw contact"))?;
        raw.photo = Some(jpeg.to_vec());
        raw.index.version += 1;
        Ok(Some(raw.index.version))
    }

    fn groups(&self, account_type: &str, account_name: &str) -> Result<Vec<PhoneGroup>> {
        Ok(self
            .book()
            .groups
            .iter()
            .filter(|group| group.account.0 == account_type && group.account.1 == account_name)
            .map(|group| PhoneGroup {
                id: group.id,
                title: group.title.clone(),
                read_only: group.read_only,
            })
            .collect())
    }
}

impl Book {
    /// A data row as the plugin hands it over: a membership carries its group's title and whether
    /// the group is the system's own.
    pub(super) fn named(&self, row: &DataRow) -> DataRow {
        let mut row = row.clone();
        if row.mimetype == "GroupMembership" {
            let id = row
                .fields
                .get("groupId")
                .and_then(Json::as_str)
                .and_then(|id| id.parse::<i64>().ok());
            let group = self.groups.iter().find(|group| Some(group.id) == id);
            row.fields.insert(
                "title".to_owned(),
                Json::String(group.map(|group| group.title.clone()).unwrap_or_default()),
            );
            row.fields.insert(
                "readOnly".to_owned(),
                Json::Bool(group.is_none_or(|group| group.read_only)),
            );
        }
        row
    }
}

fn missing(what: &str) -> HolziError {
    HolziError::PhoneContactsFailed {
        reason: format!("no such {what}"),
    }
}
