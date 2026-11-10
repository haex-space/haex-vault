use std::collections::BTreeMap;

use super::*;

/// The files a vault of the old layout can have, each with content of its own.
const SIDECARS: [&str; 5] = [".lock", "-wal", "-shm", ".vault-id", ".pending"];

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// An old-layout vault: the database and its sidecars (the marker only when asked for).
fn legacy_vault(paths: &VaultPaths, name: &str, pending: bool) {
    let db = paths.legacy_database(name);
    put(&db, &format!("{name} database"));
    for suffix in SIDECARS {
        if suffix == ".pending" && !pending {
            continue;
        }
        let mut file = db.clone().into_os_string();
        file.push(suffix);
        put(Path::new(&file), &format!("{name}{suffix}"));
    }
}

/// Three vaults, one named like the folder of the tracker lists, with profile and chat files at
/// their old places, and the device-wide files of the agent browser that must not move.
fn old_data_folder() -> (tempfile::TempDir, VaultPaths) {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());
    legacy_vault(&paths, "anna", false);
    legacy_vault(&paths, "ben-2", false);
    legacy_vault(&paths, "filters", false);
    put(
        &paths.legacy_agent_browser("anna").join("Profil/p.zip"),
        "anna profile",
    );
    put(&paths.legacy_chat_files("anna").join("Chat/f"), "anna chat");
    put(&paths.legacy_chat_files("ben-2").join("Chat/f"), "ben chat");
    // The tracker lists share their folder with the profile of the vault `filters`.
    let lists = paths.legacy_agent_browser("filters");
    put(&lists.join("settings.json"), "settings");
    put(&lists.join("easylist.txt"), "list");
    put(&lists.join("easylist.1234.part"), "download");
    put(&lists.join("Profil/p.zip"), "filters profile");
    put(&paths.agent_browser_root().join("dns.json"), "dns");
    (dir, paths)
}

/// Every file below `root` with its content, by relative path.
fn tree(root: &Path) -> BTreeMap<String, String> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                out.insert(relative, fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn moved(outcome: &Outcome) -> Vec<&str> {
    outcome.moved.iter().map(String::as_str).collect()
}

#[test]
fn every_vault_gets_its_folder_with_all_its_files() {
    let (dir, paths) = old_data_folder();

    let outcome = run(&paths).unwrap();

    assert_eq!(moved(&outcome), ["anna", "ben-2", "filters"]);
    assert!(outcome.left.is_empty());
    assert_eq!(
        outcome.adopted,
        [
            "anna/agent-browser",
            "anna/chat-files",
            "ben-2/chat-files",
            "filters/agent-browser"
        ]
    );
    let files = tree(dir.path());
    for name in ["anna", "ben-2", "filters"] {
        assert_eq!(
            files[&format!("vaults/{name}/vault.db")],
            format!("{name} database")
        );
        for suffix in ["-wal", "-shm", ".vault-id"] {
            assert_eq!(
                files[&format!("vaults/{name}/vault.db{suffix}")],
                format!("{name}{suffix}"),
                "{name}{suffix}"
            );
        }
    }
    assert!(!paths.legacy_dir().exists(), "the old folder is gone");
    assert!(
        !fs::read_dir(paths.vaults_dir())
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with('.')),
        "no staging folder is left"
    );
    // The profile and the chat files follow their vault; the device-wide files stay.
    assert_eq!(
        files["vaults/anna/agent-browser/Profil/p.zip"],
        "anna profile"
    );
    assert_eq!(files["vaults/anna/chat-files/Chat/f"], "anna chat");
    assert_eq!(files["vaults/ben-2/chat-files/Chat/f"], "ben chat");
    assert_eq!(files["agent-browser/dns.json"], "dns");
    // The tracker lists went to their own folder.
    assert_eq!(files["filter-lists/settings.json"], "settings");
    assert_eq!(files["filter-lists/easylist.txt"], "list");
}

#[test]
fn a_vault_with_a_pending_marker_moves_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());
    legacy_vault(&paths, "half", true);
    put(&paths.legacy_dir().join("only-marker.db.pending"), "");

    let outcome = run(&paths).unwrap();

    assert_eq!(moved(&outcome), ["half", "only-marker"]);
    assert!(paths.folder("half").join("vault.db.pending").is_file());
    assert!(paths
        .folder("only-marker")
        .join("vault.db.pending")
        .is_file());
    assert!(!paths.folder("only-marker").join("vault.db").exists());
}

#[test]
fn a_second_run_changes_nothing() {
    let (dir, paths) = old_data_folder();
    run(&paths).unwrap();
    let after_first = tree(dir.path());

    let outcome = run(&paths).unwrap();

    assert_eq!(outcome, Outcome::default());
    assert_eq!(tree(dir.path()), after_first);
}

