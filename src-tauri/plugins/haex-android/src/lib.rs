//! haex-vault's Android platform code (spec 043, ADR-0010).
//!
//! Everything only Android has goes through this crate, so its Kotlin, manifest entries and
//! Gradle dependencies stay out of the generated `gen/android` project. On every other
//! platform the crate compiles and does nothing.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "android")]
mod android;
pub mod contacts;
#[cfg(not(target_os = "android"))]
mod other;

#[cfg(target_os = "android")]
pub use android::HaexAndroid;

/// The space the system takes at the edges of the window, in CSS pixels: status bar, navigation
/// bar and display cutout, and the on-screen keyboard apart from the navigation bar below it.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
    pub keyboard: f64,
}
#[cfg(not(target_os = "android"))]
pub use other::HaexAndroid;

pub use tauri::plugin::PermissionState;

/// A runtime permission the app asks the person for (research R12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Recording audio for the speech input (FR-029).
    Microphone,
    /// The shared storage before API 30 (spec 044 FR-039); from API 30 on "Zugriff auf alle Dateien"
    /// is no runtime permission ([`HaexAndroid::request_all_files_access`]).
    Storage,
    /// Reading and writing the phone's address book (spec 051 FR-017).
    Contacts,
}

impl Permission {
    /// The alias the Kotlin plugin declares it under.
    pub fn alias(self) -> &'static str {
        match self {
            Permission::Microphone => "microphone",
            Permission::Storage => "storage",
            Permission::Contacts => "contacts",
        }
    }
}

/// Access to the platform code from any [`Manager`].
pub trait HaexAndroidExt<R: Runtime> {
    fn haex_android(&self) -> &HaexAndroid<R>;
}

impl<R: Runtime, T: Manager<R>> HaexAndroidExt<R> for T {
    fn haex_android(&self) -> &HaexAndroid<R> {
        self.state::<HaexAndroid<R>>().inner()
    }
}

/// The plugin; register it once on the app builder.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("haex-android")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let platform = android::init(app, api)?;
            #[cfg(not(target_os = "android"))]
            let platform = other::init(app, api);
            app.manage(platform);
            Ok(())
        })
        .build()
}
