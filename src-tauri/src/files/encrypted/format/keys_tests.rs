use super::*;
use crate::files::encrypted::format::vectors::{check_keys, parse, V1};

#[test]
fn the_keys_match_the_vectors() {
    let vectors = parse(V1).unwrap();
    assert_eq!(check_keys(&vectors), Ok(6));
}

#[test]
fn each_purpose_gets_its_own_key() {
    let fk = [7u8; 32];
    let keys = FolderKeys::derive(&fk);
    assert_ne!(*keys.meta, *keys.wrap);
    assert_ne!(*kek(&fk), *keys.meta);
    assert_ne!(*kek(&fk), *keys.wrap);
}

#[test]
fn another_content_key_gives_another_kek() {
    assert_ne!(*kek(&[1u8; 32]), *kek(&[2u8; 32]));
}
