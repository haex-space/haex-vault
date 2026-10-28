//! The drives of a Linux desktop through udisks2 on the system D-Bus (spec 064 research R1), with
//! zbus's blocking API. Every call blocks; callers run it off the async executor. Mounting needs no
//! rights of haex's own: polkit asks the user where the system wants it (FR-006).

use std::collections::HashMap;

use zbus::blocking::fdo::ObjectManagerProxy;
use zbus::blocking::{Connection, Proxy};
use zbus::fdo::ManagedObjects;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::backend::{DriveBackend, DriveError, UnlockMode};
use super::udisks_classify::{classify, BlockProps, DriveProps, EncProps, FsProps, UdisksObject};
use super::{space, Drive, DriveState};

const SERVICE: &str = "org.freedesktop.UDisks2";
const ROOT: &str = "/org/freedesktop/UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const ENCRYPTED: &str = "org.freedesktop.UDisks2.Encrypted";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";

pub struct UdisksBackend {
    connection: Connection,
}

impl UdisksBackend {
    /// Connects to the system bus; whether udisks2 answers shows on the first list.
    pub fn connect() -> zbus::Result<Self> {
        Ok(Self {
            connection: Connection::system()?,
        })
    }

    fn objects(&self) -> Result<Vec<UdisksObject>, DriveError> {
        let manager = ObjectManagerProxy::builder(&self.connection)
            .destination(SERVICE)
            .and_then(|builder| builder.path(ROOT))
            .and_then(|builder| builder.build())
            .map_err(from_zbus)?;
        let managed = manager
            .get_managed_objects()
            .map_err(|error| DriveError::Failed(error.to_string()))?;
        Ok(to_objects(managed))
    }

    fn proxy(&self, path: &str, interface: &'static str) -> Result<Proxy<'_>, DriveError> {
        Proxy::new(&self.connection, SERVICE, path.to_owned(), interface).map_err(from_zbus)
    }

    /// The listed drive with `id`, so a window can only act on what the list showed.
    fn listed(&self, id: &str) -> Result<Drive, DriveError> {
        self.list()?
            .into_iter()
            .find(|drive| drive.id == id)
            .ok_or(DriveError::Gone)
    }
}

impl DriveBackend for UdisksBackend {
    fn list(&self) -> Result<Vec<Drive>, DriveError> {
        let mut drives = classify(&self.objects()?);
        for drive in drives.iter_mut().filter(|d| d.state == DriveState::Mounted) {
            if let Some((total, free)) = drive.path.as_deref().and_then(|p| space(p.as_ref())) {
                drive.total = Some(total);
                drive.free = Some(free);
            }
        }
        Ok(drives)
    }

    fn mount(&self, id: &str) -> Result<String, DriveError> {
        let drive = self.listed(id)?;
        match drive.state {
            DriveState::Mounted => drive.path.ok_or(DriveError::Gone),
            DriveState::Locked => Err(DriveError::Unsupported),
            DriveState::Unmounted => self
                .proxy(id, FILESYSTEM)?
                .call("Mount", &(no_options(),))
                .map_err(from_zbus),
        }
    }

    fn unlock_mode(&self) -> UnlockMode {
        UnlockMode::Passphrase
    }
}

fn no_options() -> HashMap<&'static str, Value<'static>> {
    HashMap::new()
}

fn from_zbus(error: zbus::Error) -> DriveError {
    match error {
        zbus::Error::MethodError(name, message, _) => {
            map_error(name.as_str(), message.as_deref().unwrap_or_default())
        }
        other => DriveError::Failed(other.to_string()),
    }
}

/// A udisks2 error name to what the window shows (contracts/commands.md).
pub fn map_error(name: &str, message: &str) -> DriveError {
    match name.strip_prefix("org.freedesktop.UDisks2.Error.") {
        Some("NotAuthorizedDismissed" | "Cancelled") => DriveError::Cancelled,
        Some("DeviceBusy") => DriveError::Busy,
        _ if matches!(
            name,
            "org.freedesktop.DBus.Error.UnknownObject" | "org.freedesktop.DBus.Error.UnknownMethod"
        ) =>
        {
            DriveError::Gone
        }
        _ => DriveError::Failed(if message.is_empty() {
            name.to_owned()
        } else {
            message.to_owned()
        }),
    }
}

/// The objects of `GetManagedObjects` with the properties the list reads.
pub fn to_objects(managed: ManagedObjects) -> Vec<UdisksObject> {
    managed
        .into_iter()
        .map(|(path, interfaces)| {
            let interfaces: HashMap<&str, &HashMap<String, OwnedValue>> = interfaces
                .iter()
                .map(|(name, props)| (name.as_str(), props))
                .collect();
            UdisksObject {
                path: path.to_string(),
                block: interfaces.get(BLOCK).map(|props| BlockProps {
                    label: text(props, "IdLabel"),
                    uuid: text(props, "IdUUID"),
                    hint_ignore: flag(props, "HintIgnore"),
                    hint_system: flag(props, "HintSystem"),
                    crypto_backing: object_path(props, "CryptoBackingDevice"),
                    size: props
                        .get("Size")
                        .and_then(|value| u64::try_from(value).ok())
                        .unwrap_or(0),
                    drive: object_path(props, "Drive"),
                }),
                filesystem: interfaces.get(FILESYSTEM).map(|props| FsProps {
                    mount_points: mount_points(props),
                }),
                encrypted: interfaces.get(ENCRYPTED).map(|props| EncProps {
                    cleartext: object_path(props, "CleartextDevice"),
                }),
                drive: interfaces.get(DRIVE).map(|props| DriveProps {
                    removable: flag(props, "Removable"),
                }),
            }
        })
        .collect()
}

fn text(props: &HashMap<String, OwnedValue>, name: &str) -> String {
    props
        .get(name)
        .and_then(|value| <&str>::try_from(value).ok())
        .unwrap_or_default()
        .to_owned()
}

fn flag(props: &HashMap<String, OwnedValue>, name: &str) -> bool {
    props
        .get(name)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

/// An object path property; `/` means none in udisks2.
fn object_path(props: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    props
        .get(name)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| OwnedObjectPath::try_from(value).ok())
        .map(|path| path.to_string())
        .filter(|path| path != "/")
}

/// `MountPoints` is an array of NUL-terminated byte strings.
fn mount_points(props: &HashMap<String, OwnedValue>) -> Vec<String> {
    props
        .get("MountPoints")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<Vec<u8>>::try_from(value).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|bytes| {
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        })
        .filter(|point| !point.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "udisks_tests.rs"]
mod tests;
