//! The Android side: a handle to `HolziAndroidPlugin.kt`.

use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::plugin::{PluginApi, PluginHandle};
use tauri::{AppHandle, Runtime};

use std::collections::HashMap;

use crate::contacts::{ApplyResult, Op, PhoneAccount, RawData, RawIndex};
use crate::{Insets, Permission, PermissionState};

const PLUGIN_IDENTIFIER: &str = "space.haex.holzi.android";

pub fn init<R: Runtime>(
    _app: &AppHandle<R>,
    api: PluginApi<R, ()>,
) -> Result<HolziAndroid<R>, Box<dyn std::error::Error>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "HolziAndroidPlugin")?;
    Ok(HolziAndroid(handle))
}

/// The registered Kotlin plugin.
pub struct HolziAndroid<R: Runtime>(PluginHandle<R>);

#[derive(Serialize)]
struct UriArgs<'a> {
    uri: &'a str,
}

#[derive(Deserialize)]
struct DisplayName {
    name: Option<String>,
}

#[derive(Serialize)]
struct SecureArgs {
    enabled: bool,
}

#[derive(Serialize)]
struct InsetsArgs {
    channel: Channel,
}

#[derive(Serialize)]
struct NetworkArgs {
    channel: Channel,
}

#[derive(Deserialize)]
struct DeviceName {
    name: Option<String>,
}

#[derive(Deserialize)]
struct AllFilesAccess {
    granted: bool,
}

#[derive(Deserialize)]
struct SettingsOpened {
    opened: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexArgs {
    after_id: i64,
    limit: u32,
}

#[derive(Serialize)]
struct ReadArgs<'a> {
    ids: &'a [i64],
}

#[derive(Serialize)]
struct ApplyArgs<'a> {
    ops: &'a [Op],
}

#[derive(Serialize)]
struct PhotoArgs {
    id: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Photo {
    jpeg_base64: Option<String>,
}

#[derive(Deserialize)]
struct Rows<T> {
    rows: Vec<T>,
}

#[derive(Serialize)]
struct PermissionArgs {
    permissions: [&'static str; 1],
}

impl<R: Runtime> HolziAndroid<R> {
    /// The name a document provider shows for a `content://` address; `None` if it names none.
    pub fn display_name(&self, uri: &str) -> Option<String> {
        self.0
            .run_mobile_plugin::<DisplayName>("displayName", UriArgs { uri })
            .ok()?
            .name
    }

    /// Calls `on_change` with the insets now and on every change (FR-011, FR-012).
    pub fn watch_insets(
        &self,
        on_change: impl Fn(Insets) + Send + Sync + 'static,
    ) -> Result<(), String> {
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Json(json) = body {
                match serde_json::from_str::<Insets>(&json) {
                    Ok(insets) => on_change(insets),
                    Err(error) => log::warn!("insets: {error}"),
                }
            }
            Ok(())
        });
        self.0
            .run_mobile_plugin::<()>("watchInsets", InsetsArgs { channel })
            .map_err(|error| error.to_string())
    }

    /// Calls `on_change` whenever the device moves to another network (FR-019).
    pub fn watch_network(
        &self,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        let channel = Channel::new(move |_| {
            on_change();
            Ok(())
        });
        self.0
            .run_mobile_plugin::<()>("watchNetwork", NetworkArgs { channel })
            .map_err(|error| error.to_string())
    }

    /// The name the person gave the phone, else its model (research R10).
    pub fn device_name(&self) -> Option<String> {
        self.0
            .run_mobile_plugin::<DeviceName>("deviceName", ())
            .ok()?
            .name
    }

    /// The state of `permission` without asking.
    pub fn check_permission(&self, permission: Permission) -> Result<PermissionState, String> {
        let states = self
            .0
            .run_mobile_plugin::<HashMap<String, PermissionState>>("checkPermissions", ())
            .map_err(|error| error.to_string())?;
        state_of(states, permission)
    }

    /// Asks the person for `permission` unless it was decided before; blocks until they answer.
    pub fn request_permission(&self, permission: Permission) -> Result<PermissionState, String> {
        let states = self
            .0
            .run_mobile_plugin::<HashMap<String, PermissionState>>(
                "requestPermissions",
                PermissionArgs {
                    permissions: [permission.alias()],
                },
            )
            .map_err(|error| error.to_string())?;
        state_of(states, permission)
    }

    /// Whether holzi may read and write the shared storage (spec 044 FR-039).
    pub fn all_files_access(&self) -> Result<bool, String> {
        self.0
            .run_mobile_plugin::<AllFilesAccess>("allFilesAccess", ())
            .map(|access| access.granted)
            .map_err(|error| error.to_string())
    }

    /// Leads the person to "Zugriff auf alle Dateien" in the system's settings (API 30 on), or asks
    /// for the storage permission before API 30 and blocks until they answer. From the settings
    /// they come back by themselves; the window checks again then.
    pub fn request_all_files_access(&self) -> Result<(), String> {
        let opened = self
            .0
            .run_mobile_plugin::<SettingsOpened>("requestAllFilesAccess", ())
            .map_err(|error| error.to_string())?
            .opened;
        if !opened {
            self.request_permission(Permission::Storage)?;
        }
        Ok(())
    }

    /// The accounts of the phone's address book (spec 051, contracts/android-plugin.md).
    pub fn contacts_accounts(&self) -> Result<Vec<PhoneAccount>, String> {
        self.0
            .run_mobile_plugin::<Rows<PhoneAccount>>("contactsAccounts", ())
            .map(|rows| rows.rows)
            .map_err(|error| error.to_string())
    }

    /// The next page of the raw contacts with an id above `after_id`, deleted ones included.
    pub fn contacts_index(&self, after_id: i64, limit: u32) -> Result<Vec<RawIndex>, String> {
        self.0
            .run_mobile_plugin::<Rows<RawIndex>>("contactsIndex", IndexArgs { after_id, limit })
            .map(|rows| rows.rows)
            .map_err(|error| error.to_string())
    }

    /// The data rows of these raw contacts (at most 200).
    pub fn contacts_read(&self, ids: &[i64]) -> Result<Vec<RawData>, String> {
        self.0
            .run_mobile_plugin::<Rows<RawData>>("contactsRead", ReadArgs { ids })
            .map(|rows| rows.rows)
            .map_err(|error| error.to_string())
    }

    /// Applies the ops as one batch (at most 400).
    pub fn contacts_apply(&self, ops: &[Op]) -> Result<ApplyResult, String> {
        self.0
            .run_mobile_plugin::<ApplyResult>("contactsApply", ApplyArgs { ops })
            .map_err(|error| error.to_string())
    }

    /// The large photo of a raw contact as base64 JPEG, else its thumbnail; `None` without one.
    pub fn contacts_photo_read(&self, id: i64) -> Result<Option<String>, String> {
        self.0
            .run_mobile_plugin::<Photo>("contactsPhotoRead", PhotoArgs { id })
            .map(|photo| photo.jpeg_base64)
            .map_err(|error| error.to_string())
    }

    /// Hides the window from screenshots, screen recordings and the recent apps view (FR-011a).
    pub fn set_secure(&self, enabled: bool) -> Result<(), String> {
        self.0
            .run_mobile_plugin::<()>("setSecure", SecureArgs { enabled })
            .map_err(|error| error.to_string())
    }
}

fn state_of(
    mut states: HashMap<String, PermissionState>,
    permission: Permission,
) -> Result<PermissionState, String> {
    states
        .remove(permission.alias())
        .ok_or_else(|| format!("no state for the permission {}", permission.alias()))
}
