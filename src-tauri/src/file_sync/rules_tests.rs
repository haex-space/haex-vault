//! Which rules holzi accepts (FR-002, FR-004): every error code, overlap within the rule and with
//! other rules also crosswise, and `BToA` stored as `AToB`.

use std::path::PathBuf;

use super::*;
use crate::file_sync::model::SyncRuleDeleteMode;

fn device(path: &str) -> SyncRulePlace {
    SyncRulePlace {
        source: SourceRef::Device,
        path: path.to_owned(),
    }
}

fn storage(id: &str, path: &str) -> SyncRulePlace {
    SyncRulePlace {
        source: SourceRef::Storage {
            storage_id: id.to_owned(),
        },
        path: path.to_owned(),
    }
}

fn draft(a: SyncRulePlace, b: SyncRulePlace) -> SyncRuleDraft {
    SyncRuleDraft {
        name: "Fotos".to_owned(),
        place_a: a,
        place_b: b,
        direction: SyncRuleDirection::Both,
        delete_mode: SyncRuleDeleteMode::Mirror,
        interval_secs: 900,
        enabled: true,
    }
}

fn stored_rule(name: &str, a: SyncRulePlace, b: SyncRulePlace) -> SyncRule {
    let d = draft(a, b);
    SyncRule {
        id: name.to_owned(),
        name: name.to_owned(),
        place_a: d.place_a,
        place_b: d.place_b,
        direction: d.direction,
        delete_mode: d.delete_mode,
        interval_secs: d.interval_secs,
        enabled: true,
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
    assert_eq!(got.place_a, device("/home/anna/Fotos"));
    assert_eq!(got.place_b, storage("s3", "Backup/Fotos/"));
}

#[test]
fn b_to_a_is_stored_as_a_to_b_with_swapped_places() {
    let mut d = draft(device("/home/a"), storage("s3", "x/"));
    d.direction = SyncRuleDirection::BToA;
    let got = validate(d, &[], &own()).expect("accepted");
    assert_eq!(got.direction, SyncRuleDirection::AToB);
    assert_eq!(got.place_a, storage("s3", "x/"));
    assert_eq!(got.place_b, device("/home/a"));
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
    let others = [stored_rule(
        "Alt",
        device("/home/a"),
        storage("s3", "Fotos/"),
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
