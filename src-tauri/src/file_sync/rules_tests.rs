//! Which rules holzi accepts (FR-002, FR-004): every error code, overlap within the rule and with
//! other rules also crosswise.

use std::path::PathBuf;

use super::*;
use crate::file_sync::model::{SyncRuleDeleteMode, SyncRuleDirection};

fn device(path: &str) -> SyncRulePlace {
    SyncRulePlace {
        location: SourceRef::Device,
        path: path.to_owned(),
    }
}

fn storage(id: &str, path: &str) -> SyncRulePlace {
    SyncRulePlace {
        location: SourceRef::Storage {
            storage_id: id.to_owned(),
        },
        path: path.to_owned(),
    }
}

fn draft(source: SyncRulePlace, target: SyncRulePlace) -> SyncRuleDraft {
    SyncRuleDraft {
        name: "Fotos".to_owned(),
        source,
        target,
        source_kind: None,
        direction: SyncRuleDirection::Both,
        delete_mode: SyncRuleDeleteMode::Mirror,
        interval_secs: 900,
        enabled: true,
        allow_decrypted: false,
    }
}

fn stored_rule(name: &str, source: SyncRulePlace, target: SyncRulePlace) -> SyncRule {
    let d = draft(source, target);
    let source_kind = d.kind();
    SyncRule {
        id: name.to_owned(),
        name: name.to_owned(),
        source: d.source,
        target: d.target,
        source_kind,
        direction: d.direction,
        delete_mode: d.delete_mode,
        interval_secs: d.interval_secs,
        enabled: true,
        allow_decrypted: false,
        created_at: String::new(),
        updated_at: String::new(),
        last_run_at: None,
        last_outcome: None,
        last_error: None,
    }
}

fn own() -> OwnPlaces {
    OwnPlaces::new(vec![PathBuf::from("/data/holzi")])
}

fn code(result: Result<SyncRuleDraft, SyncRuleError>) -> SyncRuleErrorCode {
    result.expect_err("refused").code
}

#[test]
fn a_good_rule_is_normalised() {
    let got = validate(
        draft(device("/home/anna/Fotos/"), storage("s3", "/Backup/Fotos")),
        &[],
        &own(),
    )
    .expect("accepted");
    assert_eq!(got.source, device("/home/anna/Fotos"));
    assert_eq!(got.target, storage("s3", "/Backup/Fotos"));
}

#[test]
fn names_and_intervals_out_of_range_are_refused() {
    let mut d = draft(device("/home/a"), device("/home/b"));
    d.name = "  ".to_owned();
    assert_eq!(
        code(validate(d, &[], &own())),
        SyncRuleErrorCode::InvalidName
    );
    let mut d = draft(device("/home/a"), device("/home/b"));
    d.name = "x".repeat(81);
    assert_eq!(
        code(validate(d, &[], &own())),
        SyncRuleErrorCode::InvalidName
    );
    let mut d = draft(device("/home/a"), device("/home/b"));
    d.interval_secs = 59;
    assert_eq!(
        code(validate(d, &[], &own())),
        SyncRuleErrorCode::InvalidInterval
    );
    let mut d = draft(device("/home/a"), device("/home/b"));
    d.interval_secs = 86_401;
    assert_eq!(
        code(validate(d, &[], &own())),
        SyncRuleErrorCode::InvalidInterval
    );
}

