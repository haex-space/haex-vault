//! The drives of Windows (spec 064 research R6): the volumes with a drive letter, their kind from
//! the storage bus, ejecting through the PnP manager with a veto buffer (so Windows shows no dialog
//! of its own and says why it refused), and BitLocker To Go through the system's own unlock dialog
//! (FR-019). Volumes without a letter stay hidden; internal drives offer no unmount, which would
//! need an administrator. The pure parts come first, so their tests run on every platform.

use super::backend::DriveError;
use super::{Drive, DriveKind, DriveRelease, DriveState};

/// What `GetVolumeInformationW` fails with on a locked BitLocker volume, also for a standard user
/// (spike T070).
pub const FVE_E_LOCKED_VOLUME: u32 = 0x8031_0000;

/// `STORAGE_BUS_TYPE` values of removable media: USB, SD, MMC.
const REMOVABLE_BUSES: [i32; 3] = [7, 12, 13];

/// The paths of `GetVolumePathNamesForVolumeNameW`: strings separated by NUL, ending in two.
pub fn path_names(buffer: &[u16]) -> Vec<String> {
    buffer
        .split(|&unit| unit == 0)
        .filter(|name| !name.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// A drive root such as `E:\`, the only kind of path a drive is shown at.
pub fn is_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() == 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
}

/// System for the Windows directory's drive, removable for USB, SD and MMC or removable media,
/// internal otherwise. Without a bus (the query failed) it is internal.
pub fn kind_of(bus: Option<i32>, removable_media: bool, system: bool) -> DriveKind {
    if system {
        DriveKind::System
    } else if removable_media || bus.is_some_and(|bus| REMOVABLE_BUSES.contains(&bus)) {
        DriveKind::Removable
    } else {
        DriveKind::Internal
    }
}

/// The volume serial as the stable key, lowercase hex.
pub fn key_of(serial: u32) -> String {
    format!("{serial:08x}")
}

/// What the backend learned about one lettered volume.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeFacts {
    /// `E:\`; also the drive's id.
    pub root: String,
    pub label: Option<String>,
    pub serial: Option<u32>,
    pub locked: bool,
    pub bus: Option<i32>,
    pub removable_media: bool,
    pub system: bool,
    /// Total and free bytes.
    pub space: Option<(u64, u64)>,
}

/// The drive the sidebar shows for `facts`. Every lettered volume is mounted unless BitLocker
/// keeps it locked.
pub fn drive_of(facts: VolumeFacts) -> Drive {
    let kind = kind_of(facts.bus, facts.removable_media, facts.system);
    let state = if facts.locked {
        DriveState::Locked
    } else {
        DriveState::Mounted
    };
    let (total, free) = facts.space.unzip();
    Drive {
        id: facts.root.clone(),
        key: facts.serial.map(key_of),
        label: facts.label.filter(|label| !label.is_empty()),
        path: (!facts.locked).then(|| facts.root.clone()),
        kind,
        state,
        release: if kind == DriveKind::Removable && state == DriveState::Mounted {
            DriveRelease::Eject
        } else {
            DriveRelease::None
        },
        encrypted: facts.locked,
        size: total,
        total,
        free,
    }
}

/// What a `PNP_VETO_TYPE` from `CM_Request_Device_EjectW` means; `None` for no veto. `name` is the
/// vetoing program or device, for the log.
pub fn veto_error(veto: i32, name: &str) -> Option<DriveError> {
    match veto {
        0 => None,
        // Pending close, an application, a service, an open handle, another device or driver.
        1..=7 | 11 => {
            log::info!("files: ejecting refused by {name:?} (veto {veto})");
            Some(DriveError::Busy)
        }
        13 => Some(DriveError::Gone),
        _ => Some(DriveError::Failed(format!(
            "Windows did not eject it (veto {veto}: {name})"
        ))),
    }
}

#[cfg(windows)]
pub use system::WindowsBackend;