#[test]
fn a_new_data_folder_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());

    assert_eq!(run(&paths).unwrap(), Outcome::default());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn files_that_are_no_vault_stay_in_the_old_folder() {
    let (_dir, paths) = old_data_folder();
    put(&paths.legacy_dir().join("notes.txt"), "mine");
    put(&paths.legacy_dir().join(".trash/old.db"), "trash");

    run(&paths).unwrap();

    assert_eq!(
        fs::read_to_string(paths.legacy_dir().join("notes.txt")).unwrap(),
        "mine"
    );
    assert!(paths.legacy_dir().join(".trash/old.db").is_file());
    assert!(paths.folder("anna").join("vault.db").is_file());
}

#[test]
fn a_folder_that_exists_is_never_overwritten() {
    let (_dir, paths) = old_data_folder();
    put(&paths.folder("anna").join("vault.db"), "the newer vault");

    let outcome = run(&paths).unwrap();

    assert_eq!(moved(&outcome), ["ben-2", "filters"]);
    assert_eq!(outcome.left.len(), 1);
    assert_eq!(outcome.left[0].0, "anna");
    assert_eq!(
        fs::read_to_string(paths.folder("anna").join("vault.db")).unwrap(),
        "the newer vault"
    );
    assert_eq!(
        fs::read_to_string(paths.legacy_database("anna")).unwrap(),
        "anna database",
        "the old vault stays where it is"
    );
}

#[test]
fn a_file_in_staging_is_never_overwritten() {
    let (_dir, paths) = old_data_folder();
    put(&paths.staging("anna").join("vault.db-wal"), "not ours");

    let outcome = run(&paths).unwrap();

    assert_eq!(outcome.left.len(), 1);
    assert_eq!(
        fs::read_to_string(paths.staging("anna").join("vault.db-wal")).unwrap(),
        "not ours"
    );
    assert!(paths.legacy_database("anna").is_file());
    assert!(!paths.folder("anna").exists(), "never published half way");
    assert_eq!(moved(&outcome), ["ben-2", "filters"]);
}

#[test]
fn a_vault_another_process_holds_open_stays_until_it_is_free() {
    let (_dir, paths) = old_data_folder();
    let held = probe(&paths.legacy_database("anna")).unwrap();
    assert!(matches!(held, Probe::Free(_)));

    let outcome = run(&paths).unwrap();

    assert_eq!(moved(&outcome), ["ben-2", "filters"]);
    assert_eq!(outcome.left.len(), 1);
    assert_eq!(outcome.left[0].0, "anna");
    assert!(paths.legacy_database("anna").is_file());
    assert!(
        !paths.staging("anna").exists(),
        "nothing was started for the held vault"
    );

    drop(held);
    let outcome = run(&paths).unwrap();
    assert_eq!(moved(&outcome), ["anna"]);
    assert!(paths.folder("anna").join("vault.db-wal").is_file());
}

/// The sidecar files of every visible vault folder are complete or the folder is not there.
fn assert_no_vault_is_split(paths: &VaultPaths) {
    let Ok(entries) = fs::read_dir(paths.vaults_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        for file in [
            "vault.db",
            "vault.db-wal",
            "vault.db-shm",
            "vault.db.vault-id",
        ] {
            assert!(
                entry.path().join(file).is_file(),
                "{name} is visible without {file}"
            );
        }
    }
}

/// SC-003: whatever step a run stops at, the next start finishes the job and the result is the
/// one of an uninterrupted run; no vault is ever visible with part of its files.
#[test]
fn a_move_stopped_at_any_step_is_finished_by_the_next_start() {
    let (reference_dir, reference_paths) = old_data_folder();
    run(&reference_paths).unwrap();
    let reference = tree(reference_dir.path());

    for steps in 0..200 {
        let (dir, paths) = old_data_folder();
        let first = run_with(&paths, &mut Budget::steps(steps)).unwrap();
        if !first.stopped {
            // The budget outlasted the move: this is the uninterrupted run.
            assert_eq!(tree(dir.path()), reference, "after {steps} steps");
            assert!(steps > 0, "the move has steps");
            return;
        }
        assert_no_vault_is_split(&paths);

        let second = run(&paths).unwrap();
        assert!(second.left.is_empty(), "after {steps} steps: {second:?}");
        assert_eq!(tree(dir.path()), reference, "stopped after {steps} steps");
        assert!(!paths.legacy_dir().exists(), "after {steps} steps");
    }
    panic!("the move took more than 200 steps");
}

#[test]
fn two_stops_in_a_row_still_end_in_the_same_place() {
    let (reference_dir, reference_paths) = old_data_folder();
    run(&reference_paths).unwrap();
    let reference = tree(reference_dir.path());

    let (dir, paths) = old_data_folder();
    for steps in [1, 4, 2, 9] {
        run_with(&paths, &mut Budget::steps(steps)).unwrap();
        assert_no_vault_is_split(&paths);
    }
    run(&paths).unwrap();
    assert_eq!(tree(dir.path()), reference);
}

