use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::extensions::protocol::{encode_path, url_id};
use crate::extensions::registry::install::install;
use crate::extensions::registry::start::start;
use crate::passwords::test_support::open_test_vault;
use crate::storage::known_devices;
use crate::vault_gate::VaultGate;

struct Setup {
    _dir: tempfile::TempDir,
    vault: VaultDb,
    host: ExtensionHost,
    extension: Uuid,
}

fn setup(vector: &str) -> Setup {
    let (dir, db) = open_test_vault();
    let vault = VaultGate::new().vault_db(Arc::new(db.clone())).unwrap();
    let device = vault
        .read_blocking(|q| known_devices::list_devices(q))
        .unwrap()[0]
        .vault_device_uuid;
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/extension_bundles")
            .join(vector),
    )
    .unwrap();
    let extension = install(&vault, &bytes, vec![], false, device, 1)
        .unwrap()
        .ids
        .extension_id;
    let host = ExtensionHost::default();
    let started = start(&vault, extension, device, 2).unwrap();
    host.remember_started(started);
    Setup {
        _dir: dir,
        vault,
        host,
        extension,
    }
}

impl Setup {
    fn open(&self) -> String {
        let extension = self.extension;
        let bundle_id = self
            .vault
            .read_blocking(move |q| {
                crate::extensions::registry::effective::effective_bundle(q, extension)
                    .map_err(Into::into)
            })
            .unwrap()
            .unwrap()
            .bundle_id;
        self.host
            .frames
            .open(self.extension, bundle_id, "tab")
            .token
            .clone()
    }

    fn get(&self, file: &str, query: Option<&str>) -> Served {
        let path = format!("/{}/{}", url_id(self.extension), encode_path(file));
        serve(&self.vault, &self.host, "GET", &path, query)
    }
}

#[test]
fn nothing_is_served_while_no_frame_of_the_extension_is_open() {
    let s = setup("good-notes-like.xt");
    assert_eq!(s.get("icon.svg", None).status, StatusCode::FORBIDDEN);
}

#[test]
fn html_needs_the_start_token_and_gets_the_shim_and_the_csp() {
    let s = setup("good-notes-like.xt");
    let token = s.open();
    assert_eq!(s.get("index.html", None).status, StatusCode::FORBIDDEN);
    assert_eq!(
        s.get("index.html", Some("hf=wrong")).status,
        StatusCode::FORBIDDEN
    );
    let page = s.get("index.html", Some(&format!("hf={token}")));
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.content_type.starts_with("text/html"));
    assert!(String::from_utf8(page.body)
        .unwrap()
        .contains("haex:frame:init"));
    assert!(page.csp.unwrap().starts_with("default-src 'none'"));
}

#[test]
fn other_files_are_served_to_an_open_frame_with_type_and_csp() {
    let s = setup("good-notes-like.xt");
    s.open();
    let icon = s.get("icon.svg", None);
    assert_eq!(icon.status, StatusCode::OK);
    assert_eq!(icon.content_type, "image/svg+xml");
    assert!(icon.csp.is_some());
    let unicode = s.get("locales/übersicht.json", None);
    assert_eq!(unicode.status, StatusCode::OK);
    assert_eq!(s.get("missing.js", None).status, StatusCode::NOT_FOUND);
}

#[test]
fn a_file_without_token_comes_from_the_bundle_of_an_older_frame_too() {
    let s = setup("good-notes-like.xt");
    s.open();
    // A newer frame of the same extension on another bundle (after an update) that lacks the file.
    s.host.frames.open(s.extension, Uuid::new_v4(), "tab-new");
    assert_eq!(s.get("icon.svg", None).status, StatusCode::OK);
    assert_eq!(s.get("missing.js", None).status, StatusCode::NOT_FOUND);
}

#[test]
fn routes_without_a_file_extension_fall_back_to_the_entry() {
    let s = setup("good-notes-like.xt");
    let token = s.open();
    let page = s.get("notes/42", Some(&format!("hf={token}")));
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.content_type.starts_with("text/html"));
    assert_eq!(s.get("notes/42", None).status, StatusCode::FORBIDDEN);
}

#[test]
fn a_token_of_another_extension_and_other_methods_are_refused() {
    let s = setup("good-notes-like.xt");
    s.open();
    let foreign = s.host.frames.open(Uuid::new_v4(), Uuid::new_v4(), "tab-x");
    let path = format!("/{}/index.html", url_id(s.extension));
    let query = format!("hf={}", foreign.token);
    assert_eq!(
        serve(&s.vault, &s.host, "GET", &path, Some(&query)).status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        serve(&s.vault, &s.host, "POST", &path, None).status,
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        serve(&s.vault, &s.host, "GET", "/not-a-uuid/index.html", None).status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        serve(
            &s.vault,
            &s.host,
            "GET",
            &format!("/{}/%zz", url_id(s.extension)),
            None
        )
        .status,
        StatusCode::NOT_FOUND
    );
}
