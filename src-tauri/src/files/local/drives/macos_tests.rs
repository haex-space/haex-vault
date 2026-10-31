use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::*;
use crate::files::local::drives::DriveKind;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/files/local/drives/fixtures/macos/arm64")
}

fn bytes(file: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(file)).unwrap()
}

/// A Mac as recorded in the fixtures: `diskutil` answers from a table the test may change.
#[derive(Default)]
struct FakeDiskutil {
    answers: Mutex<HashMap<String, Output>>,
    calls: Mutex<Vec<(String, Option<String>)>>,
    fingerprint: Mutex<Vec<String>>,
}

impl FakeDiskutil {
    /// The recorded scenario `name`: its list, its APFS list and the info of each volume.
    fn scenario(name: &str) -> Self {
        let fake = Self::default();
        let dir = fixtures().join(name);
        fake.answer(
            "list -plist",
            &std::fs::read(dir.join("list.plist")).unwrap(),
        );
        fake.answer(
            "apfs list -plist",
            &std::fs::read(dir.join("apfs-list.plist")).unwrap(),
        );
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            if let Some(id) = name
                .strip_prefix("info-")
                .and_then(|rest| rest.strip_suffix(".plist"))
            {
                fake.answer(&format!("info -plist {id}"), &std::fs::read(&path).unwrap());
            }
        }
        fake
    }

    fn answer(&self, args: &str, stdout: &[u8]) {
        self.answers.lock().unwrap().insert(
            args.to_owned(),
            Output {
                success: true,
                stdout: stdout.to_vec(),
                stderr: String::new(),
            },
        );
    }

    fn fail(&self, args: &str, stderr: &str) {
        self.answers.lock().unwrap().insert(
            args.to_owned(),
            Output {
                success: false,
                stdout: Vec::new(),
                stderr: stderr.to_owned(),
            },
        );
    }

    fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(args, _)| args.clone())
            .collect()
    }
}

impl Diskutil for &FakeDiskutil {
    fn run(&self, args: &[&str], stdin: Option<&str>) -> Result<Output, DriveError> {
        let joined = args.join(" ");
        self.calls
            .lock()
            .unwrap()
            .push((joined.clone(), stdin.map(str::to_owned)));
        Ok(self
            .answers
            .lock()
            .unwrap()
            .get(&joined)
            .cloned()
            .unwrap_or_else(|| Output {
                success: false,
                stdout: Vec::new(),
                stderr: format!("Could not find disk: {joined}"),
            }))
    }

    fn fingerprint(&self) -> Vec<String> {
        self.fingerprint.lock().unwrap().clone()
    }
}

#[test]
fn unlocking_passes_the_password_on_stdin_never_as_an_argument() {
    let args = command_for(Action::Unlock("disk8s1"));
    assert_eq!(
        args,
        ["apfs", "unlockVolume", "disk8s1", "-stdinpassphrase"]
    );
    assert!(!args.contains(&"-passphrase"));

    let fake = FakeDiskutil::scenario("apfs-locked");
    fake.answer(
        "apfs unlockVolume disk8s1 -stdinpassphrase",
        b"Unlocked and mounted APFS Volume\n",
    );
    let backend = MacBackend::new(&fake);
    // Listed locked; from now on `info` sees it unlocked and mounted.
    backend.list().unwrap();
    fake.answer("info -plist disk8s1", &bytes("unlocked-info-disk8s1.plist"));
    let secret = DriveSecret::new("geheim-und-lang".to_owned());
    assert_eq!(
        backend.unlock("disk8s1", &secret).unwrap(),
        "/Volumes/E2EPLAIN"
    );
    let calls = fake.calls.lock().unwrap().clone();
    let (_, stdin) = calls
        .iter()
        .find(|(args, _)| args.starts_with("apfs unlockVolume"))
        .unwrap();
    assert_eq!(stdin.as_deref(), Some("geheim-und-lang"));
    assert!(calls.iter().all(|(args, _)| !args.contains("geheim")));
}

#[test]
fn a_wrong_password_is_an_unlock_failure_with_the_system_text() {
    let fake = FakeDiskutil::scenario("apfs-locked");
    fake.fail(
        "apfs unlockVolume disk8s1 -stdinpassphrase",
        "Passphrase incorrect or user does not exist\n",
    );
    let backend = MacBackend::new(&fake);
    assert_eq!(
        backend.unlock("disk8s1", &DriveSecret::new("falsch".to_owned())),
        Err(DriveError::UnlockFailed(
            "Passphrase incorrect or user does not exist".to_owned()
        ))
    );
}