#[cfg(windows)]
mod system {
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Parent, CM_Request_Device_EjectW, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
        CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, PNP_VETO_TYPE,
        SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW,
        GetVolumeInformationW, GetVolumePathNamesForVolumeNameW, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceProperty, GUID_DEVINTERFACE_DISK,
        IOCTL_STORAGE_GET_DEVICE_NUMBER, IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR,
        STORAGE_DEVICE_NUMBER, STORAGE_PROPERTY_QUERY,
    };
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::super::backend::{DriveBackend, DriveError, UnlockMode};
    use super::super::{Drive, DriveRelease, DriveState};
    use super::{
        drive_of, is_drive_root, path_names, veto_error, VolumeFacts, FVE_E_LOCKED_VOLUME,
    };

    pub struct WindowsBackend;

    impl WindowsBackend {
        /// The listed drive with `id`, so a window can only act on what the list showed.
        fn listed(&self, id: &str) -> Result<Drive, DriveError> {
            self.list()?
                .into_iter()
                .find(|drive| drive.id == id)
                .ok_or(DriveError::Gone)
        }
    }

    impl DriveBackend for WindowsBackend {
        fn list(&self) -> Result<Vec<Drive>, DriveError> {
            let windows_root = windows_root();
            Ok(volume_names()?
                .into_iter()
                .filter_map(|volume| facts(&volume, windows_root.as_deref()))
                .map(drive_of)
                .collect())
        }

        /// Ejects the disk the volume lies on; a veto leaves it in place and says why.
        fn release(&self, id: &str) -> Result<bool, DriveError> {
            let drive = self.listed(id)?;
            if drive.state != DriveState::Mounted || drive.release != DriveRelease::Eject {
                return Err(DriveError::Unsupported);
            }
            let disk = device_number(&device_path(id))
                .and_then(disk_instance)
                .ok_or(DriveError::Gone)?;
            match eject(disk) {
                Ok(()) => Ok(true),
                // Whether the disk or its parent must be asked depends on the stick (spike
                // T070 a): the parent once more, unless something holds the drive.
                Err(DriveError::Failed(_)) => {
                    let parent = parent_of(disk).ok_or(DriveError::Gone)?;
                    eject(parent).map(|()| true)
                }
                Err(error) => Err(error),
            }
        }

        fn unlock_mode(&self) -> UnlockMode {
            UnlockMode::SystemDialog
        }

        /// The `unlock-bde` verb of the drive opens BitLocker's own dialog (`bdeunlock.exe`).
        fn open_system_unlock(&self, id: &str) -> Result<(), DriveError> {
            if !is_drive_root(id) {
                return Err(DriveError::Gone);
            }
            if shell_verb("unlock-bde", id) {
                return Ok(());
            }
            std::process::Command::new("bdeunlock.exe")
                .arg(id.trim_end_matches('\\'))
                .spawn()
                .map(|_| ())
                .map_err(|error| DriveError::Failed(error.to_string()))
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn until_nul(buffer: &[u16]) -> String {
        let end = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    /// `\\.\E:` for the root `E:\`.
    fn device_path(root: &str) -> String {
        format!("\\\\.\\{}", root.trim_end_matches('\\'))
    }

    /// The `\\?\Volume{…}\` names of every volume.
    fn volume_names() -> Result<Vec<String>, DriveError> {
        let mut name = [0u16; 260];
        // SAFETY: `name` is a writable buffer of the length passed.
        let find = unsafe { FindFirstVolumeW(name.as_mut_ptr(), name.len() as u32) };
        if find == INVALID_HANDLE_VALUE {
            // SAFETY: reads the calling thread's last error, no pointers.
            return Err(DriveError::Failed(format!(
                "FindFirstVolumeW failed: {}",
                unsafe { GetLastError() }
            )));
        }
        let mut names = Vec::new();
        loop {
            names.push(until_nul(&name));
            // SAFETY: `find` is the open search handle; `name` as above.
            if unsafe { FindNextVolumeW(find, name.as_mut_ptr(), name.len() as u32) } == 0 {
                break;
            }
        }
        // SAFETY: `find` came from `FindFirstVolumeW` and is closed once.
        unsafe { FindVolumeClose(find) };
        Ok(names)
    }

    /// The drive root of `volume`, its first path that is one.
    fn root_of(volume: &str) -> Option<String> {
        let name = wide(volume);
        let mut buffer = vec![0u16; 1024];
        let mut length = 0u32;
        // SAFETY: `name` is NUL-terminated; `buffer` is writable for the length passed; `length`
        // receives the characters written.
        let ok = unsafe {
            GetVolumePathNamesForVolumeNameW(
                name.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut length,
            )
        };
        if ok == 0 {
            return None;
        }
        buffer.truncate((length as usize).min(buffer.len()));
        path_names(&buffer)
            .into_iter()
            .find(|path| is_drive_root(path))
    }

    /// What one volume is, `None` when it has no letter or no medium.
    fn facts(volume: &str, windows_root: Option<&str>) -> Option<VolumeFacts> {
        let root = root_of(volume)?;
        let root_w = wide(&root);
        let mut label = [0u16; 261];
        let mut serial = 0u32;
        // SAFETY: `root_w` is NUL-terminated; `label` and `serial` are writable; the other outputs
        // are optional and passed as null.
        let ok = unsafe {
            GetVolumeInformationW(
                root_w.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                &mut serial,
                null_mut(),
                null_mut(),
                null_mut(),
                0,
            )
        };
        let locked = if ok == 0 {
            // SAFETY: reads the calling thread's last error, no pointers.
            let error = unsafe { GetLastError() };
            if error != FVE_E_LOCKED_VOLUME {
                // A card reader without a card, or a volume going away.
                return None;
            }
            true
        } else {
            false
        };
        let (bus, removable_media) = bus_of(&device_path(&root)).unzip();
        Some(VolumeFacts {
            label: (!locked).then(|| until_nul(&label)),
            serial: (!locked).then_some(serial),
            locked,
            bus,
            removable_media: removable_media.unwrap_or(false),
            system: windows_root.is_some_and(|windows| windows.eq_ignore_ascii_case(&root)),
            space: if locked { None } else { space(&root_w) },
            root,
        })
    }

    /// The drive root of the Windows directory, such as `C:\`.
    fn windows_root() -> Option<String> {
        let mut buffer = [0u16; 260];
        // SAFETY: `buffer` is writable for the length passed.
        let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
        let directory = String::from_utf16_lossy(buffer.get(..length as usize)?);
        directory
            .get(..3)
            .map(str::to_owned)
            .filter(|root| is_drive_root(root))
    }

    fn space(root: &[u16]) -> Option<(u64, u64)> {
        let mut free = 0u64;
        let mut total = 0u64;
        // SAFETY: `root` is NUL-terminated; the outputs are writable; the third one is optional.
        let ok = unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut free, &mut total, null_mut()) };
        (ok != 0).then_some((total, free))
    }

    /// `device` opened with access 0, which needs no administrator; closed on drop.
    struct Device(HANDLE);

    impl Device {
        fn open(device: &str) -> Option<Self> {
            let path = wide(device);
            // SAFETY: `path` is NUL-terminated; no security attributes and no template.
            let handle = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    null(),
                    OPEN_EXISTING,
                    0,
                    null_mut(),
                )
            };
            (handle != INVALID_HANDLE_VALUE).then_some(Self(handle))
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            // SAFETY: the handle is open and closed only here.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// The storage bus and whether the medium is removable.
    fn bus_of(device: &str) -> Option<(i32, bool)> {
        let device = Device::open(device)?;
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        // Aligned for the descriptor read below.
        let mut buffer = vec![0u64; 128];
        let mut returned = 0u32;
        // SAFETY: the query is read for its size, the buffer written for its size in bytes.
        let ok = unsafe {
            DeviceIoControl(
                device.0,
                IOCTL_STORAGE_QUERY_PROPERTY,
                (&raw const query).cast(),
                size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * size_of::<u64>()) as u32,
                &mut returned,
                null_mut(),
            )
        };
        if ok == 0 || (returned as usize) < size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            return None;
        }
        // SAFETY: the call filled at least one descriptor into the aligned buffer.
        let descriptor = unsafe { &*buffer.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>() };
        Some((descriptor.BusType, descriptor.RemovableMedia))
    }

    /// The number of the disk `device` lies on.
    fn device_number(device: &str) -> Option<u32> {
        let device = Device::open(device)?;
        // SAFETY: plain data the call fills.
        let mut number: STORAGE_DEVICE_NUMBER = unsafe { std::mem::zeroed() };
        let mut returned = 0u32;
        // SAFETY: no input; the output is written for its size.
        let ok = unsafe {
            DeviceIoControl(
                device.0,
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                null(),
                0,
                (&raw mut number).cast(),
                size_of::<STORAGE_DEVICE_NUMBER>() as u32,
                &mut returned,
                null_mut(),
            )
        };
        (ok != 0).then_some(number.DeviceNumber)
    }

    /// The device set of the present disks; destroyed on drop.
    struct DiskSet(HDEVINFO);

    impl Drop for DiskSet {
        fn drop(&mut self) {
            // SAFETY: the set is valid and destroyed only here.
            unsafe { SetupDiDestroyDeviceInfoList(self.0) };
        }
    }

    /// The device instance of the disk with `number`.
    fn disk_instance(number: u32) -> Option<u32> {
        // SAFETY: a class GUID, no enumerator, no window.
        let set = unsafe {
            SetupDiGetClassDevsW(
                &GUID_DEVINTERFACE_DISK,
                null(),
                null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        };
        if set == INVALID_HANDLE_VALUE as HDEVINFO {
            return None;
        }
        let set = DiskSet(set);
        for index in 0.. {
            // SAFETY: plain data, its size set below as the API demands.
            let mut interface: SP_DEVICE_INTERFACE_DATA = unsafe { std::mem::zeroed() };
            interface.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            // SAFETY: `set` is valid; `interface` is writable with its size set.
            let more = unsafe {
                SetupDiEnumDeviceInterfaces(
                    set.0,
                    null(),
                    &GUID_DEVINTERFACE_DISK,
                    index,
                    &mut interface,
                )
            };
            if more == 0 {
                return None;
            }
            // Aligned for the detail structure, whose path follows its size field.
            let mut buffer = vec![0u64; 512];
            let detail = buffer
                .as_mut_ptr()
                .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            // SAFETY: the buffer is large and aligned enough for the structure's head.
            unsafe { (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
            // SAFETY: plain data, its size set below.
            let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
            info.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
            // SAFETY: `detail` points into `buffer`, writable for the size passed.
            let ok = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    set.0,
                    &interface,
                    detail,
                    (buffer.len() * size_of::<u64>()) as u32,
                    null_mut(),
                    &mut info,
                )
            };
            if ok == 0 {
                continue;
            }
            // SAFETY: the call wrote a NUL-terminated path inside `buffer`.
            let path = unsafe {
                let start = (&raw const (*detail).DevicePath).cast::<u16>();
                let room = (buffer.len() * size_of::<u64>()
                    - (start as usize - buffer.as_ptr() as usize))
                    / size_of::<u16>();
                until_nul(std::slice::from_raw_parts(start, room))
            };
            if device_number(&path) == Some(number) {
                return Some(info.DevInst);
            }
        }
        None
    }

    fn parent_of(instance: u32) -> Option<u32> {
        let mut parent = 0u32;
        // SAFETY: `parent` is writable; `instance` came from SetupAPI.
        (unsafe { CM_Get_Parent(&mut parent, instance, 0) } == CR_SUCCESS).then_some(parent)
    }

    /// Asks the PnP manager to eject `instance`; the veto buffer keeps Windows from asking itself.
    fn eject(instance: u32) -> Result<(), DriveError> {
        let mut veto: PNP_VETO_TYPE = 0;
        let mut name = [0u16; 260];
        // SAFETY: `veto` and `name` are writable, `name` for the length passed.
        let result = unsafe {
            CM_Request_Device_EjectW(instance, &mut veto, name.as_mut_ptr(), name.len() as u32, 0)
        };
        if let Some(error) = veto_error(veto, &until_nul(&name)) {
            return Err(error);
        }
        if result == CR_SUCCESS {
            Ok(())
        } else {
            Err(DriveError::Failed(format!("ejecting failed ({result})")))
        }
    }

    /// Runs the shell verb `verb` on `file`; whether it started.
    fn shell_verb(verb: &str, file: &str) -> bool {
        let verb = wide(verb);
        let file = wide(file);
        // SAFETY: plain data, its size set below.
        let mut execute: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        execute.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        execute.fMask = SEE_MASK_FLAG_NO_UI;
        execute.lpVerb = verb.as_ptr();
        execute.lpFile = file.as_ptr();
        execute.nShow = SW_SHOWNORMAL;
        // SAFETY: `execute` and the strings it points to outlive the call.
        unsafe { ShellExecuteExW(&mut execute) != 0 }
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
