//! Diff engine — compares two `FileState` manifests and produces `SyncActions`.

use std::collections::HashMap;

use super::types::{DeleteMode, FileState, SyncActions, SyncConflict, SyncDirection};

/// Compare source and target file manifests and compute the actions needed to sync them.
///
/// This is a pure function — no IO, no async, just data comparison.
pub fn compute_sync_actions(
    source_files: &[FileState],
    target_files: &[FileState],
    direction: SyncDirection,
    delete_mode: DeleteMode,
) -> SyncActions {
    let source_map: HashMap<&str, &FileState> = source_files
        .iter()
        .map(|f| (f.relative_path.as_str(), f))
        .collect();

    let target_map: HashMap<&str, &FileState> = target_files
        .iter()
        .map(|f| (f.relative_path.as_str(), f))
        .collect();

    let mut actions = SyncActions {
        to_download: Vec::new(),
        to_upload: Vec::new(),
        to_delete: Vec::new(),
        to_create_directories: Vec::new(),
        conflicts: Vec::new(),
    };

    match direction {
        SyncDirection::OneWay => {
            compute_one_way(&source_map, &target_map, delete_mode, &mut actions);
        }
        SyncDirection::TwoWay => {
            compute_two_way(&source_map, &target_map, &mut actions);
        }
    }

    // Sort directories by depth (parents first)
    actions
        .to_create_directories
        .sort_by_key(|path| path.matches('/').count());

    actions
}

fn compute_one_way(
    source_map: &HashMap<&str, &FileState>,
    target_map: &HashMap<&str, &FileState>,
    delete_mode: DeleteMode,
    actions: &mut SyncActions,
) {
    // Check source entries against target
    for (&path, &source) in source_map {
        if source.is_directory {
            if !target_map.contains_key(path) {
                actions.to_create_directories.push(path.to_string());
            }
            continue;
        }

        match target_map.get(path) {
            None => {
                // File in source but not in target → download
                actions.to_download.push(source.clone());
            }
            Some(target) => {
                // File in both — download if source has different size or newer timestamp
                if source.size != target.size || source.modified_at > target.modified_at {
                    actions.to_download.push(source.clone());
                }
            }
        }
    }

    // Files in target but not in source → delete (unless ignored)
    if delete_mode != DeleteMode::Ignore {
        for (&path, &target) in target_map {
            if target.is_directory {
                continue;
            }
            if !source_map.contains_key(path) {
                actions.to_delete.push(path.to_string());
            }
        }
    }
}

