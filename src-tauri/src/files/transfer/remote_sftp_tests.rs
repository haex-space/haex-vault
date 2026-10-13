//! Copies inside one SFTP storage (spec 050 US5, T083): the server cannot copy, so the bytes
//! stream through holzi in parts with progress, and a cancelled copy leaves nothing under the
//! target name and no temporary file on the server.

use std::sync::Arc;

use super::tests::{job, run_job};
use super::*;
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::local::Outcome;
use crate::remote_storage::sftp::test_server::{self, Options, TestServer};
use crate::remote_storage::sftp::SftpStore;
use crate::remote_storage::test_support::resolver;
use crate::remote_storage::{
    Access, Addressing, Credentials, EndpointScope, Location, PasswordCredentials, Protocol,
    ProviderKind,
};

fn access(server: &TestServer) -> Access {
    Access {
        location: Location {
            protocol: Protocol::Sftp,
            provider_kind: ProviderKind::Other,
            endpoint: server.endpoint(),
            endpoint_scope: EndpointScope::Local,
            region: String::new(),
            addressing: Addressing::Path,
            bucket: "Ablage".to_owned(),
            host_key: Some(server.host_key.clone()),
        },
        credentials: Credentials::Password(PasswordCredentials {
            username: "anna".to_owned(),
            password: zeroize::Zeroizing::new("password".to_owned()),
        }),
    }
}

fn names(server: &TestServer, folder: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(server.path(folder))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

async fn scene() -> (TestServer, Side) {
    let server = test_server::start(Options::new(31)).await;
    std::fs::create_dir_all(server.path("Ablage/Alt")).unwrap();
    let body: Vec<u8> = (0..=255u8).cycle().take(2 * PART + 17).collect();
    std::fs::write(server.path("Ablage/Alt/film.bin"), &body).unwrap();
    let store = Arc::new(SftpStore::new(resolver(&[])));
    let files = StorageFiles::new(store, access(&server));
    (server, Side::Storage(Arc::new(files)))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_inside_one_sftp_storage_streams_through_holzi_in_parts() {
    let (server, side) = scene().await;
    let Side::Storage(files) = &side else {
        unreachable!()
    };
    let target = Side::Storage(Arc::clone(files));
    let (outcome, _) = run_job(
        job(TransferOp::Copy, side, &["/Alt/film.bin"], target, "/Neu"),
        vec![],
        None,
    )
    .await;
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(
        std::fs::read(server.path("Ablage/Neu/film.bin")).unwrap(),
        std::fs::read(server.path("Ablage/Alt/film.bin")).unwrap()
    );
    assert_eq!(
        names(&server, "Ablage/Neu"),
        ["film.bin"],
        "no temporary file"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_copy_leaves_nothing_under_the_target_name() {
    let (server, side) = scene().await;
    let Side::Storage(files) = &side else {
        unreachable!()
    };
    let target = Side::Storage(Arc::clone(files));
    let (outcome, _) = run_job(
        job(TransferOp::Copy, side, &["/Alt/film.bin"], target, "/Neu"),
        vec![],
        Some(1),
    )
    .await;
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(
        names(&server, "Ablage/Neu").is_empty(),
        "no file and no temporary file: {:?}",
        names(&server, "Ablage/Neu")
    );
    assert_eq!(
        names(&server, "Ablage/Alt"),
        ["film.bin"],
        "the source stays"
    );
}
