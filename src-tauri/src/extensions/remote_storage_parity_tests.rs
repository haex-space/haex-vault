//! An extension gets the same answers on WebDAV as on S3 (spec 050 US3, SC-002, research R9): one
//! catalog of bridge calls runs against the S3 fake and against a WebDAV server of the test
//! process, and the answers must be equal. Then what only WebDAV needs: keys a server cannot
//! store, names that differ only in case, empty folders after a delete, the list of storages, and
//! that no answer and no log line carries the login, the password or the address (SC-005, T093).

use serde_json::{json, Value};

use crate::extensions::bridge::blocking::block_on;
use crate::extensions::error::BridgeError;
use crate::extensions::remote_storage_test_support::{
    setup, Setup, WebdavServer, ACCESS_KEY, SECRET, WEBDAV_PASSWORD, WEBDAV_USER,
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

#[test]
fn the_catalog_answers_alike_on_s3_and_webdav() {
    let s = setup();
    let server = WebdavServer::start(FOLDER);
    let webdav = s.add_webdav(&server, "Server", FOLDER);
    s.permit("readWrite", &s.photos, "granted");
    s.permit("readWrite", &webdav, "granted");
    let on_s3 = catalog(&s, &s.photos);
    let on_webdav = catalog(&s, &webdav);
    for ((case, s3), (_, dav)) in on_s3.iter().zip(&on_webdav) {
        assert_eq!(s3, dav, "{case}");
    }
    let listed = &on_webdav
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
        "no folders, no temporary files"
    );
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
fn keys_a_server_cannot_store_are_refused_on_webdav_only() {
    let s = setup();
    let server = WebdavServer::start(FOLDER);
    let webdav = s.add_webdav(&server, "Server", FOLDER);
    s.permit("readWrite", &s.photos, "granted");
    s.permit("readWrite", &webdav, "granted");
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
        assert_eq!(upload(&webdav), 3001, "{key:?} on WebDAV");
        assert_eq!(upload(&s.photos), 0, "{key:?} on S3 is unchanged");
    }
    let listed = |prefix: &str| {
        s.code(
            "extension_remote_storage_list",
            json!({ "request": { "backendId": webdav, "prefix": prefix } }),
        )
    };
    assert_eq!(listed("a:"), 3001);
    assert_eq!(listed(&"y".repeat(255)), 0, "255 bytes fit");
}

#[test]
fn a_new_name_that_differs_only_in_case_is_refused_on_webdav() {
    let s = setup();
    let server = WebdavServer::start(FOLDER);
    let webdav = s.add_webdav(&server, "Server", FOLDER);
    s.permit("readWrite", &webdav, "granted");
    let upload = |key: &str, data: &[u8]| {
        s.code(
            "extension_remote_storage_upload",
            json!({ "request": { "backendId": webdav, "key": key, "data": b64(data) } }),
        )
    };
    assert_eq!(upload("dir/a.txt", b"eins"), 0);
    assert_eq!(
        upload("dir/A.txt", b"zwei"),
        3001,
        "would hit a.txt on Koofr"
    );
    assert_eq!(
        upload("DIR/b.txt", b"drei"),
        0,
        "only the last part is compared"
    );
    assert_eq!(
        upload("dir/a.txt", b"neu"),
        0,
        "the same name is overwritten"
    );
    let read = s
        .call(
            "extension_remote_storage_download",
            json!({ "request": { "backendId": webdav, "key": "dir/a.txt" } }),
        )
        .expect("download");
    assert_eq!(read, json!(b64(b"neu")));
}

#[test]
fn deleting_the_last_file_removes_empty_folders_up_to_the_area() {
    let s = setup();
    let server = WebdavServer::start(FOLDER);
    let webdav = s.add_webdav(&server, "Server", FOLDER);
    s.permit("readWrite", &webdav, "granted");
    let area = s.prefix();
    for key in ["x/y/z.txt", "x/keep.txt"] {
        s.call(
            "extension_remote_storage_upload",
            json!({ "request": { "backendId": webdav, "key": key, "data": b64(b"1") } }),
        )
        .expect("upload");
    }
    let delete = |key: &str| {
        s.call(
            "extension_remote_storage_delete",
            json!({ "request": { "backendId": webdav, "key": key } }),
        )
        .expect("delete")
    };
    delete("x/y/z.txt");
    let keys = on_server(&s, &webdav);
    assert!(!keys.contains(&format!("{area}x/y/")), "{keys:?}");
    assert!(keys.contains(&format!("{area}x/")), "x/ still holds a file");
    delete("x/keep.txt");
    let keys = on_server(&s, &webdav);
    assert!(!keys.contains(&format!("{area}x/")), "{keys:?}");
    assert!(
        keys.contains(&area),
        "the area itself stays, nothing above it goes: {keys:?}"
    );
}

#[test]
fn the_list_names_a_folder_for_a_webdav_storage() {
    let s = setup();
    let server = WebdavServer::start(FOLDER);
    let webdav = s.add_webdav(&server, "Server", FOLDER);
    s.permit("read", &webdav, "granted");
    let listed = s
        .call("extension_remote_storage_list_backends", json!({}))
        .expect("list");
    assert_eq!(
        listed,
        json!([{
            "id": webdav,
            "type": "webdav",
            "name": "Server",
            "providerName": "Server",
            "folder": FOLDER,
        }]),
        "no address, no login (FR-017a)"
    );
}

#[test]
fn no_answer_and_no_log_line_carries_the_login_or_the_address() {
    crate::test_log::install();
    let s = setup();
    let server = WebdavServer::start(FOLDER);
    let address = server
        .endpoint
        .trim_start_matches("http://")
        .split('/')
        .next()
        .expect("host and port")
        .to_owned();
    let webdav = s.add_webdav(&server, "Server", FOLDER);
    s.permit("readWrite", &webdav, "granted");
    s.permit("readWrite", &s.photos, "granted");
    let mut answers = format!("{:?}", catalog(&s, &webdav));
    answers.push_str(&format!("{:?}", catalog(&s, &s.photos)));
    answers.push_str(&format!(
        "{:?}",
        s.call("extension_remote_storage_list_backends", json!({}))
    ));
    drop(server);
    // The server is gone: the failures of every call are searched too.
    answers.push_str(&format!("{:?}", catalog(&s, &webdav)));
    // What holzi's log keeps (Info and above); the HTTP client's debug lines name addresses.
    let logs = crate::test_log::at_least(log::Level::Info);
    for secret in [WEBDAV_PASSWORD, WEBDAV_USER, &address, SECRET, ACCESS_KEY] {
        assert!(!answers.contains(secret), "an answer names {secret}");
        let named: Vec<&str> = logs.lines().filter(|l| l.contains(secret)).collect();
        assert!(named.is_empty(), "log lines name {secret}: {named:#?}");
    }
}
