use super::*;
use crate::files::encrypted::format::vectors::{check_head, parse, V1};

const FID: Id = [3u8; 16];
const KEY_ID: Id = [9u8; 16];
const VCK: [u8; 32] = [5u8; 32];
const FK: [u8; 32] = [6u8; 32];

fn own_vault(key_id: &Id) -> Option<Zeroizing<[u8; 32]>> {
    (*key_id == KEY_ID).then(|| Zeroizing::new(VCK))
}

fn opened(head: &[u8]) -> OpenedHead {
    match open_head(head, &FID, own_vault) {
        HeadOutcome::Readable(opened) => opened,
        other => panic!("expected a readable head, got {other:?}"),
    }
}

#[test]
fn the_head_matches_the_vectors_and_every_head_case() {
    let vectors = parse(V1).unwrap();
    let checked = check_head(&vectors).unwrap();
    assert!(checked > 6, "{checked}");
}

#[test]
fn a_new_head_opens_with_its_name_key_and_generation() {
    let head = seal_head(&FID, &FK, "Unterlagen", &KEY_ID, &VCK).unwrap();
    let opened = opened(&head);
    assert_eq!(*opened.fk, FK);
    assert_eq!(opened.name, "Unterlagen");
    assert_eq!(opened.key_id, KEY_ID);
}

#[test]
fn two_new_heads_differ_in_their_nonces() {
    let a = seal_head(&FID, &FK, "Unterlagen", &KEY_ID, &VCK).unwrap();
    let b = seal_head(&FID, &FK, "Unterlagen", &KEY_ID, &VCK).unwrap();
    assert_ne!(a, b);
}

#[test]
fn a_rename_keeps_the_recipients_and_takes_a_new_name_nonce() {
    let head = seal_head(&FID, &FK, "Unterlagen", &KEY_ID, &VCK).unwrap();
    let renamed = rename_head(&head, &FK, "Belege").unwrap();
    let recipients_end = FIXED_LEN + 3 + OWN_LEN;
    assert_eq!(renamed[..recipients_end], head[..recipients_end]);
    assert_ne!(
        renamed[recipients_end..recipients_end + NONCE_LEN],
        head[recipients_end..recipients_end + NONCE_LEN]
    );
    assert_eq!(opened(&renamed).name, "Belege");
}

#[test]
fn a_name_that_check_name_refuses_is_not_sealed() {
    for name in ["", "a/b", "trailing."] {
        assert!(matches!(
            seal_head(&FID, &FK, name, &KEY_ID, &VCK),
            Err(FormatError::Name(_))
        ));
        let head = seal_head(&FID, &FK, "ok", &KEY_ID, &VCK).unwrap();
        assert!(matches!(
            rename_head(&head, &FK, name),
            Err(FormatError::Name(_))
        ));
    }
}

#[test]
fn an_unknown_recipient_with_a_large_len_is_skipped() {
    let head = seal_head(&FID, &FK, "Unterlagen", &KEY_ID, &VCK).unwrap();
    let mut with_space = head[..FIXED_LEN].to_vec();
    with_space[FIXED_LEN - 1] = 2;
    with_space.push(0x01);
    with_space.extend_from_slice(&300u16.to_le_bytes());
    with_space.extend_from_slice(&[0xaa; 300]);
    with_space.extend_from_slice(&head[FIXED_LEN..]);
    assert_eq!(opened(&with_space).name, "Unterlagen");
}

#[test]
fn a_recipient_count_beyond_the_bytes_is_damaged() {
    let mut head = seal_head(&FID, &FK, "Unterlagen", &KEY_ID, &VCK).unwrap();
    head[FIXED_LEN - 1] = 200;
    assert!(matches!(
        open_head(&head, &FID, own_vault),
        HeadOutcome::Damaged
    ));
}

#[test]
fn a_reader_without_the_generation_sees_another_vault() {
    let head = seal_head(&FID, &FK, "Unterlagen", &KEY_ID, &VCK).unwrap();
    assert!(matches!(
        open_head(&head, &FID, |_| None),
        HeadOutcome::OtherVault
    ));
}

#[test]
fn the_outcome_never_prints_the_key_or_the_name() {
    let head = seal_head(&FID, &FK, "Geheimer Name", &KEY_ID, &VCK).unwrap();
    let printed = format!("{:?}", open_head(&head, &FID, own_vault));
    assert_eq!(printed, "Readable");
}
