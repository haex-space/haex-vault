//! The base containment: what the child's environment gets and what it keeps (spec 094, T009,
//! FR-011, `data-model.md` "Process environment of the base layer").

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::{
    apply, home_variables, temp_variables, Containment, EnvContainment, IsolationLevel, UNTOUCHED,
};
use crate::cli_tools::catalog::Catalog;
use crate::cli_tools::runtime::DelegateRuntime;

fn host(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(key, value)| (OsString::from(key), OsString::from(value)))
        .collect()
}

/// What `command` changes: `Some(value)` for a variable it sets, `None` for one it removes.
fn changes(command: &Command) -> HashMap<OsString, Option<OsString>> {
    command
        .get_envs()
        .map(|(key, value)| (key.to_os_string(), value.map(OsStr::to_os_string)))
        .collect()
}

fn set_to<'a>(changes: &'a HashMap<OsString, Option<OsString>>, name: &str) -> Option<&'a Path> {
    changes
        .get(OsStr::new(name))
        .and_then(Option::as_ref)
        .map(|value| Path::new(value.as_os_str()))
}

struct Setup {
    _dir: TempDir,
    runtime: DelegateRuntime,
}

fn setup(vendor: &str) -> Setup {
    let dir = TempDir::new().unwrap();
    let runtime = DelegateRuntime::for_stub(vendor, dir.path().join("stub"), dir.path());
    Setup { _dir: dir, runtime }
}

#[test]
fn the_base_layer_reports_best_effort() {
    let setup = setup("claude");
    let run = setup.runtime.run_folder().unwrap();
    let mut command = Command::new("true");
    let level = EnvContainment.contain(&mut command, &run, &setup.runtime.vendor);
    assert_eq!(level, IsolationLevel::BestEffort);
    assert_eq!(EnvContainment.level(), IsolationLevel::BestEffort);
    assert_eq!(setup.runtime.isolation(), IsolationLevel::BestEffort);
}

#[test]
fn the_home_the_temp_folders_and_the_config_variable_point_into_the_vault() {
    let setup = setup("claude");
    let run = setup.runtime.run_folder().unwrap();
    let mut command = Command::new("true");
    apply(&mut command, &run, &setup.runtime.vendor, &host(&[]));
    let changed = changes(&command);

    if cfg!(windows) {
        for name in ["USERPROFILE", "APPDATA", "LOCALAPPDATA"] {
            assert_eq!(set_to(&changed, name), Some(run.home()), "{name}");
        }
        for name in ["TEMP", "TMP"] {
            assert_eq!(set_to(&changed, name), Some(run.tmp()), "{name}");
        }
    } else {
        assert_eq!(set_to(&changed, "HOME"), Some(run.home()));
        assert_eq!(set_to(&changed, "TMPDIR"), Some(run.tmp()));
    }
    assert_eq!(set_to(&changed, "CLAUDE_CONFIG_DIR"), Some(run.path()));
    assert_eq!(set_to(&changed, "CLAUDE_CODE_TMPDIR"), Some(run.tmp()));
    assert_eq!(
        changed.get(OsStr::new("DISABLE_UPDATES")),
        Some(&Some(OsString::from("1")))
    );
    // The home lies in the vault's folder tree, and so does everything else that was set.
    for value in changed.values().flatten() {
        let value = Path::new(value);
        if value.is_absolute() {
            assert!(value.starts_with(setup._dir.path()), "{value:?}");
        }
    }
}

#[test]
fn codex_gets_its_own_config_variable_and_no_claude_variable() {
    let setup = setup("codex");
    let run = setup.runtime.run_folder().unwrap();
    let mut command = Command::new("true");
    apply(&mut command, &run, &setup.runtime.vendor, &host(&[]));
    let changed = changes(&command);
    assert_eq!(set_to(&changed, "CODEX_HOME"), Some(run.path()));
    assert!(!changed.contains_key(OsStr::new("CLAUDE_CONFIG_DIR")));
    assert!(!changed.contains_key(OsStr::new("DISABLE_UPDATES")));
}

