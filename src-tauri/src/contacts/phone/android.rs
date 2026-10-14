//! The phone's address book through the Android plugin (spec 051, contracts/android-plugin.md).
//! Compiles everywhere: off Android the plugin answers every call with "unavailable", which
//! becomes [`HolziError::PhoneContactsUnavailable`].

use base64::Engine;
use tauri::{AppHandle, Runtime};
use tauri_plugin_holzi_android::contacts::{
    ApplyResult, Op, PhoneAccount, RawData, RawIndex, UNAVAILABLE,
};
use tauri_plugin_holzi_android::{HolziAndroidExt, Permission, PermissionState};

use super::PhoneBook;
use crate::error::{HolziError, Result};

/// The address book of this phone.
pub struct AndroidPhoneBook<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> AndroidPhoneBook<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

fn failed(reason: String) -> HolziError {
    if reason == UNAVAILABLE {
        HolziError::PhoneContactsUnavailable
    } else {
        HolziError::PhoneContactsFailed { reason }
    }
}

impl<R: Runtime> PhoneBook for AndroidPhoneBook<R> {
    fn permitted(&self) -> Result<bool> {
        let state = self
            .app
            .holzi_android()
            .check_permission(Permission::Contacts)
            .map_err(failed)?;
        Ok(state == PermissionState::Granted)
    }

    fn accounts(&self) -> Result<Vec<PhoneAccount>> {
        self.app.holzi_android().contacts_accounts().map_err(failed)
    }

    fn index(&self, after_id: i64, limit: u32) -> Result<Vec<RawIndex>> {
        self.app
            .holzi_android()
            .contacts_index(after_id, limit)
            .map_err(failed)
    }

    fn read(&self, ids: &[i64]) -> Result<Vec<RawData>> {
        self.app.holzi_android().contacts_read(ids).map_err(failed)
    }

    fn apply(&self, ops: &[Op]) -> Result<ApplyResult> {
        self.app.holzi_android().contacts_apply(ops).map_err(failed)
    }

    fn photo(&self, id: i64) -> Result<Option<Vec<u8>>> {
        let encoded = self
            .app
            .holzi_android()
            .contacts_photo_read(id)
            .map_err(failed)?;
        Ok(encoded.and_then(|text| base64::engine::general_purpose::STANDARD.decode(text).ok()))
    }
}

/// Asks the person for the permission unless it was decided before; anything but "granted" is
/// [`HolziError::ContactsDenied`] (FR-017). Blocks until they answer.
pub async fn ensure_permission<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let app = app.clone();
    let state = tauri::async_runtime::spawn_blocking(move || {
        app.holzi_android().request_permission(Permission::Contacts)
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|state| state)
    .map_err(failed)?;
    match state {
        PermissionState::Granted => Ok(()),
        _ => Err(HolziError::ContactsDenied),
    }
}
