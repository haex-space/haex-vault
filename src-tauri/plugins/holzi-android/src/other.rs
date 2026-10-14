//! Every platform but Android: nothing to do.

use std::marker::PhantomData;

use tauri::plugin::PluginApi;
use tauri::{AppHandle, Runtime};

use crate::contacts::{ApplyResult, Op, PhoneAccount, RawData, RawIndex, UNAVAILABLE};

pub fn init<R: Runtime>(_app: &AppHandle<R>, _api: PluginApi<R, ()>) -> HolziAndroid<R> {
    HolziAndroid(PhantomData)
}

/// Stand-in with the same methods as the Android side; each does nothing.
pub struct HolziAndroid<R: Runtime>(PhantomData<fn() -> R>);

impl<R: Runtime> HolziAndroid<R> {
    /// Addresses of document providers exist only on Android.
    pub fn display_name(&self, _uri: &str) -> Option<String> {
        None
    }

    /// The system takes no space at the window's edges here.
    pub fn watch_insets(
        &self,
        _on_change: impl Fn(crate::Insets) + Send + Sync + 'static,
    ) -> Result<(), String> {
        Ok(())
    }

    /// The network changes the sync notices by itself here.
    pub fn watch_network(
        &self,
        _on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        Ok(())
    }

    /// The host name is the device's name here.
    pub fn device_name(&self) -> Option<String> {
        None
    }

    /// Desktop systems ask for no permission of their own here; the system decides on its own.
    pub fn check_permission(
        &self,
        _permission: crate::Permission,
    ) -> Result<crate::PermissionState, String> {
        Ok(crate::PermissionState::Granted)
    }

    /// As [`HolziAndroid::check_permission`]: nothing to ask here.
    pub fn request_permission(
        &self,
        _permission: crate::Permission,
    ) -> Result<crate::PermissionState, String> {
        Ok(crate::PermissionState::Granted)
    }

    /// Every file the system lets the user reach is reachable here.
    pub fn all_files_access(&self) -> Result<bool, String> {
        Ok(true)
    }

    /// As [`HolziAndroid::all_files_access`]: nothing to ask here.
    pub fn request_all_files_access(&self) -> Result<(), String> {
        Ok(())
    }

    /// The phone's address book exists only on Android (spec 051).
    pub fn contacts_accounts(&self) -> Result<Vec<PhoneAccount>, String> {
        Err(UNAVAILABLE.to_owned())
    }

    /// As [`HolziAndroid::contacts_accounts`].
    pub fn contacts_index(&self, _after_id: i64, _limit: u32) -> Result<Vec<RawIndex>, String> {
        Err(UNAVAILABLE.to_owned())
    }

    /// As [`HolziAndroid::contacts_accounts`].
    pub fn contacts_read(&self, _ids: &[i64]) -> Result<Vec<RawData>, String> {
        Err(UNAVAILABLE.to_owned())
    }

    /// As [`HolziAndroid::contacts_accounts`].
    pub fn contacts_apply(&self, _ops: &[Op]) -> Result<ApplyResult, String> {
        Err(UNAVAILABLE.to_owned())
    }

    /// As [`HolziAndroid::contacts_accounts`].
    pub fn contacts_photo_read(&self, _id: i64) -> Result<Option<String>, String> {
        Err(UNAVAILABLE.to_owned())
    }

    /// Screen capture protection exists only on Android.
    pub fn set_secure(&self, _enabled: bool) -> Result<(), String> {
        Ok(())
    }
}
