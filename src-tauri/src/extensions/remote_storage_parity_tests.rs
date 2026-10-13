//! An extension gets the same answers on WebDAV and SFTP as on S3 (spec 050 US3, US5, SC-002,
//! research R9): one catalog of bridge calls runs against the S3 fake and against a WebDAV and an
//! SFTP server of the test process, and the answers must be equal. Then what only servers with
//! real folders need: keys a server cannot store, names that differ only in case, empty folders
//! after a delete, the list of storages, and that no answer and no log line carries the login, the
//! password or the address (SC-005, T093).

use serde_json::{json, Value};

use crate::extensions::bridge::blocking::block_on;
use crate::extensions::error::BridgeError;
use crate::extensions::remote_storage_test_support::{
    setup, Setup, SftpServer, WebdavServer, ACCESS_KEY, SECRET, WEBDAV_PASSWORD, WEBDAV_USER,
};
use crate::files::storage_time::parse_iso_ms;
use crate::passwords::service::PasswordsService;
use crate::remote_storage::service::StorageService;
use crate::remote_storage::test_support::resolver;

const FOLDER: &str = "Ablage";

fn b64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// An answer as the catalog compares it: a list without its times (checked to be ISO times on
/// their own), an error by code, message and kind, never the storage id it may name.
fn normalized(answer: Result<Value, BridgeError>) -> Value {
    match answer {
        Ok(Value::Array(items)) => Value::Array(
            items
                .into_iter()
                .map(|mut item| {
                    if let Some(time) = item.get("lastModified").and_then(Value::as_str) {
                        assert!(parse_iso_ms(time).is_some(), "an ISO time: {time:?}");
                        item.as_object_mut().expect("item").remove("lastModified");
                    }
                    item
                })
                .collect(),
        ),
        Ok(value) => value,
        Err(error) => json!({
            "code": error.code.as_u16(),
            "message": error.message,
            "kind": error.details.as_ref().and_then(|d| d.get("kind")).cloned(),
        }),
    }
}

/// The 038 catalog of object calls on the storage `id`, case by case.
fn catalog(s: &Setup, id: &str) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let mut run = |case: &str, method: &str, request: Value| {
        let mut request = request;
        request["backendId"] = json!(id);
        let answer = s.call(
            &format!("extension_remote_storage_{method}"),
            json!({ "request": request }),
        );
        out.push((case.to_owned(), normalized(answer)));
    };
    for (key, data) in [
        ("notizen/a.txt", "eins"),
        ("notizen/tief/b.txt", "zwei"),
        ("notizen/bc.txt", "drei"),
        ("wurzel.txt", "alt"),
        ("wurzel.txt", "neu"),
    ] {
        run(
            &format!("upload {key}"),
            "upload",
            json!({ "key": key, "data": b64(data.as_bytes()) }),
        );
    }
    run("list all", "list", json!({}));
    for prefix in ["notizen/b", "notizen/", "notizen", "fehlt/", "notizen/tief"] {
        run(
            &format!("list {prefix}"),
            "list",
            json!({ "prefix": prefix }),
        );
    }
    for key in ["notizen/a.txt", "wurzel.txt", "fehlt.txt", "notizen/tief"] {
        run(
            &format!("download {key}"),
            "download",
            json!({ "key": key }),
        );
    }
    for bad in ["../x", "/x", "a//b", "a/./b", "a\\b", "a\u{1}b", "", "a/"] {
        run(
            &format!("upload {bad:?}"),
            "upload",
            json!({ "key": bad, "data": "" }),
        );
        run(
            &format!("download {bad:?}"),
            "download",
            json!({ "key": bad }),
        );
        run(&format!("delete {bad:?}"), "delete", json!({ "key": bad }));
    }
    for bad in ["../", "/x", "a//"] {
        run(&format!("list {bad:?}"), "list", json!({ "prefix": bad }));
    }
    run(
        "upload not base64",
        "upload",
        json!({ "key": "x.txt", "data": "%%%" }),
    );
    run("delete b", "delete", json!({ "key": "notizen/tief/b.txt" }));
    run("delete missing", "delete", json!({ "key": "fehlt.txt" }));
    run("list after delete", "list", json!({}));
    out
}