#[test]
fn the_list_is_read_again_only_when_disks_or_volumes_change() {
    let fake = FakeDiskutil::scenario("fat-unmounted");
    let backend = MacBackend::new(&fake);
    let mut first = backend.list().unwrap();
    assert!(first.iter().any(|d| d.label.as_deref() == Some("E2EFAT")));
    let reads = fake.calls().len();
    let mut cached = backend.list().unwrap();
    // Space is refreshed from the host even when diskutil's volume list is cached.
    for drive in first.iter_mut().chain(cached.iter_mut()) {
        drive.total = None;
        drive.free = None;
    }
    assert_eq!(cached, first);
    assert_eq!(
        fake.calls().len(),
        reads,
        "no diskutil call without a change"
    );

    *fake.fingerprint.lock().unwrap() = vec!["/Volumes/E2EFAT".into()];
    backend.list().unwrap();
    assert!(fake.calls().len() > reads, "a change reads diskutil again");
}

#[test]
fn mounting_mounts_and_answers_the_mount_point() {
    let fake = FakeDiskutil::scenario("fat-unmounted");
    fake.answer("mount disk7s1", b"Volume E2EFAT on disk7s1 mounted\n");
    let backend = MacBackend::new(&fake);
    backend.list().unwrap();
    fake.answer(
        "info -plist disk7s1",
        &bytes("fat-mounted-info-disk7s1.plist"),
    );
    assert_eq!(backend.mount("disk7s1").unwrap(), "/Volumes/E2EFAT");
    assert!(fake.calls().contains(&"mount disk7s1".to_owned()));
}

#[test]
fn releasing_a_stick_unmounts_it_and_ejects_its_disk() {
    let fake = FakeDiskutil::scenario("fat-unmounted");
    fake.answer(
        "info -plist disk7s1",
        &bytes("fat-mounted-info-disk7s1.plist"),
    );
    fake.answer("unmount disk7s1", b"Volume E2EFAT on disk7s1 unmounted\n");
    fake.answer("eject disk7", b"Disk disk7 ejected\n");
    let backend = MacBackend::new(&fake);
    let stick = backend
        .list()
        .unwrap()
        .into_iter()
        .find(|d| d.id == "disk7s1")
        .unwrap();
    assert_eq!(stick.kind, DriveKind::Removable);
    assert_eq!(backend.release("disk7s1"), Ok(true));
    let calls = fake.calls();
    let unmount = calls.iter().position(|c| c == "unmount disk7s1").unwrap();
    let eject = calls.iter().position(|c| c == "eject disk7").unwrap();
    assert!(unmount < eject, "{calls:?}");
}

#[test]
fn a_drive_in_use_stays_and_says_busy() {
    let fake = FakeDiskutil::scenario("fat-unmounted");
    fake.answer(
        "info -plist disk7s1",
        &bytes("fat-mounted-info-disk7s1.plist"),
    );
    fake.fail(
        "unmount disk7s1",
        "Volume E2EFAT on disk7s1 failed to unmount: dissented by PID 2954 (/bin/sleep)\n\
         Dissenter parent PPID 2708 (/bin/bash)\n",
    );
    let backend = MacBackend::new(&fake);
    assert_eq!(backend.release("disk7s1"), Err(DriveError::Busy));
    assert!(!fake.calls().iter().any(|c| c.starts_with("eject")));
}

#[test]
fn a_volume_that_is_gone_is_gone() {
    let fake = FakeDiskutil::scenario("fat-unmounted");
    let backend = MacBackend::new(&fake);
    assert_eq!(backend.mount("disk42s1"), Err(DriveError::Gone));
    assert_eq!(
        error_from(&Output {
            stderr: "Could not find disk: disk42s1".into(),
            ..Default::default()
        }),
        DriveError::Gone
    );
    assert_eq!(
        error_from(&Output {
            stderr: "Unmount failed for disk7s1\n".into(),
            ..Default::default()
        }),
        DriveError::Failed("Unmount failed for disk7s1".into())
    );
}

/// On the Mac itself: the real `diskutil` lists `/` as the system drive and nothing macOS keeps for
/// itself.
#[cfg(target_os = "macos")]
#[test]
fn the_real_diskutil_lists_the_system_drive() {
    let drives = MacBackend::new(SystemDiskutil).list().unwrap();
    assert!(
        drives.iter().all(super::super::invariants_hold),
        "{drives:#?}"
    );
    let system: Vec<_> = drives
        .iter()
        .filter(|d| d.kind == DriveKind::System)
        .collect();
    assert_eq!(system.len(), 1, "{drives:#?}");
    assert_eq!(system[0].path.as_deref(), Some("/"));
    assert!(
        drives.iter().all(|d| d
            .path
            .as_deref()
            .is_none_or(|path| path == "/" || path.starts_with("/Volumes/"))),
        "{drives:#?}"
    );
}
