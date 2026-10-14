use std::collections::{HashMap, HashSet};

use super::*;
use crate::files::encrypted::format::sidecar::WrappedDek;

const FID: Id = [0; 16];
/// 2026-10-11 10:30 UTC.
const T: u64 = 1_791_714_600_000;

fn id(n: u8) -> Id {
    [n; 16]
}

fn folder(sid: u8, entry: u8, parent: Id, name: &str, written: u64) -> Version {
    Version {
        sid: id(sid),
        sidecar: Sidecar {
            entry: id(entry),
            parent,
            name: name.to_owned(),
            revision: 1,
            base: None,
            written,
            kind: SidecarKind::Folder,
        },
    }
}

fn file(sid: u8, entry: u8, parent: Id, name: &str, written: u64) -> Version {
    Version {
        sid: id(sid),
        sidecar: Sidecar {
            entry: id(entry),
            parent,
            name: name.to_owned(),
            revision: 1,
            base: None,
            written,
            kind: SidecarKind::File(FileFacts {
                size: 5,
                modified: written,
                content_type: None,
                sha256: [0; 32],
                content: id(sid + 100),
                cid: id(sid + 100),
                dek: WrappedDek::from_bytes(&[0; 72]).expect("72 bytes"),
            }),
        },
    }
}

fn based(mut version: Version, base: u8, revision: u64) -> Version {
    version.sidecar.base = Some(id(base));
    version.sidecar.revision = revision;
    version
}

fn tree(versions: &[Version]) -> Tree {
    Tree::build(&Inputs {
        fid: FID,
        versions,
        broken: &[],
        contents: None,
        damaged: &HashSet::new(),
    })
}

fn names(tree: &Tree, index: usize) -> Vec<String> {
    let mut names: Vec<String> = tree
        .children(index)
        .iter()
        .map(|&child| tree.node(child).name.clone())
        .collect();
    names.sort();
    names
}

#[test]
fn the_newest_revision_of_an_entry_counts() {
    let tree = tree(&[
        file(1, 10, FID, "alt.txt", T),
        based(file(2, 10, FID, "neu.txt", T + 1), 1, 2),
    ]);
    assert_eq!(names(&tree, 0), ["neu.txt"]);
}

#[test]
fn two_versions_on_one_base_both_show_the_older_as_a_conflict_copy() {
    let tree = tree(&[
        file(1, 10, FID, "notiz.txt", T - 60_000),
        based(file(2, 10, FID, "notiz.txt", T + 60_000), 1, 2),
        based(file(3, 10, FID, "notiz.txt", T), 1, 2),
    ]);
    assert_eq!(
        names(&tree, 0),
        ["notiz (Konflikt 2026-10-11 10-30).txt", "notiz.txt"]
    );
    let copy = tree
        .find(&["notiz (Konflikt 2026-10-11 10-30).txt"])
        .unwrap();
    assert!(tree.node(copy).conflict);
    assert!(!tree.node(tree.find(&["notiz.txt"]).unwrap()).conflict);
}

#[test]
fn one_name_for_two_entries_in_one_parent_is_named_the_same_way() {
    let tree = tree(&[
        folder(1, 10, FID, "Belege", T),
        folder(2, 11, FID, "Belege", T + 5),
    ]);
    assert_eq!(
        names(&tree, 0),
        ["Belege", "Belege (Konflikt 2026-10-11 10-30)"]
    );
}

#[test]
fn entries_without_their_parent_go_to_the_recovered_folder() {
    let tree = tree(&[
        folder(1, 10, FID, "Belege", T),
        file(2, 20, id(99), "strom.pdf", T),
    ]);
    assert_eq!(names(&tree, 0), ["Belege", RECOVERED]);
    let recovered = tree.find(&[RECOVERED]).unwrap();
    assert_eq!(names(&tree, recovered), ["strom.pdf"]);
    assert!(tree.is_dir(recovered));
    assert_eq!(tree.parent_id(recovered), None);
}

#[test]
fn entries_that_only_reach_each_other_are_recovered() {
    let tree = tree(&[folder(1, 10, id(11), "A", T), folder(2, 11, id(10), "B", T)]);
    let recovered = tree.find(&[RECOVERED]).unwrap();
    assert_eq!(names(&tree, recovered), ["A"]);
    assert!(tree.find(&[RECOVERED, "A", "B"]).is_some());
}

