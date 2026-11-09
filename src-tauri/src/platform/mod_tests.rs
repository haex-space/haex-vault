#[cfg(target_os = "linux")]
use super::{capabilities, os_name};
use super::{desktop, ModelPresets, PlatformCapabilities, PlatformName, ANDROID, IOS};

#[test]
fn desktop_offers_every_desktop_facility() {
    let table = desktop(PlatformName::Linux, true);
    assert!(table.cli_delegates);
    assert!(table.command_tool);
    assert!(table.terminal);
    assert!(table.folder_watch);
    assert!(table.free_paths);
    assert!(table.folder_pick);
    assert!(table.gpu_detection);
    assert!(table.relaunch_on_close);
    assert!(!table.screen_capture);
    assert!(!table.back_gesture);
    assert!(!table.phone_contacts);
    assert!(table.browser_bridge);
    assert_eq!(table.model_presets, ModelPresets::Desktop);
}

#[test]
fn only_a_desktop_release_build_relaunches_after_closing() {
    assert!(desktop(PlatformName::Windows, true).relaunch_on_close);
    assert!(!desktop(PlatformName::Windows, false).relaunch_on_close);
}

#[test]
fn android_has_none_of_the_desktop_facilities() {
    assert_eq!(
        ANDROID,
        PlatformCapabilities {
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
            mcp_http: true,
            agent_browser: false,
        }
    );
}

#[test]
fn mobile_systems_offer_no_browser_bridge() {
    // Spec 049 FR-043: holzi does not run in the background there (043 FR-007).
    assert_eq!(
        [ANDROID, IOS].map(|table| table.browser_bridge),
        [false, false]
    );
}

#[test]
fn only_android_has_a_phone_address_book() {
    let tables = [ANDROID, IOS];
    assert_eq!(tables.map(|table| table.phone_contacts), [true, false]);
}

#[cfg(target_os = "linux")]
#[test]
fn this_host_reads_the_linux_desktop_table() {
    assert_eq!(
        capabilities(),
        desktop(PlatformName::Linux, !cfg!(debug_assertions))
    );
    assert_eq!(os_name(), Some("linux"));
}

#[test]
fn the_table_reaches_the_interface_in_camel_case() {
    let json = serde_json::to_value(ANDROID).unwrap();
    assert_eq!(json["platform"], "android");
    assert_eq!(json["cliDelegates"], false);
    assert_eq!(json["screenCapture"], true);
    assert_eq!(json["relaunchOnClose"], false);
    assert_eq!(json["phoneContacts"], true);
}

#[test]
fn only_desktop_systems_have_an_agent_browser() {
    // Spec 065 research R13.
    let desktops = [
        PlatformName::Linux,
        PlatformName::Macos,
        PlatformName::Windows,
    ];
    assert_eq!(
        desktops.map(|platform| desktop(platform, true).agent_browser),
        [true, true, true]
    );
    assert_eq!(
        [ANDROID, IOS].map(|table| table.agent_browser),
        [false, false]
    );
}

#[test]
fn the_http_way_runs_on_desktop_too() {
    assert!(desktop(PlatformName::Linux, true).mcp_http);
}
