//! A phone address book in memory for the tests of the sync (spec 051 T030): raw contacts with a
//! version that rises on every change, a deleted flag, an account, a source id, an aggregated
//! contact with its lookup key, a read-only flag and data rows per mimetype. The tests change it
//! "as the user" and see every op holzi sent.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::{json, Map, Value as Json};
use tauri_plugin_holzi_android::contacts::{DataRow, Op, PhoneAccount, RawIndex};

use fake::merge_fields;

/// A raw contact of the fake.
#[derive(Debug, Clone)]
pub struct FakeRaw {
    pub index: RawIndex,
    pub rows: Vec<DataRow>,
    pub photo: Option<Vec<u8>>,
}

#[derive(Default)]
struct Book {
    next_raw: i64,
    next_data: i64,
    next_aggregate: i64,
    clock: i64,
    raws: BTreeMap<i64, FakeRaw>,
    accounts: Vec<PhoneAccount>,
    permitted: bool,
    ops: Vec<Op>,
    fail_next: bool,
    next_group: i64,
    groups: Vec<FakeGroup>,
    /// Raw contacts whose photo holzi wrote, in order.
    photo_writes: Vec<i64>,
}

/// A group of an account of the fake.
#[derive(Debug, Clone)]
struct FakeGroup {
    id: i64,
    account: (String, String),
    title: String,
    read_only: bool,
}

pub struct FakePhoneBook {
    book: Mutex<Book>,
}

pub const GOOGLE: (&str, &str) = ("com.google", "mara@example.org");
pub const LOCAL: (&str, &str) = ("", "");

fn account(kind: &str, name: &str, writable: bool) -> PhoneAccount {
    PhoneAccount {
        account_type: kind.to_owned(),
        name: name.to_owned(),
        writable,
        is_default: false,
    }
}

/// A membership in the group with this id.
pub fn member(group: i64) -> (String, Map<String, Json>) {
    row("GroupMembership", json!({ "groupId": group.to_string() }))
}

/// A data row of `mimetype` with these fields.
pub fn row(mimetype: &str, fields: Json) -> (String, Map<String, Json>) {
    let Json::Object(fields) = fields else {
        panic!("fields are an object")
    };
    (mimetype.to_owned(), fields)
}

pub fn name(given: &str, family: &str) -> (String, Map<String, Json>) {
    row(
        "StructuredName",
        json!({ "given": given, "family": family, "display": format!("{given} {family}") }),
    )
}

pub fn phone(number: &str, kind: i64) -> (String, Map<String, Json>) {
    row(
        "Phone",
        json!({ "value": number, "type": kind.to_string() }),
    )
}

pub fn email(address: &str) -> (String, Map<String, Json>) {
    row("Email", json!({ "value": address, "type": "1" }))
}

impl FakePhoneBook {
    /// A phone with the local account and a Google account, both writable, permission granted.
    pub fn new() -> Self {
        let book = Book {
            next_raw: 100,
            next_data: 1000,
            next_aggregate: 1,
            clock: 1_000,
            accounts: vec![
                account(LOCAL.0, LOCAL.1, true),
                account(GOOGLE.0, GOOGLE.1, true),
            ],
            permitted: true,
            ..Book::default()
        };
        Self {
            book: Mutex::new(book),
        }
    }

