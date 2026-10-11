use super::*;
use crate::files::encrypted::format::vectors::{check_sidecars, parse, V1};

const FID: Id = [1u8; 16];
const SID: Id = [2u8; 16];
const ENTRY: Id = [3u8; 16];
const CID: Id = [4u8; 16];
const DEK: [u8; 32] = [5u8; 32];

fn keys() -> FolderKeys {
    FolderKeys::derive(&[9u8; 32])
}

fn file() -> Sidecar {
    Sidecar {
        entry: ENTRY,
        parent: FID,
        name: "Rechnung.pdf".to_owned(),
        revision: 1,
        base: None,
        written: 1_791_000_000_000,
        kind: SidecarKind::File(FileFacts {
            size: 1,
            modified: 1_790_000_000_000,
            content_type: None,
            sha256: [7u8; 32],
            content: CID,
            cid: CID,
            dek: WrappedDek::wrap(&keys(), &FID, &ENTRY, &CID, &DEK),
        }),
    }
}

fn folder() -> Sidecar {
    Sidecar {
        kind: SidecarKind::Folder,
        name: "Belege".to_owned(),
        base: Some([6u8; 16]),
        revision: 2,
        ..file()
    }
}

#[test]
fn the_sidecars_match_the_vectors_and_every_case_is_refused() {
    let vectors = parse(V1).unwrap();
    assert!(check_sidecars(&vectors).unwrap() > 0);
}

#[test]
fn a_sidecar_round_trips() {
    for sidecar in [file(), folder()] {
        let object = seal_sidecar(&keys(), &FID, &SID, &sidecar).unwrap();
        assert_eq!(open_sidecar(&keys(), &FID, &SID, &object).unwrap(), sidecar);
    }
}

#[test]
fn the_json_has_the_fixed_order_and_a_folder_has_no_file_fields() {
    let json = String::from_utf8(file().to_json()).unwrap();
    let order = [
        "v", "entry", "parent", "name", "kind", "revision", "base", "written", "size", "modified",
        "type", "sha256", "content", "cid", "dek",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|field| json.find(&format!("\"{field}\":")).unwrap())
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{json}");
    assert!(json.contains(r#""type":null"#));
    assert!(!json.contains(' '));

    let json = String::from_utf8(folder().to_json()).unwrap();
    assert!(json.ends_with(r#","written":1791000000000}"#), "{json}");
}

#[test]
fn a_sidecar_of_another_folder_or_under_another_sid_is_refused() {
    let object = seal_sidecar(&keys(), &FID, &SID, &file()).unwrap();
    assert!(matches!(
        open_sidecar(&keys(), &[0u8; 16], &SID, &object),
        Err(FormatError::Decrypt)
    ));
    assert!(matches!(
        open_sidecar(&keys(), &FID, &[0u8; 16], &object),
        Err(FormatError::Mismatch)
    ));
}

#[test]
fn a_rewrapped_dek_opens_only_for_its_entry() {
    let copy_entry = [8u8; 16];
    let rewrapped = WrappedDek::wrap(&keys(), &FID, &copy_entry, &CID, &DEK);
    assert_eq!(
        *rewrapped.unwrap(&keys(), &FID, &copy_entry, &CID).unwrap(),
        DEK
    );
    for (fid, entry, cid) in [
        (FID, ENTRY, CID),
        ([0u8; 16], copy_entry, CID),
        (FID, copy_entry, [0u8; 16]),
    ] {
        assert!(rewrapped.unwrap(&keys(), &fid, &entry, &cid).is_err());
    }
}

#[test]
fn a_name_that_check_name_refuses_is_neither_sealed_nor_read() {
    let mut bad = file();
    bad.name = "a/b".to_owned();
    assert!(matches!(
        seal_sidecar(&keys(), &FID, &SID, &bad),
        Err(FormatError::Name(_))
    ));
    let json = bad.to_json();
    assert!(matches!(
        Sidecar::from_json(&json),
        Err(FormatError::Name(_))
    ));
}

#[test]
fn missing_or_misplaced_file_fields_are_malformed() {
    let file_json = String::from_utf8(file().to_json()).unwrap();
    let without_type = file_json.replace(r#""type":null,"#, "");
    let folder_with_size = String::from_utf8(folder().to_json())
        .unwrap()
        .replace('}', r#","size":1}"#);
    let file_as_folder = file_json.replace(r#""kind":"file""#, r#""kind":"folder""#);
    for json in [without_type, folder_with_size, file_as_folder] {
        assert!(
            matches!(
                Sidecar::from_json(json.as_bytes()),
                Err(FormatError::Malformed)
            ),
            "{json}"
        );
    }
}

#[test]
fn a_newer_json_version_or_invalid_json_is_refused() {
    let json = String::from_utf8(folder().to_json()).unwrap();
    assert!(matches!(
        Sidecar::from_json(json.replace(r#""v":1"#, r#""v":2"#).as_bytes()),
        Err(FormatError::NewerFormat)
    ));
    assert!(matches!(
        Sidecar::from_json(b"{\"v\":1,"),
        Err(FormatError::Json(_))
    ));
}

#[test]
fn a_json_error_does_not_quote_the_name() {
    let json = String::from_utf8(folder().to_json())
        .unwrap()
        .replace(r#""revision":2"#, r#""revision":"Belege""#);
    let error = Sidecar::from_json(json.as_bytes()).unwrap_err();
    assert!(!error.to_string().contains("Belege"), "{error}");
}

#[test]
fn an_unknown_field_makes_the_sidecar_invalid() {
    let json = String::from_utf8(folder().to_json())
        .unwrap()
        .replace('}', r#","later":1}"#);
    assert!(matches!(
        Sidecar::from_json(json.as_bytes()),
        Err(FormatError::Json(_))
    ));
}