#[test]
fn only_the_variables_that_pick_the_account_or_endpoint_are_removed() {
    let setup = setup("claude");
    let run = setup.runtime.run_folder().unwrap();
    let mut command = Command::new("true");
    let environment = host(&[
        ("ANTHROPIC_API_KEY", "sk-host"),
        ("ANTHROPIC_BASE_URL", "https://host.example"),
        ("CLAUDE_CODE_USE_BEDROCK", "1"),
        ("OPENAI_API_KEY", "not this vendor's"),
        ("CLAUDE_OTHER", "kept"),
        ("MY_ANTHROPIC_NOTE", "kept"),
    ]);
    apply(&mut command, &run, &setup.runtime.vendor, &environment);
    let changed = changes(&command);
    for removed in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
        "CLAUDE_CODE_USE_BEDROCK",
    ] {
        assert_eq!(changed.get(OsStr::new(removed)), Some(&None), "{removed}");
    }
    // Another vendor's variables and unrelated names are left to the child as they are.
    for kept in ["OPENAI_API_KEY", "CLAUDE_OTHER", "MY_ANTHROPIC_NOTE"] {
        assert!(!changed.contains_key(OsStr::new(kept)), "{kept}");
    }

    let codex = setup_for(&setup, "codex");
    let run = codex.run_folder().unwrap();
    let mut command = Command::new("true");
    apply(&mut command, &run, &codex.vendor, &environment);
    let changed = changes(&command);
    assert_eq!(changed.get(OsStr::new("OPENAI_API_KEY")), Some(&None));
    assert!(!changed.contains_key(OsStr::new("ANTHROPIC_API_KEY")));
}

fn setup_for(existing: &Setup, vendor: &str) -> DelegateRuntime {
    DelegateRuntime::for_stub(
        vendor,
        existing._dir.path().join("stub"),
        existing._dir.path(),
    )
}

#[test]
fn what_a_tool_needs_from_the_host_is_never_touched() {
    let setup = setup("claude");
    let run = setup.runtime.run_folder().unwrap();
    let mut command = Command::new("true");
    let environment: Vec<(OsString, OsString)> = UNTOUCHED
        .iter()
        .map(|name| (OsString::from(name), OsString::from("host value")))
        .collect();
    apply(&mut command, &run, &setup.runtime.vendor, &environment);
    let changed = changes(&command);
    for name in UNTOUCHED {
        assert!(
            !changed.contains_key(OsStr::new(name)),
            "{name} must reach the tool as the host has it"
        );
    }
    // Neither vendor's removal prefixes match any of them.
    for vendor in &Catalog::shipped().vendors {
        for prefix in &vendor.env_remove_prefixes {
            assert!(
                !UNTOUCHED
                    .iter()
                    .any(|name| name.starts_with(prefix.as_str())),
                "{prefix} would remove a pass-through variable"
            );
        }
    }
}

#[test]
fn an_xdg_variable_is_redirected_only_where_the_host_defines_it() {
    let setup = setup("claude");
    let run = setup.runtime.run_folder().unwrap();

    let mut command = Command::new("true");
    apply(&mut command, &run, &setup.runtime.vendor, &host(&[]));
    let changed = changes(&command);
    assert!(
        !changed
            .keys()
            .any(|key| key.to_string_lossy().starts_with("XDG_")),
        "an unset XDG variable stays unset: {changed:?}"
    );

    let mut command = Command::new("true");
    let environment = host(&[
        ("XDG_CONFIG_HOME", "/host/config"),
        ("XDG_RUNTIME_DIR", "/run/user/1000"),
    ]);
    apply(&mut command, &run, &setup.runtime.vendor, &environment);
    let changed = changes(&command);
    let config = set_to(&changed, "XDG_CONFIG_HOME").unwrap();
    assert_eq!(config, run.home().join(".config"));
    assert_eq!(set_to(&changed, "XDG_RUNTIME_DIR"), Some(run.runtime()));
    for name in ["XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"] {
        assert!(!changed.contains_key(OsStr::new(name)), "{name}");
    }
}

#[test]
fn the_variable_names_follow_the_platform() {
    let home = Path::new("/vault/home");
    let unix: Vec<_> = home_variables(false, home)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(unix, ["HOME"]);
    let windows: Vec<_> = home_variables(true, home)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(windows, ["USERPROFILE", "APPDATA", "LOCALAPPDATA"]);
    assert_eq!(temp_variables(false), ["TMPDIR"]);
    assert_eq!(temp_variables(true), ["TEMP", "TMP"]);
}

#[test]
fn the_containment_changes_the_child_alone_and_never_the_process_environment() {
    let before: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let setup = setup("codex");
    let run = setup.runtime.run_folder().unwrap();
    let mut command = Command::new("true");
    EnvContainment.contain(&mut command, &run, &setup.runtime.vendor);
    setup.runtime.contain(&mut Command::new("true"), &run);
    let after: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    // Other tests may run in parallel, so compare only what this containment could have written.
    for name in ["HOME", "TMPDIR", "CODEX_HOME", "CLAUDE_CONFIG_DIR"] {
        assert_eq!(
            before.iter().find(|(key, _)| key == name),
            after.iter().find(|(key, _)| key == name),
            "{name} of the test process changed"
        );
    }
}
