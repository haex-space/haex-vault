//! The fixed texts of key derivation, signatures and protocols (spec 054 S3, FR-004). Every
//! expected value below was computed by a Python script (hmac, hashlib, uuid) that knows only the
//! texts, never this code: a text that changes by accident, or a derivation that drifts, fails here.

use super::content_keys::key_id;
use super::keys::derive_vault_identity;
use super::presence::mailbox_keys;
use super::signing::{digest, Domain};
use super::wire::{EXT_PEER_ALPN, FILES_ALPN, LINK_ALPN, SYNC_ALPN};
use crate::contacts::card::format::PREFIX;
use crate::models::huggingface::derive_model_id;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn the_vault_identity_has_fixed_values() {
    assert_eq!(
        hex(derive_vault_identity(&[1u8; 32]).as_slice()),
        "f2043c61700e1df7192b5ac2c8890dc498f2690ac64d4f685c009b40798a6ef1"
    );
    assert_eq!(
        hex(derive_vault_identity(&[42u8; 32]).as_slice()),
        "11d9b90d3a3d787cb71c16781c631225b55df0e97e1681ef43ef15b59e8dd45f"
    );
}

#[test]
fn the_signature_domains_have_fixed_tags_and_digests() {
    let cases = [
        (
            Domain::DeviceAuth,
            "haex-device-auth/v1",
            "6ed45edf8c7330bb3fe07ff670bbd94e2cb114272b2fe6476cdbb4fed32c1105",
        ),
        (
            Domain::DeviceList,
            "haex-device-list/v1",
            "2d31aefb8c8e4afecad904e42bc1331dadc68705728fd2721efbfce44b217e94",
        ),
        (
            Domain::KeyGeneration,
            "haex-key-generation/v1",
            "36c7d2b3422c286f368c4fc41352cb8a9b836cef1d39790ff9ff00a528a6332b",
        ),
        (
            Domain::KeyEnvelope,
            "haex-key-envelope/v1",
            "b75e2b2f386c623286e76ebce11ea36e1ecd0fd02452ce6019cc8c14575a27d2",
        ),
        (
            Domain::Admission,
            "haex-admission/v1",
            "32b9b8d325bd7fb9e274ac141dc82102886008cfb5ad7281f97045c45173920b",
        ),
        (
            Domain::Link,
            "haex-link/v1",
            "09de258af9ccbcc0db218bf6b034e745121ac0244d5879081ad027b35f888969",
        ),
        (
            Domain::LinkResume,
            "haex-link-resume/v1",
            "25c3fd3e1c0ae7baea8725a7f9a96ef7a569d7488c4b5426bf4b77852c18cfa0",
        ),
        (
            Domain::ContactCard,
            "haex-contact-card/v1",
            "3e77b1f0e53e5bc53bfe2ed9ef9a7ce27f3cc0f4c20d045dc6a335d8520b1993",
        ),
        (
            Domain::ExtPeerAuth,
            "haex-ext-peer-auth/v1",
            "d6c2fa13b36540fc3f16dd990d57d8673d7bbf1b85840b6bd71a8f92591595a9",
        ),
        (
            Domain::FilesAuth,
            "haex-files-auth/v1",
            "5afc99ed17e56ed9bbe8aab8df60b1d156c969b081cb6053cf30041223ff3a5f",
        ),
    ];
    for (domain, tag, expected) in cases {
        assert_eq!(domain.tag(), tag.as_bytes(), "tag of {domain:?}");
        assert_eq!(
            hex(&digest(domain, b"abc")),
            expected,
            "digest of {domain:?}"
        );
    }
}

#[test]
fn the_protocols_have_fixed_alpns() {
    assert_eq!(SYNC_ALPN, b"haex-sync/1");
    assert_eq!(LINK_ALPN, b"haex-link/1");
    assert_eq!(EXT_PEER_ALPN, b"haex-ext-peer/1");
    assert_eq!(FILES_ALPN, b"haex-files/1");
}

#[test]
fn the_contact_card_text_has_a_fixed_prefix() {
    assert_eq!(PREFIX, "haex:card:");
}

#[test]
fn the_key_id_has_a_fixed_value() {
    let key: [u8; 32] = std::array::from_fn(|i| i as u8);
    assert_eq!(hex(&key_id(&key)), "8f92b51f31a2895cec18e4ba774a84bc");
}

#[test]
fn the_mailbox_key_has_a_fixed_value() {
    let (secret, _) = mailbox_keys(&[7u8; 32], 20_000).expect("keys");
    assert_eq!(
        hex(&secret.secret_bytes()),
        "c10d15e40514937ff50a855b72180b2df1573c00865309edc4e50d5d4394b7fd"
    );
}

#[test]
fn the_model_id_has_a_fixed_value() {
    assert_eq!(
        derive_model_id("Qwen/Qwen3-4B-GGUF", "Qwen3-4B-Q4_K_M.gguf"),
        "hf-3397ea98c182f958c6d0065f264122018869edeff9ee02dd7fd6a5632fcefda4"
    );
}