fn compute_two_way(
    source_map: &HashMap<&str, &FileState>,
    target_map: &HashMap<&str, &FileState>,
    actions: &mut SyncActions,
) {
    // Check source entries against target
    for (&path, &source) in source_map {
        if source.is_directory {
            if !target_map.contains_key(path) {
                actions.to_create_directories.push(path.to_string());
            }
            continue;
        }

        match target_map.get(path) {
            None => {
                // Only on source → download to target
                actions.to_download.push(source.clone());
            }
            Some(target) => {
                if source.modified_at == target.modified_at && source.size == target.size {
                    // Unchanged — skip
                } else if source.modified_at == target.modified_at && source.size != target.size {
                    // Same timestamp, different size → conflict
                    actions.conflicts.push(SyncConflict {
                        relative_path: path.to_string(),
                        source_state: source.clone(),
                        target_state: (*target).clone(),
                    });
                } else if source.modified_at > target.modified_at {
                    // Source newer → download
                    actions.to_download.push(source.clone());
                } else {
                    // Target newer → upload
                    actions.to_upload.push((*target).clone());
                }
            }
        }
    }

    // Entries only on target
    for (&path, &target) in target_map {
        if source_map.contains_key(path) {
            continue;
        }

        if target.is_directory {
            actions.to_create_directories.push(path.to_string());
        } else {
            // Only on target → upload to source
            actions.to_upload.push(target.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(relative_path: &str, size: u64, modified_at: u64) -> FileState {
        FileState {
            relative_path: relative_path.to_string(),
            size,
            modified_at,
            is_directory: false,
        }
    }

    fn dir(relative_path: &str) -> FileState {
        FileState {
            relative_path: relative_path.to_string(),
            size: 0,
            modified_at: 0,
            is_directory: true,
        }
    }

    #[test]
    fn one_way_new_files() {
        let source = vec![file("a.txt", 100, 1000)];
        let target = vec![];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::OneWay, DeleteMode::Trash);

        assert_eq!(actions.to_download.len(), 1);
        assert_eq!(actions.to_download[0].relative_path, "a.txt");
        assert!(actions.to_upload.is_empty());
        assert!(actions.to_delete.is_empty());
        assert!(actions.conflicts.is_empty());
    }

    #[test]
    fn one_way_modified_by_size() {
        let source = vec![file("a.txt", 200, 1000)];
        let target = vec![file("a.txt", 100, 1000)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::OneWay, DeleteMode::Trash);

        assert_eq!(actions.to_download.len(), 1);
        assert_eq!(actions.to_download[0].size, 200);
    }

    #[test]
    fn one_way_modified_by_timestamp() {
        let source = vec![file("a.txt", 100, 2000)];
        let target = vec![file("a.txt", 100, 1000)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::OneWay, DeleteMode::Trash);

        assert_eq!(actions.to_download.len(), 1);
        assert_eq!(actions.to_download[0].modified_at, 2000);
    }

    #[test]
    fn one_way_deleted_files() {
        let source = vec![];
        let target = vec![file("old.txt", 50, 500)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::OneWay, DeleteMode::Permanent);

        assert_eq!(actions.to_delete.len(), 1);
        assert_eq!(actions.to_delete[0], "old.txt");
        assert!(actions.to_download.is_empty());
    }

    #[test]
    fn one_way_delete_mode_ignore() {
        let source = vec![];
        let target = vec![file("old.txt", 50, 500)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::OneWay, DeleteMode::Ignore);

        assert!(actions.to_delete.is_empty());
    }

    #[test]
    fn one_way_unchanged_skipped() {
        let source = vec![file("same.txt", 100, 1000)];
        let target = vec![file("same.txt", 100, 1000)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::OneWay, DeleteMode::Trash);

        assert!(actions.to_download.is_empty());
        assert!(actions.to_upload.is_empty());
        assert!(actions.to_delete.is_empty());
        assert!(actions.conflicts.is_empty());
    }

    #[test]
    fn two_way_source_newer() {
        let source = vec![file("doc.txt", 100, 2000)];
        let target = vec![file("doc.txt", 100, 1000)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::TwoWay, DeleteMode::Trash);

        assert_eq!(actions.to_download.len(), 1);
        assert_eq!(actions.to_download[0].relative_path, "doc.txt");
        assert!(actions.to_upload.is_empty());
        assert!(actions.conflicts.is_empty());
    }

    #[test]
    fn two_way_target_newer() {
        let source = vec![file("doc.txt", 100, 1000)];
        let target = vec![file("doc.txt", 100, 2000)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::TwoWay, DeleteMode::Trash);

        assert!(actions.to_download.is_empty());
        assert_eq!(actions.to_upload.len(), 1);
        assert_eq!(actions.to_upload[0].relative_path, "doc.txt");
        assert!(actions.conflicts.is_empty());
    }

    #[test]
    fn two_way_conflict_same_timestamp_different_size() {
        let source = vec![file("doc.txt", 100, 1000)];
        let target = vec![file("doc.txt", 200, 1000)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::TwoWay, DeleteMode::Trash);

        assert!(actions.to_download.is_empty());
        assert!(actions.to_upload.is_empty());
        assert_eq!(actions.conflicts.len(), 1);
        assert_eq!(actions.conflicts[0].relative_path, "doc.txt");
        assert_eq!(actions.conflicts[0].source_state.size, 100);
        assert_eq!(actions.conflicts[0].target_state.size, 200);
    }

    #[test]
    fn two_way_new_on_source_only() {
        let source = vec![file("new_src.txt", 100, 1000)];
        let target = vec![];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::TwoWay, DeleteMode::Trash);

        assert_eq!(actions.to_download.len(), 1);
        assert_eq!(actions.to_download[0].relative_path, "new_src.txt");
        assert!(actions.to_upload.is_empty());
    }

    #[test]
    fn two_way_new_on_target_only() {
        let source = vec![];
        let target = vec![file("new_tgt.txt", 100, 1000)];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::TwoWay, DeleteMode::Trash);

        assert!(actions.to_download.is_empty());
        assert_eq!(actions.to_upload.len(), 1);
        assert_eq!(actions.to_upload[0].relative_path, "new_tgt.txt");
    }

    #[test]
    fn directories_sorted_parents_first() {
        let source = vec![
            dir("a/b/c"),
            dir("a"),
            dir("a/b"),
            dir("x"),
        ];
        let target = vec![];

        let actions =
            compute_sync_actions(&source, &target, SyncDirection::OneWay, DeleteMode::Trash);

        assert_eq!(actions.to_create_directories.len(), 4);
        // Depth-0 entries (no slash) must come before depth-1, which come before depth-2
        let depths: Vec<usize> = actions
            .to_create_directories
            .iter()
            .map(|p| p.matches('/').count())
            .collect();
        assert_eq!(depths, vec![0, 0, 1, 2]);
        // Verify all expected paths are present
        assert!(actions.to_create_directories.contains(&"a".to_string()));
        assert!(actions.to_create_directories.contains(&"x".to_string()));
        assert!(actions.to_create_directories.contains(&"a/b".to_string()));
        assert!(actions.to_create_directories.contains(&"a/b/c".to_string()));
    }

    #[test]
    fn empty_manifests_produce_empty_actions() {
        let actions =
            compute_sync_actions(&[], &[], SyncDirection::OneWay, DeleteMode::Trash);

        assert!(actions.to_download.is_empty());
        assert!(actions.to_upload.is_empty());
        assert!(actions.to_delete.is_empty());
        assert!(actions.to_create_directories.is_empty());
        assert!(actions.conflicts.is_empty());

        let actions =
            compute_sync_actions(&[], &[], SyncDirection::TwoWay, DeleteMode::Trash);

        assert!(actions.to_download.is_empty());
        assert!(actions.to_upload.is_empty());
        assert!(actions.to_delete.is_empty());
        assert!(actions.to_create_directories.is_empty());
        assert!(actions.conflicts.is_empty());
    }
}