/// A storage on each server with real folders of the test process, which live as long as this.
struct Trees {
    webdav: Option<WebdavServer>,
    sftp: Option<SftpServer>,
    /// Protocol and storage id.
    storages: Vec<(&'static str, String)>,
}

impl Trees {
    fn start(s: &Setup, access: &str) -> Self {
        let webdav = WebdavServer::start(FOLDER);
        let sftp = SftpServer::start(FOLDER);
        let storages = vec![
            ("webdav", s.add_webdav(&webdav, "Server", FOLDER)),
            ("sftp", s.add_sftp(&sftp, "SFTP", FOLDER)),
        ];
        for (_, id) in &storages {
            s.permit(access, id, "granted");
        }
        Self {
            webdav: Some(webdav),
            sftp: Some(sftp),
            storages,
        }
    }

    /// `host:port` of each server.
    fn addresses(&self) -> Vec<String> {
        let webdav = self.webdav.as_ref().map(|server| {
            server
                .endpoint
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or_default()
                .to_owned()
        });
        let sftp = self
            .sftp
            .as_ref()
            .map(|server| server.server.address.to_string());
        webdav.into_iter().chain(sftp).collect()
    }
}

#[test]
fn the_catalog_answers_alike_on_s3_webdav_and_sftp() {
    let s = setup();
    let trees = Trees::start(&s, "readWrite");
    s.permit("readWrite", &s.photos, "granted");
    let on_s3 = catalog(&s, &s.photos);
    for (protocol, id) in &trees.storages {
        let on_tree = catalog(&s, id);
        for ((case, s3), (_, tree)) in on_s3.iter().zip(&on_tree) {
            assert_eq!(s3, tree, "{protocol}: {case}");
        }
        let listed = &on_tree
            .iter()
            .find(|(case, _)| case == "list after delete")
            .expect("case")
            .1;
        assert_eq!(
            listed,
            &json!([
                { "key": "notizen/a.txt", "size": 4 },
                { "key": "notizen/bc.txt", "size": 4 },
                { "key": "wurzel.txt", "size": 3 },
            ]),
            "{protocol}: no folders, no temporary files"
        );
    }
}

/// The keys and folders under the storage `id` on its server, as holzi's client lists them.
fn on_server(s: &Setup, id: &str) -> Vec<String> {
    let service = StorageService::new(
        s.vault.clone(),
        PasswordsService::new(s.vault.clone()),
        s.store.clone(),
        resolver(&[]),
    );
    let access = block_on(service.access_of(id)).expect("access");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    block_on(s.store.list(&access, "", 1000, deadline))
        .expect("list")
        .into_iter()
        .map(|object| object.key)
        .collect()
}

#[test]
fn keys_a_server_cannot_store_are_refused_on_servers_with_folders_only() {
    let s = setup();
    let trees = Trees::start(&s, "readWrite");
    s.permit("readWrite", &s.photos, "granted");
    let long = "x".repeat(256);
    let part = format!("{}1234", crate::remote_storage::PART_PREFIX);
    for key in [
        "a:b.txt", "was?.txt", "a|b", "<a>", "\"a\"", "x*", &long, &part,
    ] {
        let upload = |id: &str| {
            s.code(
                "extension_remote_storage_upload",
                json!({ "request": { "backendId": id, "key": key, "data": b64(b"x") } }),
            )
        };
        for (protocol, id) in &trees.storages {
            assert_eq!(upload(id), 3001, "{key:?} on {protocol}");
        }
        assert_eq!(upload(&s.photos), 0, "{key:?} on S3 is unchanged");
    }
    for (protocol, id) in &trees.storages {
        let listed = |prefix: &str| {
            s.code(
                "extension_remote_storage_list",
                json!({ "request": { "backendId": id, "prefix": prefix } }),
            )
        };
        assert_eq!(listed("a:"), 3001, "{protocol}");
        assert_eq!(listed(&"y".repeat(255)), 0, "{protocol}: 255 bytes fit");
    }
}

#[test]
fn a_new_name_that_differs_only_in_case_is_refused_on_servers_with_folders() {
    let s = setup();
    let trees = Trees::start(&s, "readWrite");
    for (protocol, id) in &trees.storages {
        let upload = |key: &str, data: &[u8]| {
            s.code(
                "extension_remote_storage_upload",
                json!({ "request": { "backendId": id, "key": key, "data": b64(data) } }),
            )
        };
        assert_eq!(upload("dir/a.txt", b"eins"), 0, "{protocol}");
        assert_eq!(
            upload("dir/A.txt", b"zwei"),
            3001,
            "{protocol}: would hit a.txt on Koofr"
        );
        assert_eq!(
            upload("DIR/b.txt", b"drei"),
            0,
            "{protocol}: only the last part is compared"
        );
        assert_eq!(
            upload("dir/a.txt", b"neu"),
            0,
            "{protocol}: the same name is overwritten"
        );
        let read = s
            .call(
                "extension_remote_storage_download",
                json!({ "request": { "backendId": id, "key": "dir/a.txt" } }),
            )
            .expect("download");
        assert_eq!(read, json!(b64(b"neu")), "{protocol}");
    }
}

#[test]
fn deleting_the_last_file_removes_empty_folders_up_to_the_area() {
    let s = setup();
    let trees = Trees::start(&s, "readWrite");
    let area = s.prefix();
    for (protocol, id) in &trees.storages {
        for key in ["x/y/z.txt", "x/keep.txt"] {
            s.call(
                "extension_remote_storage_upload",
                json!({ "request": { "backendId": id, "key": key, "data": b64(b"1") } }),
            )
            .expect("upload");
        }
        let delete = |key: &str| {
            s.call(
                "extension_remote_storage_delete",
                json!({ "request": { "backendId": id, "key": key } }),
            )
            .expect("delete")
        };
        delete("x/y/z.txt");
        let keys = on_server(&s, id);
        assert!(
            !keys.contains(&format!("{area}x/y/")),
            "{protocol}: {keys:?}"
        );
        assert!(
            keys.contains(&format!("{area}x/")),
            "{protocol}: x/ holds a file"
        );
        delete("x/keep.txt");
        let keys = on_server(&s, id);
        assert!(!keys.contains(&format!("{area}x/")), "{protocol}: {keys:?}");
        assert!(
            keys.contains(&area),
            "{protocol}: the area stays, nothing above it goes: {keys:?}"
        );
    }
}

#[test]
fn the_list_names_a_folder_for_servers_with_folders() {
    let s = setup();
    let trees = Trees::start(&s, "read");
    let listed = s
        .call("extension_remote_storage_list_backends", json!({}))
        .expect("list");
    let mut listed = listed.as_array().expect("array").clone();
    listed.sort_by_key(|item| item["type"].as_str().unwrap_or_default().to_owned());
    assert_eq!(
        Value::Array(listed),
        json!([
            {
                "id": trees.storages[1].1,
                "type": "sftp",
                "name": "SFTP",
                "providerName": "SFTP",
                "folder": FOLDER,
            },
            {
                "id": trees.storages[0].1,
                "type": "webdav",
                "name": "Server",
                "providerName": "Server",
                "folder": FOLDER,
            },
        ]),
        "no address, no login, no host key (FR-017a)"
    );
}

#[test]
fn no_answer_and_no_log_line_carries_the_login_or_the_address() {
    crate::test_log::install();
    let s = setup();
    let mut trees = Trees::start(&s, "readWrite");
    let addresses = trees.addresses();
    let host_key = trees
        .sftp
        .as_ref()
        .map(|server| server.server.host_key.clone())
        .unwrap_or_default();
    s.permit("readWrite", &s.photos, "granted");
    let mut answers = format!("{:?}", catalog(&s, &s.photos));
    for (_, id) in &trees.storages {
        answers.push_str(&format!("{:?}", catalog(&s, id)));
    }
    answers.push_str(&format!(
        "{:?}",
        s.call("extension_remote_storage_list_backends", json!({}))
    ));
    trees.webdav.take();
    trees.sftp.take();
    // The servers are gone: the failures of every call are searched too.
    for (_, id) in &trees.storages {
        answers.push_str(&format!("{:?}", catalog(&s, id)));
    }
    // What holzi's log keeps (Info and above); the HTTP client's debug lines name addresses.
    let logs = crate::test_log::at_least(log::Level::Info);
    let mut secrets = vec![WEBDAV_PASSWORD, WEBDAV_USER, SECRET, ACCESS_KEY];
    secrets.extend(addresses.iter().map(String::as_str));
    let key_body = host_key.split(' ').nth(1).unwrap_or("none");
    secrets.push(key_body);
    for secret in secrets {
        assert!(!answers.contains(secret), "an answer names {secret}");
        let named: Vec<&str> = logs.lines().filter(|l| l.contains(secret)).collect();
        assert!(named.is_empty(), "log lines name {secret}: {named:#?}");
    }
}