    fn book(&self) -> std::sync::MutexGuard<'_, Book> {
        self.book.lock().expect("the fake's lock")
    }

    /// Adds an account; `writable` false is a sync adapter that does not upload.
    pub fn add_account(&self, (kind, name): (&str, &str), writable: bool) {
        self.book().accounts.push(account(kind, name, writable));
    }

    /// Creates a raw contact "as the user", in its own aggregated contact.
    pub fn add(&self, (kind, name): (&str, &str), rows: Vec<(String, Map<String, Json>)>) -> i64 {
        let mut book = self.book();
        book.next_raw += 1;
        book.next_aggregate += 1;
        book.clock += 1;
        let id = book.next_raw;
        let aggregate = book.next_aggregate;
        let mut data = Vec::new();
        for (mimetype, fields) in rows {
            book.next_data += 1;
            data.push(DataRow {
                data_id: book.next_data,
                mimetype,
                is_read_only: false,
                fields,
            });
        }
        let index = RawIndex {
            id,
            version: 1,
            deleted: false,
            account_type: kind.to_owned(),
            account_name: name.to_owned(),
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
                rows: data,
                photo: None,
            },
        );
        id
    }

    /// Joins `raw` into the aggregated contact of `with`, as the phone does for the same person.
    pub fn join(&self, raw: i64, with: i64) {
        let mut book = self.book();
        let (aggregate, lookup) = {
            let other = &book.raws[&with].index;
            (other.contact_id, other.lookup_key.clone())
        };
        let raw = book.raws.get_mut(&raw).expect("the raw contact");
        raw.index.contact_id = aggregate;
        raw.index.lookup_key = lookup;
    }

    fn touch(book: &mut Book, raw: i64) {
        book.clock += 1;
        let clock = book.clock;
        let raw = book.raws.get_mut(&raw).expect("the raw contact");
        raw.index.version += 1;
        raw.index.last_updated = Some(clock);
    }

    /// Changes fields of a row "as the user"; a null field is removed.
    pub fn edit(&self, raw: i64, data_id: i64, fields: Json) {
        let mut book = self.book();
        let target = book.raws.get_mut(&raw).expect("the raw contact");
        let row = target
            .rows
            .iter_mut()
            .find(|row| row.data_id == data_id)
            .expect("the row");
        merge_fields(&mut row.fields, fields.as_object().expect("an object"));
        Self::touch(&mut book, raw);
    }

    /// Adds a row "as the user".
    pub fn add_row(&self, raw: i64, (mimetype, fields): (String, Map<String, Json>)) -> i64 {
        let mut book = self.book();
        book.next_data += 1;
        let data_id = book.next_data;
        let target = book.raws.get_mut(&raw).expect("the raw contact");
        target.rows.push(DataRow {
            data_id,
            mimetype,
            is_read_only: false,
            fields,
        });
        Self::touch(&mut book, raw);
        data_id
    }

    /// Removes a row "as the user".
    pub fn remove_row(&self, raw: i64, data_id: i64) {
        let mut book = self.book();
        book.raws
            .get_mut(&raw)
            .expect("the raw contact")
            .rows
            .retain(|row| row.data_id != data_id);
        Self::touch(&mut book, raw);
    }

    /// Deletes a raw contact "as the user"; the provider keeps it marked deleted.
    pub fn delete(&self, raw: i64) {
        let mut book = self.book();
        Self::touch(&mut book, raw);
        book.raws
            .get_mut(&raw)
            .expect("the raw contact")
            .index
            .deleted = true;
    }

    /// Removes an account from the phone with all its raw contacts.
    pub fn remove_account(&self, (kind, name): (&str, &str)) {
        let mut book = self.book();
        book.accounts
            .retain(|account| !(account.account_type == kind && account.name == name));
        book.raws
            .retain(|_, raw| !(raw.index.account_type == kind && raw.index.account_name == name));
    }

    pub fn set_permitted(&self, permitted: bool) {
        self.book().permitted = permitted;
    }

    pub fn set_read_only(&self, raw: i64) {
        self.book()
            .raws
            .get_mut(&raw)
            .expect("the raw contact")
            .index
            .read_only = true;
    }

    pub fn set_photo(&self, raw: i64, bytes: Vec<u8>) {
        self.book()
            .raws
            .get_mut(&raw)
            .expect("the raw contact")
            .photo = Some(bytes);
    }

    /// Adds a group to an account "as the user"; `read_only` is a group of the system's own.
    pub fn add_group(&self, (kind, name): (&str, &str), title: &str, read_only: bool) -> i64 {
        let mut book = self.book();
        book.next_group += 1;
        let id = book.next_group;
        book.groups.push(FakeGroup {
            id,
            account: (kind.to_owned(), name.to_owned()),
            title: title.to_owned(),
            read_only,
        });
        id
    }

    /// The titles of the groups of an account.
    pub fn group_titles(&self, (kind, name): (&str, &str)) -> Vec<String> {
        self.book()
            .groups
            .iter()
            .filter(|group| group.account.0 == kind && group.account.1 == name)
            .map(|group| group.title.clone())
            .collect()
    }

    /// The titles of the groups a raw contact is a member of.
    pub fn memberships(&self, raw: i64) -> Vec<String> {
        let book = self.book();
        let Some(found) = book.raws.get(&raw) else {
            return Vec::new();
        };
        found
            .rows
            .iter()
            .filter(|row| row.mimetype == "GroupMembership")
            .filter_map(|row| {
                let named = book.named(row);
                named.fields.get("title")?.as_str().map(str::to_owned)
            })
            .collect()
    }

    /// The raw contacts whose photo holzi wrote.
    pub fn photo_writes(&self) -> Vec<i64> {
        self.book().photo_writes.clone()
    }

    /// The next apply fails as a whole, as an `OperationApplicationException` does.
    pub fn fail_next_apply(&self) {
        self.book().fail_next = true;
    }

    /// Every op holzi sent.
    pub fn ops(&self) -> Vec<Op> {
        self.book().ops.clone()
    }

    /// A raw contact as it is now.
    pub fn raw(&self, raw: i64) -> Option<FakeRaw> {
        self.book().raws.get(&raw).cloned()
    }

    /// The raw contacts that are not deleted.
    pub fn live(&self) -> Vec<FakeRaw> {
        self.book()
            .raws
            .values()
            .filter(|raw| !raw.index.deleted)
            .cloned()
            .collect()
    }

    /// The `value` fields of the rows of a mimetype of a raw contact.
    pub fn values(&self, raw: i64, mimetype: &str) -> Vec<String> {
        self.raw(raw)
            .map(|raw| {
                raw.rows
                    .iter()
                    .filter(|row| row.mimetype == mimetype)
                    .filter_map(|row| {
                        row.fields
                            .get("value")
                            .and_then(Json::as_str)
                            .map(str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[path = "test_fake.rs"]
mod fake;
