//! Encrypted folders on the device for agents (spec 048 US7, FR-050, T079): the raw directory is
//! blocked and never listed, the device as a storage follows the grant "files of the device", and a
//! folder there needs its own grant, capped by the device's.

use std::path::PathBuf;

use super::*;
use crate::files::FilesErrorCode;

const RAW: &str = "/home/anna/abcdefghijklmnopqrstuvwxyz.hxef";
const DEVICE: &str = "device:/";
const FOLDER: &str = "device:/abc";

fn refused(verdict: Verdict) -> FilesErrorCode {
    match verdict {
        Verdict::Refused(error) => error.code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn external() -> Caller {
    Caller::ExternalAgent {
        id: "claude-code".to_owned(),
    }
}

fn raw(rest: &str) -> Target {
    Target::Device(PathBuf::from(format!("{RAW}{rest}")))
}

#[test]
fn the_raw_directory_of_an_encrypted_folder_is_blocked_for_everyone() {
    let grants = AgentGrants::default();
    for rest in ["", "/h", "/m/abcdefghijklmnopqrstuvwxyz"] {
        for want in [Want::Read, Want::Write] {
            assert_eq!(
                refused(check(
                    &Caller::BuiltinAgent,
                    &raw(rest),
                    want,
                    &OwnPlaces::default(),
                    &grants
                )),
                FilesErrorCode::Blocked
            );
            assert_eq!(
                refused(check(
                    &Caller::User,
                    &raw(rest),
                    want,
                    &OwnPlaces::default(),
                    &grants
                )),
                FilesErrorCode::NotFound
            );
        }
    }
    assert!(!visible_to(
        &Caller::BuiltinAgent,
        &PathBuf::from(RAW),
        &OwnPlaces::default(),
        &grants
    ));
    assert!(visible_to(
        &Caller::BuiltinAgent,
        &PathBuf::from("/home/anna/Unterlagen"),
        &OwnPlaces::default(),
        &grants
    ));
}

#[test]
fn the_device_as_a_storage_follows_the_device_grant() {
    let device = Target::Storage(DEVICE.to_owned());
    let own = OwnPlaces::default();
    assert!(is_device_storage(DEVICE) && is_device_storage("device:C:"));
    assert!(!is_device_storage("s1"));
    // The built-in agent has the device from the start; an external one only once granted.
    let none = AgentGrants::default();
    assert_eq!(
        check(&Caller::BuiltinAgent, &device, Want::Write, &own, &none),
        Verdict::Allowed
    );
    assert_eq!(
        refused(check(&external(), &device, Want::Read, &own, &none)),
        FilesErrorCode::NotGranted
    );
    let revoked = AgentGrants {
        device: Some(DeviceGrant::Denied),
        ..AgentGrants::default()
    };
    assert_eq!(
        refused(check(
            &Caller::BuiltinAgent,
            &device,
            Want::Read,
            &own,
            &revoked
        )),
        FilesErrorCode::NotGranted
    );
}

#[test]
fn a_folder_on_the_device_needs_its_own_grant_capped_by_the_device() {
    let folder = |grants: &AgentGrants, caller: &Caller| {
        check_folder(
            caller,
            DEVICE,
            FOLDER,
            Want::Read,
            grants,
            None,
            Locality::Local,
        )
    };
    // The device is granted, the folder not yet: a question.
    let mut grants = AgentGrants::default();
    assert_eq!(folder(&grants, &Caller::BuiltinAgent), Verdict::Ask);
    grants.folders.insert(
        FOLDER.to_owned(),
        FolderGrant {
            grant: StorageGrant::Read,
            reach: Locality::Cloud,
        },
    );
    assert_eq!(folder(&grants, &Caller::BuiltinAgent), Verdict::Allowed);
    // Without the device, the folder's grant alone does nothing.
    assert_eq!(
        refused(folder(&grants, &external())),
        FilesErrorCode::NotGranted
    );
    grants.device = Some(DeviceGrant::Denied);
    assert_eq!(
        refused(folder(&grants, &Caller::BuiltinAgent)),
        FilesErrorCode::NotGranted
    );
}
