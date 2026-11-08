use super::*;

fn transcript() -> Transcript {
    Transcript {
        nonce_h: [1; 32],
        nonce_n: [2; 32],
        endpoint_h: [3; 32],
        endpoint_n: [4; 32],
        device_h: [5; 32],
        device_n: [6; 32],
    }
}

#[test]
fn a_code_is_26_characters_in_groups_of_four() {
    let code = LinkCode::generate().display();

    assert_eq!(code.replace('-', "").len(), 26);
    let groups: Vec<&str> = code.split('-').collect();
    assert_eq!(groups.len(), 7);
    assert!(groups[..6].iter().all(|g| g.len() == 4));
    assert_eq!(groups[6].len(), 2);
    assert!(code
        .chars()
        .all(|c| c == '-' || c.is_ascii_uppercase() || ('2'..='7').contains(&c)));
}

#[test]
fn a_shown_code_parses_back_to_the_same_code() {
    for _ in 0..50 {
        let code = LinkCode::generate();
        let shown = code.display();
        let parsed = LinkCode::parse(&shown).expect("parses");
        assert_eq!(parsed.0.as_slice(), code.0.as_slice());
        assert_eq!(parsed.display(), shown);
    }
}

#[test]
fn a_typed_code_is_forgiving_about_case_spaces_and_dashes() {
    let code = LinkCode::generate();
    let shown = code.display();
    let sloppy = shown.replace('-', " ").to_lowercase();

    assert_eq!(
        LinkCode::parse(&sloppy).expect("parses").0.as_slice(),
        code.0.as_slice()
    );
    assert_eq!(
        LinkCode::parse(&shown.replace('-', ""))
            .expect("parses")
            .0
            .as_slice(),
        code.0.as_slice()
    );
}

#[test]
fn text_that_is_no_code_is_refused() {
    assert_eq!(LinkCode::parse("").err(), Some(CodeError::Length));
    assert_eq!(LinkCode::parse("ABCD-EFGH").err(), Some(CodeError::Length));
    let too_long = format!("{}A", LinkCode::generate().display());
    assert_eq!(LinkCode::parse(&too_long).err(), Some(CodeError::Length));
    // `1`, `8`, `0` and `9` are not in the alphabet.
    let bad = "1".repeat(26);
    assert_eq!(LinkCode::parse(&bad).err(), Some(CodeError::Character));
}

#[test]
fn a_last_character_with_stray_bits_is_no_code_this_device_showed() {
    // 26 × 5 = 130 bits carry 128: the last character only uses 3 bits.
    let shown = LinkCode::generate().display().replace('-', "");
    let last = shown.chars().last().expect("a character");
    let index = ALPHABET
        .iter()
        .position(|a| *a as char == last)
        .expect("in alphabet");
    let stray = ALPHABET[index ^ 1] as char;
    let altered = format!("{}{stray}", &shown[..25]);

    assert_eq!(LinkCode::parse(&altered).err(), Some(CodeError::Character));
}

#[test]
fn each_purpose_derives_its_own_key() {
    let code = LinkCode::generate();
    let other = LinkCode::generate();

    let (secret, public) = code.rendezvous_keys().expect("keys");
    let (_, other_public) = other.rendezvous_keys().expect("keys");
    assert_eq!(
        code.rendezvous_keys().expect("keys").1,
        public,
        "deterministic"
    );
    assert_ne!(public, other_public);
    assert_ne!(
        secret.to_secret_bytes(),
        code.expand(None, b"holzi/link/proof/v1").as_slice()
    );
}

#[test]
fn a_proof_holds_only_for_its_role_and_its_transcript() {
    let code = LinkCode::generate();
    let t = transcript();
    let mac_h = code.proof(Role::Host, &t);

    assert!(code.verify_proof(Role::Host, &t, &mac_h));
    assert!(
        !code.verify_proof(Role::Joiner, &t, &mac_h),
        "a proof cannot be replayed as the other side's"
    );
    let mut other = t.clone();
    other.endpoint_n = [9; 32];
    assert!(!code.verify_proof(Role::Host, &other, &mac_h));
    assert!(!LinkCode::generate().verify_proof(Role::Host, &t, &mac_h));
    assert!(!code.verify_proof(Role::Host, &t, &mac_h[..31]));
}

#[test]
fn the_session_secret_depends_on_the_code_and_both_nonces() {
    let code = LinkCode::generate();
    let secret = code.session_secret(&[1; 32], &[2; 32]);

    assert_eq!(
        secret.as_slice(),
        code.session_secret(&[1; 32], &[2; 32]).as_slice()
    );
    assert_ne!(
        secret.as_slice(),
        code.session_secret(&[1; 32], &[3; 32]).as_slice()
    );
    assert_ne!(
        secret.as_slice(),
        code.session_secret(&[9; 32], &[2; 32]).as_slice()
    );
    assert_ne!(
        secret.as_slice(),
        LinkCode::generate()
            .session_secret(&[1; 32], &[2; 32])
            .as_slice()
    );
}

#[test]
fn a_resume_proof_holds_only_for_its_link_and_state() {
    let secret = [7u8; 32];
    let link = [1u8; 32];
    let mac = resume_mac(&secret, &link, "awaiting_publication");

    assert!(verify_resume_mac(
        &secret,
        &link,
        "awaiting_publication",
        &mac
    ));
    assert!(!verify_resume_mac(&secret, &link, "transferring", &mac));
    assert!(!verify_resume_mac(
        &secret,
        &[2; 32],
        "awaiting_publication",
        &mac
    ));
    assert!(!verify_resume_mac(
        &[8; 32],
        &link,
        "awaiting_publication",
        &mac
    ));
}

#[test]
fn the_code_never_shows_in_debug_output() {
    let code = LinkCode::generate();
    let debug = format!("{code:?}");

    assert!(!debug.contains(&code.display().replace('-', "")));
    assert!(!debug.contains(&crate::sync::keys::hex(code.0.as_slice())));
}
