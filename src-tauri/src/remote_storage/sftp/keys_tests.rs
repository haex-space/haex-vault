//! SSH keys (spec 050 research R4, T065): a generated key reads back and names itself `holzi`,
//! keys of other forms read, a wrong passphrase does not open a key, pending keys go when taken
//! and when the vault locks, and a host key line gives the fingerprint OpenSSH shows.

use super::keys::{decode, fingerprint, generate, host_key_line, parse_host_key, PendingKeys};

#[test]
fn a_generated_key_reads_back_and_names_itself_holzi() {
    let (private, public) = generate().expect("generate");
    assert!(public.starts_with("ssh-ed25519 "), "{public}");
    assert!(public.ends_with(" holzi"), "{public}");
    let key = decode(&private, None).expect("decode");
    assert_eq!(
        key.public_key().to_openssh().expect("line"),
        public,
        "the private key belongs to the public line"
    );
    let (other, _) = generate().expect("generate");
    assert_ne!(*private, *other, "every key is new");
}

#[test]
fn a_key_with_a_passphrase_opens_only_with_it() {
    let (private, _) = generate().expect("generate");
    let key = decode(&private, None).expect("decode");
    let encrypted = key
        .encrypt(&mut rand_core_rng(), "geheim")
        .expect("encrypt")
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .expect("pem");
    assert!(decode(&encrypted, Some("geheim")).is_some());
    assert!(decode(&encrypted, Some("falsch")).is_none());
    assert!(decode(&encrypted, None).is_none());
    assert!(decode("kein Schlüssel", None).is_none());
}

/// A random generator of the version `ssh-key` takes.
fn rand_core_rng() -> impl russh::keys::ssh_key::rand_core::CryptoRng {
    struct Rng;
    impl russh::keys::ssh_key::rand_core::TryRng for Rng {
        type Error = std::convert::Infallible;
        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            let mut bytes = [0; 4];
            getrandom::fill(&mut bytes).expect("random");
            Ok(u32::from_le_bytes(bytes))
        }
        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            let mut bytes = [0; 8];
            getrandom::fill(&mut bytes).expect("random");
            Ok(u64::from_le_bytes(bytes))
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
            getrandom::fill(dest).expect("random");
            Ok(())
        }
    }
    impl russh::keys::ssh_key::rand_core::TryCryptoRng for Rng {}
    Rng
}

#[test]
fn a_host_key_line_gives_the_fingerprint_openssh_shows() {
    let (_, public) = generate().expect("generate");
    let key = parse_host_key(&public).expect("parse");
    let line = host_key_line(&key);
    assert!(
        line.starts_with("ssh-ed25519 ") && !line.ends_with("holzi"),
        "{line}"
    );
    let print = fingerprint(&line).expect("fingerprint");
    assert!(print.starts_with("SHA256:"), "{print}");
    assert_eq!(
        print.len(),
        "SHA256:".len() + 43,
        "unpadded base64 of 32 bytes"
    );
    assert_eq!(
        fingerprint(&public),
        Some(print),
        "the comment does not count"
    );
    assert!(fingerprint("ssh-ed25519 kaputt").is_none());
}

#[test]
fn a_pending_key_goes_when_taken_and_when_the_vault_locks() {
    let pending = PendingKeys::default();
    let (id, public) = pending.generate().expect("generate");
    let private = pending.take(&id).expect("taken");
    assert_eq!(
        decode(&private, None)
            .expect("decode")
            .public_key()
            .to_openssh()
            .expect("line"),
        public
    );
    assert!(pending.take(&id).is_none(), "taken once");
    let (id, _) = pending.generate().expect("generate");
    pending.clear();
    assert!(pending.take(&id).is_none(), "gone when the vault locks");
    assert!(pending.take("unbekannt").is_none());
}