#[test]
fn the_tracker_lists_leave_the_folder_of_a_vault_named_filters() {
    let (dir, paths) = old_data_folder();

    let outcome = run(&paths).unwrap();

    let files = tree(dir.path());
    assert_eq!(
        files["vaults/filters/agent-browser/Profil/p.zip"],
        "filters profile"
    );
    for (list, content) in [
        ("settings.json", "settings"),
        ("easylist.txt", "list"),
        ("easylist.1234.part", "download"),
    ] {
        assert_eq!(files[&format!("filter-lists/{list}")], content, "{list}");
        assert!(
            !files.contains_key(&format!("vaults/filters/agent-browser/{list}")),
            "{list} is no profile"
        );
        assert!(outcome.lists.contains(&list.to_string()), "{list}");
    }
    assert!(outcome.lists_left.is_empty());
    assert!(
        !paths.legacy_filter_lists().exists(),
        "nothing is left in the old folder"
    );
}

#[test]
fn lists_move_without_any_vault_and_a_second_run_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());
    put(
        &paths.legacy_filter_lists().join("settings.json"),
        "settings",
    );
    put(&paths.legacy_filter_lists().join("easylist.txt"), "list");
    put(&paths.agent_browser_root().join("dns.json"), "dns");

    let first = run(&paths).unwrap();
    let after_first = tree(dir.path());
    let second = run(&paths).unwrap();

    assert_eq!(first.lists.len(), 2);
    assert_eq!(second, Outcome::default());
    assert_eq!(tree(dir.path()), after_first);
    assert_eq!(after_first["filter-lists/easylist.txt"], "list");
    assert_eq!(after_first["agent-browser/dns.json"], "dns");
}

#[test]
fn a_list_file_that_exists_at_the_new_place_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());
    put(&paths.legacy_filter_lists().join("easylist.txt"), "old");
    put(&paths.legacy_filter_lists().join("other.txt"), "other");
    put(&paths.filter_lists().join("easylist.txt"), "newer");

    let outcome = run(&paths).unwrap();

    assert_eq!(outcome.lists, ["other.txt"]);
    assert_eq!(outcome.lists_left.len(), 1);
    assert_eq!(outcome.lists_left[0].0, "easylist.txt");
    let files = tree(dir.path());
    assert_eq!(files["filter-lists/easylist.txt"], "newer");
    assert_eq!(files["agent-browser/filters/easylist.txt"], "old");
}

#[test]
fn things_in_the_old_list_folder_that_are_no_list_files_stay() {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());
    put(
        &paths.legacy_filter_lists().join("settings.json"),
        "settings",
    );
    put(&paths.legacy_filter_lists().join("Cookies"), "profile");
    put(&paths.legacy_filter_lists().join("Profil/p.zip"), "profile");

    run(&paths).unwrap();

    let files = tree(dir.path());
    assert_eq!(files["filter-lists/settings.json"], "settings");
    assert_eq!(files["agent-browser/filters/Cookies"], "profile");
    assert_eq!(files["agent-browser/filters/Profil/p.zip"], "profile");
}

#[test]
fn an_existing_folder_in_the_vault_folder_is_not_overwritten() {
    let (dir, paths) = old_data_folder();
    put(&paths.chat_files("anna").join("Chat/f"), "newer");

    let outcome = run(&paths).unwrap();

    let files = tree(dir.path());
    assert_eq!(files["vaults/anna/chat-files/Chat/f"], "newer");
    assert_eq!(files["chat-files/anna/Chat/f"], "anna chat");
    assert!(!outcome.adopted.contains(&"anna/chat-files".to_string()));
    assert!(outcome.adopted.contains(&"anna/agent-browser".to_string()));
}

#[test]
fn a_profile_without_a_vault_folder_stays_until_there_is_one() {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());
    put(
        &paths.legacy_agent_browser("gone").join("Profil/p.zip"),
        "x",
    );

    assert_eq!(run(&paths).unwrap(), Outcome::default());
    assert!(paths.legacy_agent_browser("gone").is_dir());
}

#[test]
fn a_vault_another_process_holds_open_keeps_its_folders_where_they_are() {
    let (_dir, paths) = old_data_folder();
    run(&paths).unwrap();
    put(
        &paths.legacy_agent_browser("ben-2").join("Profil/g"),
        "late",
    );
    let held = probe(&paths.database("ben-2")).unwrap();
    assert!(matches!(held, Probe::Free(_)));

    let outcome = run(&paths).unwrap();

    assert!(outcome.adopted.is_empty());
    assert!(paths
        .legacy_agent_browser("ben-2")
        .join("Profil/g")
        .is_file());
    assert_eq!(outcome.adoption_left.len(), 1);
    assert_eq!(outcome.adoption_left[0].0, "ben-2");

    drop(held);
    let outcome = run(&paths).unwrap();
    assert_eq!(outcome.adopted, ["ben-2/agent-browser"]);
    assert!(paths.agent_browser("ben-2").join("Profil/g").is_file());
}
