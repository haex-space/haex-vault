//! The layout, written down so a stage changes it on purpose (spec 084, T002, T010).

use std::path::Path;

use super::VaultPaths;
use crate::instances::paths::validate_instance_name;

const ROOT: &str = "/data/haex-vault";

/// Names that a device-wide folder also carries.
const NAMES: [&str; 5] = ["anna", "Test-01", "filters", "models", "agent-browser"];

#[test]
fn a_vault_has_a_folder_with_its_database_in_it() {
    let paths = VaultPaths::new(ROOT);
    for name in NAMES {
        let folder = Path::new(ROOT).join("vaults").join(name);
        assert_eq!(paths.folder(name), folder);
        assert_eq!(paths.database(name), folder.join("vault.db"));
        assert_eq!(
            paths.staging(name),
            Path::new(ROOT)
                .join("vaults")
                .join(format!(".moving-{name}"))
        );
    }
    assert_eq!(paths.vaults_dir(), Path::new(ROOT).join("vaults"));
    assert_eq!(paths.device_lock(), Path::new(ROOT).join("vaults.lock"));
}

#[test]
fn no_vault_name_can_address_a_hidden_folder_or_the_database_name() {
    // The staging folders start with a dot and no vault name does.
    for name in NAMES {
        assert!(!name.starts_with('.'), "{name}");
    }
    assert!(validate_instance_name(".moving-anna").is_err());
}

#[test]
fn the_old_layout_is_named_for_the_move() {
    let paths = VaultPaths::new(ROOT);
    for name in NAMES {
        assert_eq!(
            paths.legacy_database(name),
            Path::new(ROOT).join("instances").join(format!("{name}.db"))
        );
    }
    assert_eq!(paths.legacy_dir(), Path::new(ROOT).join("instances"));
}

#[test]
fn the_profile_and_the_chat_files_lie_in_subfolders_of_the_vault_folder() {
    let paths = VaultPaths::new(ROOT);
    for name in NAMES {
        let folder = Path::new(ROOT).join("vaults").join(name);
        assert_eq!(paths.agent_browser(name), folder.join("agent-browser"));
        assert_eq!(paths.chat_files(name), folder.join("chat-files"));
        assert_eq!(paths.subfolder(name, "other"), folder.join("other"));
        // Nothing of a vault lies in the folder that holds the device-wide agent browser files.
        assert!(!paths
            .agent_browser(name)
            .starts_with(paths.agent_browser_root()));
    }
    assert_eq!(
        paths.agent_browser_root(),
        Path::new(ROOT).join("agent-browser")
    );
}

#[test]
fn the_old_places_of_the_profile_and_the_chat_files_are_named_for_the_move() {
    let paths = VaultPaths::new(ROOT);
    for name in NAMES {
        assert_eq!(
            paths.legacy_agent_browser(name),
            Path::new(ROOT).join("agent-browser").join(name)
        );
        assert_eq!(
            paths.legacy_chat_files(name),
            Path::new(ROOT).join("chat-files").join(name)
        );
    }
    assert_eq!(
        paths.legacy_chat_files_root(),
        Path::new(ROOT).join("chat-files")
    );
}

#[test]
fn every_name_of_the_table_is_a_valid_vault_name() {
    // The collision that spec 084 removes only exists because these are valid names.
    for name in NAMES {
        assert!(validate_instance_name(name).is_ok(), "{name}");
    }
}

#[test]
fn the_root_is_kept_as_given() {
    assert_eq!(VaultPaths::new(ROOT).root(), Path::new(ROOT));
}

#[test]
fn the_cli_tools_and_their_home_lie_in_subfolders_of_the_vault_folder() {
    let paths = VaultPaths::new(ROOT);
    for name in NAMES {
        let folder = Path::new(ROOT).join("vaults").join(name);
        assert_eq!(paths.cli_tools(name), folder.join("cli-tools"));
        assert_eq!(
            paths.cli_tool_dir(name, "claude"),
            folder.join("cli-tools").join("claude")
        );
        assert_eq!(paths.home(name), folder.join("home"));
        assert_eq!(paths.run_root(name), folder.join("home").join("run"));
        assert_eq!(paths.records_dir(name), folder.join("home").join("records"));
        // The home is a subfolder of its own, never the vault folder itself (spec 094 FR-001).
        assert_ne!(paths.home(name), paths.folder(name));
    }
}

#[test]
fn the_log_lies_in_a_subfolder_of_the_vault_folder() {
    let paths = VaultPaths::new(ROOT);
    for name in NAMES {
        let folder = Path::new(ROOT).join("vaults").join(name);
        assert_eq!(paths.logs_dir(name), folder.join("logs"));
        // Never the folder of the vault or the folder of all vaults (spec 098 FR-010).
        assert_ne!(paths.logs_dir(name), paths.folder(name));
        assert!(paths.logs_dir(name).starts_with(paths.folder(name)));
    }
}
