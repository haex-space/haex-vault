use std::path::PathBuf;

use super::*;
#[cfg(unix)]
use crate::files::local::resolve;
use crate::files::local::OwnPlaces;
use crate::files::FilesErrorCode;

fn own() -> OwnPlaces {
    OwnPlaces::new(vec![PathBuf::from("/data/holzi")])
}

fn device(path: &str) -> Target {
    Target::Device(PathBuf::from(path))
}

fn storage(id: &str) -> Target {
    Target::Storage(id.to_owned())
}

fn refused(verdict: Verdict) -> FilesErrorCode {
    match verdict {
        Verdict::Refused(error) => error.code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn builtin() -> Caller {
    Caller::BuiltinAgent
}

fn external() -> Caller {
    Caller::ExternalAgent {
        id: "claude-code".to_owned(),
    }
}

// The user

#[test]
fn the_user_reaches_the_device_and_storages() {
    let grants = AgentGrants::default();
    for want in [Want::Read, Want::Write] {
        assert_eq!(
            check(
                &Caller::User,
                &device("/home/anna/a.txt"),
                want,
                &own(),
                &grants
            ),
            Verdict::Allowed
        );
        assert_eq!(
            check(&Caller::User, &storage("s1"), want, &own(), &grants),
            Verdict::Allowed
        );
    }
}

#[test]
fn the_user_sees_holzis_own_places_read_only() {
    let grants = AgentGrants::default();
    let target = device("/data/holzi/instances/a.db");
    assert_eq!(
        check(&Caller::User, &target, Want::Read, &own(), &grants),
        Verdict::ReadOnly
    );
    assert_eq!(
        refused(check(&Caller::User, &target, Want::Write, &own(), &grants)),
        FilesErrorCode::HolziOwned
    );
}

// Agents on the device

#[test]
fn the_builtin_agent_has_the_device_from_the_start() {
    let grants = AgentGrants::default();
    for want in [Want::Read, Want::Write] {
        assert_eq!(
            check(
                &builtin(),
                &device("/home/anna/a.txt"),
                want,
                &own(),
                &grants
            ),
            Verdict::Allowed
        );
    }
}

#[test]
fn an_external_agent_needs_the_device_granted() {
    let target = device("/home/anna/a.txt");
    assert_eq!(
        refused(check(
            &external(),
            &target,
            Want::Read,
            &own(),
            &AgentGrants::default()
        )),
        FilesErrorCode::NotGranted
    );
    let granted = AgentGrants {
        device: Some(DeviceGrant::Granted),
        ..AgentGrants::default()
    };
    assert_eq!(
        check(&external(), &target, Want::Read, &own(), &granted),
        Verdict::Allowed
    );
}

#[test]
fn a_revoked_device_grant_holds_for_the_builtin_agent_too() {
    let denied = AgentGrants {
        device: Some(DeviceGrant::Denied),
        ..AgentGrants::default()
    };
    assert_eq!(
        refused(check(
            &builtin(),
            &device("/home/anna/a.txt"),
            Want::Read,
            &own(),
            &denied
        )),
        FilesErrorCode::NotGranted
    );
}

/// SC-005: no agent reaches holzi's own places, whatever the spelling, and the refusal comes
/// before any grant question.
#[test]
fn agents_never_reach_holzis_own_places() {
    let granted = AgentGrants {
        device: Some(DeviceGrant::Granted),
        ..AgentGrants::default()
    };
    for caller in [builtin(), external()] {
        for path in [
            "/data/holzi",
            "/data/holzi/instances/a.db",
            "/data/holzi/models/x.gguf",
        ] {
            for want in [Want::Read, Want::Write] {
                assert_eq!(
                    refused(check(&caller, &device(path), want, &own(), &granted)),
                    FilesErrorCode::Blocked,
                    "{caller:?} {path} {want:?}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn spelling_and_links_do_not_lead_an_agent_into_an_own_place() {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    let holzi = real.join("holzi");
    std::fs::create_dir_all(holzi.join("instances")).unwrap();
    std::fs::create_dir_all(real.join("home").join("docs")).unwrap();
    std::os::unix::fs::symlink(&holzi, real.join("home").join("shortcut")).unwrap();
    let own = OwnPlaces::new(vec![holzi.clone()]);
    let grants = AgentGrants::default();
    for spelled in [
        real.join("home")
            .join("docs")
            .join("..")
            .join("..")
            .join("holzi"),
        real.join("home").join("shortcut").join("instances"),
        real.join("home").join("shortcut").join("new.db"),
    ] {
        let target = Target::Device(resolve(&spelled).unwrap());
        assert_eq!(
            refused(check(&builtin(), &target, Want::Read, &own, &grants)),
            FilesErrorCode::Blocked,
            "{spelled:?}"
        );
    }
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn letter_case_does_not_lead_an_agent_into_an_own_place() {
    let target = device("/DATA/Holzi/instances/a.db");
    assert_eq!(
        refused(check(
            &builtin(),
            &target,
            Want::Read,
            &own(),
            &AgentGrants::default()
        )),
        FilesErrorCode::Blocked
    );
}

#[test]
fn a_search_hit_inside_an_own_place_is_hidden_from_agents() {
    let grants = AgentGrants::default();
    assert!(!visible_to(
        &builtin(),
        &PathBuf::from("/data/holzi/instances/a.db"),
        &own(),
        &grants
    ));
    assert!(visible_to(
        &builtin(),
        &PathBuf::from("/home/anna/a.txt"),
        &own(),
        &grants
    ));
    assert!(visible_to(
        &Caller::User,
        &PathBuf::from("/data/holzi/instances/a.db"),
        &own(),
        &grants
    ));
}

// Agents on storages

#[test]
fn a_storage_without_a_grant_is_a_question() {
    let grants = AgentGrants::default();
    assert_eq!(
        check(&builtin(), &storage("s1"), Want::Read, &own(), &grants),
        Verdict::Ask
    );
    assert_eq!(
        check(&builtin(), &storage("s1"), Want::Write, &own(), &grants),
        Verdict::Ask
    );
}

#[test]
fn a_read_grant_reads_but_never_writes() {
    let grants = AgentGrants::default().with_storage("s1", StorageGrant::Read);
    assert_eq!(
        check(&builtin(), &storage("s1"), Want::Read, &own(), &grants),
        Verdict::Allowed
    );
    assert_eq!(
        refused(check(
            &builtin(),
            &storage("s1"),
            Want::Write,
            &own(),
            &grants
        )),
        FilesErrorCode::NotGranted
    );
}

#[test]
fn a_read_write_grant_covers_both() {
    let grants = AgentGrants::default().with_storage("s1", StorageGrant::ReadWrite);
    for want in [Want::Read, Want::Write] {
        assert_eq!(
            check(&external(), &storage("s1"), want, &own(), &grants),
            Verdict::Allowed
        );
    }
}

#[test]
fn a_denied_storage_is_refused_without_a_question() {
    let grants = AgentGrants::default().with_storage("s1", StorageGrant::Denied);
    assert_eq!(
        refused(check(
            &builtin(),
            &storage("s1"),
            Want::Read,
            &own(),
            &grants
        )),
        FilesErrorCode::NotGranted
    );
}

#[test]
fn a_grant_for_one_storage_says_nothing_about_another() {
    let grants = AgentGrants::default().with_storage("s1", StorageGrant::ReadWrite);
    assert_eq!(
        check(&builtin(), &storage("s2"), Want::Read, &own(), &grants),
        Verdict::Ask
    );
}

#[test]
fn only_storages_with_a_grant_are_listed_for_agents() {
    let grants = AgentGrants::default()
        .with_storage("s1", StorageGrant::Read)
        .with_storage("s2", StorageGrant::Denied);
    assert!(storage_listed(&builtin(), "s1", &grants));
    assert!(!storage_listed(&builtin(), "s2", &grants));
    assert!(!storage_listed(&builtin(), "s3", &grants));
    assert!(storage_listed(&Caller::User, "s3", &grants));
}

// Callers the file browser does not serve

#[test]
fn extensions_and_internal_callers_use_their_own_entrances() {
    let grants = AgentGrants::default();
    for caller in [
        Caller::Extension { id: "x".to_owned() },
        Caller::Internal { feature: "storage" },
    ] {
        assert_eq!(
            refused(check(
                &caller,
                &device("/home/anna/a.txt"),
                Want::Read,
                &own(),
                &grants
            )),
            FilesErrorCode::NotGranted
        );
    }
}

// Encrypted folders (spec 048 FR-030, FR-030a, FR-031)

const FOLDER: &str = "s1/abc";

fn folder_grants(storage: StorageGrant, folder: Option<(StorageGrant, Locality)>) -> AgentGrants {
    let mut grants = AgentGrants::default().with_storage("s1", storage);
    if let Some((grant, reach)) = folder {
        grants
            .folders
            .insert(FOLDER.to_owned(), FolderGrant { grant, reach });
    }
    grants
}

fn folder(
    grants: &AgentGrants,
    want: Want,
    held: Option<StorageGrant>,
    model: Locality,
) -> Verdict {
    check_folder(&builtin(), "s1", FOLDER, want, grants, held, model)
}

#[test]
fn a_folder_without_a_row_asks_even_with_the_storage_granted() {
    let grants = folder_grants(StorageGrant::ReadWrite, None);
    for want in [Want::Read, Want::Write] {
        assert_eq!(folder(&grants, want, None, Locality::Local), Verdict::Ask);
    }
    // External agents follow the same rules (FR-034).
    assert_eq!(
        check_folder(
            &external(),
            "s1",
            FOLDER,
            Want::Read,
            &grants,
            None,
            Locality::Cloud
        ),
        Verdict::Ask
    );
}

#[test]
fn a_folder_grant_never_exceeds_the_storage_grant() {
    let grants = folder_grants(
        StorageGrant::Read,
        Some((StorageGrant::ReadWrite, Locality::Cloud)),
    );
    assert_eq!(
        folder(&grants, Want::Read, None, Locality::Local),
        Verdict::Allowed
    );
    assert_eq!(
        refused(folder(&grants, Want::Write, None, Locality::Local)),
        FilesErrorCode::NotGranted
    );
    let denied = folder_grants(
        StorageGrant::Denied,
        Some((StorageGrant::ReadWrite, Locality::Cloud)),
    );
    assert_eq!(
        refused(folder(&denied, Want::Read, None, Locality::Local)),
        FilesErrorCode::NotGranted
    );
}

#[test]
fn read_reads_readwrite_writes_and_denied_refuses() {
    let read = folder_grants(
        StorageGrant::ReadWrite,
        Some((StorageGrant::Read, Locality::Cloud)),
    );
    assert_eq!(
        folder(&read, Want::Read, None, Locality::Cloud),
        Verdict::Allowed
    );
    assert_eq!(
        refused(folder(&read, Want::Write, None, Locality::Cloud)),
        FilesErrorCode::ReadOnly
    );
    let write = folder_grants(
        StorageGrant::ReadWrite,
        Some((StorageGrant::ReadWrite, Locality::Cloud)),
    );
    assert_eq!(
        folder(&write, Want::Write, None, Locality::Cloud),
        Verdict::Allowed
    );
    let denied = folder_grants(
        StorageGrant::ReadWrite,
        Some((StorageGrant::Denied, Locality::Cloud)),
    );
    assert_eq!(
        refused(folder(&denied, Want::Read, None, Locality::Local)),
        FilesErrorCode::NotGranted
    );
}

#[test]
fn a_grant_for_local_models_asks_again_for_a_cloud_model() {
    let local = folder_grants(
        StorageGrant::ReadWrite,
        Some((StorageGrant::Read, Locality::Local)),
    );
    assert_eq!(
        folder(&local, Want::Read, None, Locality::Local),
        Verdict::Allowed
    );
    assert_eq!(
        folder(&local, Want::Read, None, Locality::Cloud),
        Verdict::Ask
    );
}

#[test]
fn an_answer_of_the_task_comes_before_the_stored_row() {
    let none = folder_grants(StorageGrant::ReadWrite, None);
    assert_eq!(
        folder(&none, Want::Read, Some(StorageGrant::Read), Locality::Cloud),
        Verdict::Allowed
    );
    assert_eq!(
        refused(folder(
            &none,
            Want::Write,
            Some(StorageGrant::Read),
            Locality::Cloud
        )),
        FilesErrorCode::ReadOnly
    );
    let stored = folder_grants(
        StorageGrant::ReadWrite,
        Some((StorageGrant::ReadWrite, Locality::Cloud)),
    );
    assert_eq!(
        refused(folder(
            &stored,
            Want::Read,
            Some(StorageGrant::Denied),
            Locality::Local
        )),
        FilesErrorCode::NotGranted
    );
}