#[test]
fn contact_shares_own_places_and_bad_paths_are_refused() {
    for place in [
        storage("share:abc", "x/"),
        device("/data/holzi/models"),
        device("/data"),
        device("relative/path"),
        device("/home/a/../b"),
    ] {
        assert_eq!(
            code(validate(
                draft(place.clone(), device("/home/b")),
                &[],
                &own()
            )),
            SyncRuleErrorCode::InvalidPlace,
            "{place:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_device_symlink_into_an_own_place_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let own_root = root.path().join("own");
    let link = root.path().join("linked");
    std::fs::create_dir(&own_root).unwrap();
    std::os::unix::fs::symlink(&own_root, &link).unwrap();
    let own = OwnPlaces::new(vec![std::fs::canonicalize(&own_root).unwrap()]);
    assert_eq!(
        code(validate(
            draft(device(link.to_str().unwrap()), device("/home/b")),
            &[],
            &own,
        )),
        SyncRuleErrorCode::InvalidPlace
    );
}

#[test]
fn the_two_places_of_one_rule_must_not_overlap() {
    assert_eq!(
        code(validate(
            draft(device("/home/a"), device("/home/a/sub")),
            &[],
            &own()
        )),
        SyncRuleErrorCode::Overlap
    );
    assert_eq!(
        code(validate(
            draft(storage("s3", "x/"), storage("s3", "")),
            &[],
            &own()
        )),
        SyncRuleErrorCode::Overlap,
        "the root of a storage holds every folder"
    );
    validate(
        draft(storage("s3", "x/"), storage("s3", "xy/")),
        &[],
        &own(),
    )
    .expect("siblings with a common prefix do not overlap");
    validate(draft(storage("s3", "x/"), storage("s4", "x/")), &[], &own())
        .expect("other storages do not overlap");
}

#[test]
fn a_place_must_not_overlap_another_rule_also_crosswise() {
    // Stored rules are normalised (window paths).
    let others = [stored_rule(
        "Alt",
        device("/home/a"),
        storage("s3", "/Fotos"),
    )];
    assert_eq!(
        code(validate(
            draft(device("/home/a/sub"), device("/home/z")),
            &others,
            &own()
        )),
        SyncRuleErrorCode::Overlap
    );
    assert_eq!(
        code(validate(
            draft(device("/home/z"), storage("s3", "Fotos/2026/")),
            &others,
            &own()
        )),
        SyncRuleErrorCode::Overlap,
        "B of the new rule lies in B of the old one"
    );
    assert_eq!(
        code(validate(
            draft(storage("s3", "Fotos/"), device("/home/z")),
            &others,
            &own()
        )),
        SyncRuleErrorCode::Overlap,
        "A of the new rule is B of the old one"
    );
    validate(
        draft(device("/home/b"), storage("s3", "Musik/")),
        &others,
        &own(),
    )
    .expect("no overlap");
}

// ---- file rules (spec 074) ----

use crate::file_sync::model::SyncRuleSourceKind;

fn file_draft(source: SyncRulePlace, target: SyncRulePlace) -> SyncRuleDraft {
    SyncRuleDraft {
        source_kind: Some(SyncRuleSourceKind::File),
        ..draft(source, target)
    }
}

fn stored_file_rule(name: &str, source: SyncRulePlace, target: SyncRulePlace) -> SyncRule {
    SyncRule {
        source_kind: SyncRuleSourceKind::File,
        ..stored_rule(name, source, target)
    }
}

#[test]
fn a_file_rule_validates_for_a_regular_file_and_stores_no_delete_mode() {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    let file = real.join("Haushalt.ods");
    std::fs::write(&file, "x").unwrap();
    let got = validate(
        file_draft(device(&file.to_string_lossy()), device("/home/b")),
        &[],
        &own(),
    )
    .expect("accepted");
    assert_eq!(got.source_kind, Some(SyncRuleSourceKind::File));
    assert_eq!(got.delete_mode, SyncRuleDeleteMode::Keep, "had Mirror");
    assert_eq!(got.direction, SyncRuleDirection::Both, "both ways stay");
    assert_eq!(got.target, device("/home/b"));
}

#[test]
fn a_draft_without_a_kind_validates_as_a_folder_rule() {
    let got = validate(
        draft(device("/home/anna/Fotos"), device("/home/b")),
        &[],
        &own(),
    )
    .expect("accepted");
    assert_eq!(got.source_kind, Some(SyncRuleSourceKind::Folder));
    assert_eq!(
        got.delete_mode,
        SyncRuleDeleteMode::Mirror,
        "folder rules keep it"
    );
}

#[test]
fn a_folder_a_link_and_a_root_are_no_source_file() {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    let folder = real.join("ordner");
    std::fs::create_dir(&folder).unwrap();
    assert_eq!(
        code(validate(
            file_draft(device(&folder.to_string_lossy()), device("/home/b")),
            &[],
            &own()
        )),
        SyncRuleErrorCode::InvalidPlace,
        "a folder"
    );
    #[cfg(unix)]
    {
        let file = real.join("f.txt");
        std::fs::write(&file, "x").unwrap();
        let link = real.join("link.txt");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert_eq!(
            code(validate(
                file_draft(device(&link.to_string_lossy()), device("/home/b")),
                &[],
                &own()
            )),
            SyncRuleErrorCode::InvalidPlace,
            "a symbolic link is not followed"
        );
    }
    assert_eq!(
        code(validate(
            file_draft(storage("s3", "/"), device("/home/b")),
            &[],
            &own()
        )),
        SyncRuleErrorCode::InvalidPlace,
        "a root has no file name"
    );
}

#[test]
fn a_file_that_is_not_there_is_let_through_for_a_rule_to_report() {
    // It may lie in an encrypted folder, which the file system does not show under that name.
    assert!(validate(
        file_draft(device("/home/anna/nicht-da.ods"), device("/home/b")),
        &[],
        &own()
    )
    .is_ok());
}

#[test]
fn a_file_under_holzis_own_places_is_refused() {
    assert_eq!(
        code(validate(
            file_draft(device("/data/holzi/models/x.gguf"), device("/home/b")),
            &[],
            &own()
        )),
        SyncRuleErrorCode::InvalidPlace
    );
}

#[test]
fn overlap_is_judged_by_the_files_a_rule_claims() {
    let file_rule = stored_file_rule("datei", device("/home/anna/f.txt"), device("/home/b"));
    let folder_rule = stored_rule("ordner", device("/home/x"), device("/home/z"));
    let others = [file_rule, folder_rule];
    let refused = |source: &str, target: &str, why: &str| {
        assert_eq!(
            code(validate(
                file_draft(device(source), device(target)),
                &others,
                &own()
            )),
            SyncRuleErrorCode::Overlap,
            "{why}"
        );
    };
    refused(
        "/home/anna/f.txt",
        "/home/c",
        "the same file as the source of two rules",
    );
    refused(
        "/home/x/y/f.txt",
        "/home/c",
        "a file inside another rule's folder place",
    );
    refused(
        "/home/other/f.txt",
        "/home/b",
        "two rules would write the same copy",
    );
    refused(
        "/home/q/f.txt",
        "/home/q",
        "the target folder is the file's own folder",
    );
    refused(
        "/home/q/f.txt",
        "/home/z/sub",
        "the copy lies in another rule's folder place",
    );
    refused(
        "/home/q/f.txt",
        "/home/z",
        "the copy lies in another rule's folder place",
    );

    // Another file from another folder into the same target folder is fine.
    assert!(validate(
        file_draft(device("/home/q/g.txt"), device("/home/b")),
        &others,
        &own()
    )
    .is_ok());
}

#[test]
fn a_folder_rule_over_a_folder_that_holds_a_file_rules_copy_is_refused() {
    let others = [stored_file_rule(
        "datei",
        device("/home/anna/f.txt"),
        device("/home/b/sub"),
    )];
    assert_eq!(
        code(validate(
            draft(device("/home/c"), device("/home/b")),
            &others,
            &own()
        )),
        SyncRuleErrorCode::Overlap,
        "the copy /home/b/sub/f.txt lies in the folder place /home/b"
    );
    assert!(validate(draft(device("/home/c"), device("/home/d")), &others, &own()).is_ok());
}
