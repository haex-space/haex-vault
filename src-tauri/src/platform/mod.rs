//! What this device can do (spec 043 FR-016, FR-026; contract `platform-capabilities.md`).
//!
//! The one place that decides whether a facility exists on this platform. Every reader (the
//! extension shell and files, the delegates, the chat tools, the hardware detection, the close
//! path, the interface through [`commands::platform_capabilities`]) asks this table instead of
//! keeping its own `cfg!` check. `cfg` stays only where code does not compile on a platform.

pub mod commands;
mod end;
pub mod insets;

pub use end::end_process;

use serde::Serialize;
use ts_rs::TS;

/// The platform holzi runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum PlatformName {
    Linux,
    Macos,
    Windows,
    Android,
    Ios,
}

/// Which presets of the model catalogs this device gets (spec 043 FR-027, FR-030, research R11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum ModelPresets {
    /// Every catalog model; the suggestion is the largest that fits.
    Desktop,
    /// The phone profiles of the model catalog and the smallest speech recognition model.
    Phone,
}

/// The facilities of this device; fixed for the life of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct PlatformCapabilities {
    pub platform: PlatformName,
    /// The CLI delegates (claude, codex): external programs with a terminal.
    pub cli_delegates: bool,
    /// The chat tool `run_command`.
    pub command_tool: bool,
    /// The terminal of extensions.
    pub terminal: bool,
    /// Watching folders for extensions.
    pub folder_watch: bool,
    /// File paths outside a dialog choice for extensions.
    pub free_paths: bool,
    /// Choosing a folder in a dialog.
    pub folder_pick: bool,
    /// Detecting a graphics card for model recommendations.
    pub gpu_detection: bool,
    /// Closing the vault restarts the app at the vault picker (spec 013); otherwise it ends.
    pub relaunch_on_close: bool,
    /// The screen capture protection setting exists (FR-011a).
    pub screen_capture: bool,
    /// The system has a back gesture (spec 020 FR-019).
    pub back_gesture: bool,
    /// The presets of the model catalogs the suggestions come from.
    pub model_presets: ModelPresets,
    /// A phone address book the address book can sync with (spec 051 FR-015).
    pub phone_contacts: bool,
    /// The External Bridge for browser extensions (spec 049 FR-043).
    pub browser_bridge: bool,
    /// External agents reach holzi through the `--mcp-agent` helper (spec 021 research R1, R12).
    /// Phones need the HTTP way of spec 021 PR G instead.
    pub mcp_helper: bool,
    /// A browser holzi starts and steers for agents (spec 065 research R13); a phone has no browser
    /// that holzi can steer over a pipe.
    pub agent_browser: bool,
}

/// A desktop system; only a release build restarts at the vault picker after closing.
pub const fn desktop(platform: PlatformName, release: bool) -> PlatformCapabilities {
    PlatformCapabilities {
        platform,
        cli_delegates: true,
        command_tool: true,
        terminal: true,
        folder_watch: true,
        free_paths: true,
        folder_pick: true,
        gpu_detection: true,
        relaunch_on_close: release,
        screen_capture: false,
        back_gesture: false,
        model_presets: ModelPresets::Desktop,
        phone_contacts: false,
        browser_bridge: true,
        mcp_helper: true,
        agent_browser: true,
    }
}

/// Android: none of the desktop facilities; closing the vault ends the app (FR-006).
pub const ANDROID: PlatformCapabilities = PlatformCapabilities {
    platform: PlatformName::Android,
    cli_delegates: false,
    command_tool: false,
    terminal: false,
    folder_watch: false,
    free_paths: false,
    folder_pick: false,
    gpu_detection: false,
    relaunch_on_close: false,
    screen_capture: true,
    back_gesture: true,
    model_presets: ModelPresets::Phone,
    phone_contacts: true,
    browser_bridge: false,
    mcp_helper: false,
    agent_browser: false,
};

/// iOS is not a target yet (spec 043 is Android only); it gets the mobile limits without the
/// Android-only settings.
pub const IOS: PlatformCapabilities = PlatformCapabilities {
    platform: PlatformName::Ios,
    screen_capture: false,
    back_gesture: false,
    phone_contacts: false,
    ..ANDROID
};

/// The table of the platform this build runs on.
pub const fn capabilities() -> PlatformCapabilities {
    #[cfg(target_os = "android")]
    return ANDROID;
    #[cfg(target_os = "ios")]
    return IOS;
    #[cfg(target_os = "windows")]
    return desktop(PlatformName::Windows, !cfg!(debug_assertions));
    #[cfg(target_os = "macos")]
    return desktop(PlatformName::Macos, !cfg!(debug_assertions));
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        target_os = "windows",
        target_os = "macos"
    )))]
    return desktop(PlatformName::Linux, !cfg!(debug_assertions));
}

/// The OS name extensions see in their `ApplicationContext.platform` (the SDK's names).
pub fn os_name() -> Option<&'static str> {
    match std::env::consts::OS {
        os @ ("linux" | "macos" | "ios" | "freebsd" | "dragonfly" | "netbsd" | "openbsd"
        | "solaris" | "android" | "windows") => Some(os),
        _ => None,
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod mod_tests;
