//! The layout of today, written down so the later stages change it on purpose (spec 084, T002).

use std::path::Path;

use super::VaultPaths;
use crate::instances::paths::validate_instance_name;

const ROOT: &str = "/data/holzi";

/// Names that a device-wide folder also carries.
const NAMES: [&str; 5] = ["anna", "Test-01", "filters", "models", "agent-browser"];

#[test]
fn a_database_lies_in_the_instances_folder() {
    let paths = VaultPaths::new(ROOT);
    for name in NAMES {
        assert_eq!(
            paths.database(name),
            Path::new(ROOT).join("instances").join(format!("{name}.db"))
        );
    }
    assert_eq!(paths.instances_dir(), Path::new(ROOT).join("instances"));
}

#[test]
fn the_profile_and_the_chat_files_lie_in_a_folder_named_for_the_vault() {
    let paths = VaultPaths::new(ROOT);
    for name in NAMES {
        assert_eq!(
            paths.agent_browser(name),
            Path::new(ROOT).join("agent-browser").join(name)
        );
        assert_eq!(
            paths.chat_files(name),
            Path::new(ROOT).join("chat-files").join(name)
        );
    }
    assert_eq!(
        paths.agent_browser_root(),
        Path::new(ROOT).join("agent-browser")
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