#[test]
fn empty_folders_stay_and_paths_lead_to_their_entry() {
    let tree = tree(&[
        folder(1, 10, FID, "a", T),
        folder(2, 11, id(10), "b", T),
        file(3, 12, id(11), "c", T),
        folder(4, 13, FID, "leer", T),
    ]);
    assert_eq!(names(&tree, 0), ["a", "leer"]);
    let leer = tree.find(&["leer"]).unwrap();
    assert!(tree.children(leer).is_empty());
    let c = tree.find(&["a", "b", "c"]).unwrap();
    assert_eq!(tree.node(c).name, "c");
    assert!(!tree.is_dir(c));
    assert_eq!(
        tree.parent_id(tree.find(&["a", "b"]).unwrap()),
        Some(id(11))
    );
    assert_eq!(tree.parent_id(0), Some(FID));
    assert!(tree.find(&["a", "x"]).is_none());
    let below: Vec<Vec<String>> = tree.below(0).into_iter().map(|(_, rel)| rel).collect();
    assert!(below.contains(&vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]));
    let entry = tree.entry(c, "/U/a/b/c".to_owned());
    assert_eq!(entry.size, Some(5));
    assert_eq!(
        entry.encrypted,
        Some(EncryptedInfo {
            role: EncryptedRole::Inside,
            state: EncryptedState::Readable,
            foreign: 0,
        })
    );
}

#[test]
fn broken_sidecars_and_missing_or_wrong_content_are_damaged() {
    let good = file(1, 10, FID, "gut.txt", T);
    let short = file(2, 11, FID, "kurz.txt", T);
    let gone = file(3, 12, FID, "weg.txt", T);
    let read_failed = file(4, 13, FID, "gelesen.txt", T);
    let contents = HashMap::from([
        (id(101), ciphertext_len(5)),
        (id(102), ciphertext_len(5) - 1),
        (id(104), ciphertext_len(5)),
    ]);
    let tree = Tree::build(&Inputs {
        fid: FID,
        versions: &[good, short, gone, read_failed],
        broken: &[id(9)],
        contents: Some(&contents),
        damaged: &HashSet::from([id(4)]),
    });
    let state = |name: &str| {
        let index = tree.find(&[name]).unwrap();
        tree.entry(index, String::new()).encrypted.unwrap().state
    };
    assert_eq!(state("gut.txt"), EncryptedState::Readable);
    assert_eq!(state("kurz.txt"), EncryptedState::Damaged);
    assert_eq!(state("weg.txt"), EncryptedState::Damaged);
    assert_eq!(state("gelesen.txt"), EncryptedState::Damaged);
    assert_eq!(state(&encode_id(&id(9))), EncryptedState::Damaged);
}

/// What a tree shows: every node's path and name, sorted.
fn shown(tree: &Tree) -> Vec<(Vec<String>, bool)> {
    let mut all: Vec<(Vec<String>, bool)> = tree
        .below(0)
        .into_iter()
        .map(|(index, rel)| (rel, tree.node(index).conflict))
        .collect();
    all.sort();
    all
}

#[test]
fn appending_new_entries_shows_what_a_new_build_shows() {
    let mut versions = vec![
        folder(1, 1, FID, "Belege", T),
        file(2, 2, id(1), "a.txt", T + 1),
    ];
    let mut grown = tree(&versions);
    // New files and folders in the root, in a folder, and in a new folder: appended.
    for (version, appended) in [
        (file(3, 3, FID, "b.txt", T + 2), true),
        (folder(4, 4, id(1), "2026", T + 3), true),
        (file(5, 5, id(4), "c.txt", T + 4), true),
        // A taken name, and a parent that is not there: the tree must be built again.
        (file(6, 6, FID, "b.txt", T + 5), false),
        (file(7, 7, id(99), "d.txt", T + 6), false),
    ] {
        assert_eq!(grown.append(&version), appended, "{}", version.sidecar.name);
        versions.push(version);
        if !appended {
            grown = tree(&versions);
        }
    }
    assert_eq!(shown(&grown), shown(&tree(&versions)));
}
