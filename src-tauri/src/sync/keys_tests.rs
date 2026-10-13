use uuid::Uuid;

use super::*;
use crate::storage::query;
use crate::sync::test_support::open_vault;

#[test]
fn two_copies_of_a_seed_derive_the_same_valid_identity() {
    let seed = [42u8; 32];
    let first = derive_vault_identity(&seed);
    let second = derive_vault_identity(&seed);
    assert_eq!(first.as_slice(), second.as_slice());
    assert!(SecretKey::from_secret_bytes(*first).is_ok());
    assert_ne!(
        derive_vault_identity(&[43u8; 32]).as_slice(),
        first.as_slice()
    );
}

#[test]
fn the_derivation_is_hkdf_sha256_with_counter_zero_first() {
    let seed = [1u8; 32];
    let hkdf = Hkdf::<Sha256>::new(Some(b"holzi"), &seed);
    let mut expected = [0u8; 32];
    hkdf.expand(b"holzi/vault-identity/v1\0\0\0\0", &mut expected)
        .expect("expand");
    assert_eq!(derive_vault_identity(&seed).as_slice(), expected);
}

#[test]
fn device_keys_are_created_once_per_installation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let own = Uuid::new_v4();
    let source = Uuid::new_v4();

    let source_keys = db
        .write(|tx| ensure_device_keys(tx, source, 1))
        .expect("source keys");
    let first = db
        .write(|tx| ensure_device_keys(tx, own, 2))
        .expect("own keys");
    let again = db
        .write(|tx| ensure_device_keys(tx, own, 3))
        .expect("own keys again");

    assert_eq!(first.device_pubkey, again.device_pubkey);
    assert_eq!(first.endpoint_id, again.endpoint_id);
    assert_ne!(first.device_pubkey, source_keys.device_pubkey);
    let stored_source = query::read(&db, |r| load_device_keys(r, source))
        .expect("read")
        .expect("the source row stays");
    assert_eq!(stored_source.device_pubkey, source_keys.device_pubkey);
    assert_eq!(
        stored_source.device_secret.as_slice(),
        source_keys.device_secret.as_slice()
    );
}

#[test]
fn the_endpoint_id_is_the_iroh_public_key_of_the_endpoint_secret() {
    let keys = DeviceKeys::generate();
    let expected = *iroh::SecretKey::from_bytes(&keys.endpoint_secret)
        .public()
        .as_bytes();
    assert_eq!(keys.endpoint_id, expected);
    assert_eq!(
        keys.device_pubkey,
        xonly_public_key(&keys.device_secret).expect("public key")
    );
}

#[test]
fn debug_output_shows_no_secret() {
    let keys = DeviceKeys::generate();
    let debug = format!("{keys:?}");
    assert!(!debug.contains(&hex(keys.device_secret.as_slice())));
    assert!(!debug.contains(&hex(keys.endpoint_secret.as_slice())));
    assert!(debug.contains(&hex(&keys.device_pubkey)));
}

#[test]
fn a_new_vault_publishes_an_identity_only_with_genesis() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());

    assert_eq!(
        db.write(|tx| ensure_vault_identity(tx, false))
            .expect("no genesis"),
        None
    );
    let pubkey = db
        .write(|tx| ensure_vault_identity(tx, true))
        .expect("genesis")
        .expect("published");
    let secret = query::read(&db, |r| vault_secret(r))
        .expect("read")
        .expect("a new vault's first device is a main device");
    assert_eq!(xonly_public_key(&secret).expect("public key"), pubkey);
    assert_eq!(
        db.write(|tx| ensure_vault_identity(tx, true))
            .expect("again"),
        Some(pubkey),
        "publishing is idempotent"
    );
}

#[test]
fn a_placeholder_seed_becomes_the_derived_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let seed = [9u8; 32];
    db.write(|tx| {
        tx.execute(
            "INSERT INTO vault_identity_secret_no_sync (id, privkey) VALUES (1, ?1)",
            params![seed.as_slice()],
        )
    })
    .expect("seed");

    let pubkey = db
        .write(|tx| ensure_vault_identity(tx, false))
        .expect("derive")
        .expect("published");

    let derived = derive_vault_identity(&seed);
    assert_eq!(pubkey, xonly_public_key(&derived).expect("public key"));
    let stored = query::read(&db, |r| vault_secret(r))
        .expect("read")
        .expect("secret");
    assert_eq!(
        stored.as_slice(),
        derived.as_slice(),
        "the seed is replaced"
    );
}

/// Every way a secret's bytes could reach a log line: hex, the decimal list
/// `{:?}` prints for a byte array, and the `Zeroizing` wrapper's own debug.
fn secret_forms(secret: &[u8; 32]) -> Vec<String> {
    vec![
        hex(secret),
        format!("{:?}", secret.as_slice()),
        format!("{secret:?}"),
    ]
}

#[test]
fn no_key_holding_type_prints_its_secret() {
    // FR-002: the private key of the vault identity, the device keys and the
    // content key never appear in a Debug rendering of anything that holds
    // them.
    let device = DeviceKeys::generate();
    let content = crate::sync::content_keys::ContentKey::generate(1);
    let vault_secret = random_secret_key();
    let local = crate::sync::handshake::Local {
        keys: &device,
        vault: [3; 32],
        schema: crate::sync::handshake::local_schema(),
    };
    let secrets = [
        *device.device_secret,
        *device.endpoint_secret,
        *content.key,
        *vault_secret,
    ];

    let renderings = [
        format!("{device:?}"),
        format!("{content:?}"),
        format!("{:?}", std::sync::Arc::new(device.clone())),
        format!("{:?}", Some(&device)),
        format!("{:#?}", local.keys),
    ];
    for rendering in &renderings {
        for secret in &secrets {
            for form in secret_forms(secret) {
                assert!(!rendering.contains(&form), "{rendering} shows a secret");
            }
        }
    }
}

#[tokio::test]
async fn a_failing_handshake_logs_no_secret_bytes() {
    use tokio::io::{duplex, split};

    use crate::sync::handshake::{accept, dial};
    use crate::sync::test_support::Member;

    crate::test_log::install();
    let main = Member::genesis();
    let foreign = Member::genesis();
    let main_vault_secret = query::read(main.device.db(), |r| vault_secret(r))
        .expect("read")
        .expect("main holds the vault secret");
    let foreign_vault_secret = query::read(foreign.device.db(), |r| vault_secret(r))
        .expect("read")
        .expect("foreign holds its vault secret");

    let (a_side, d_side) = duplex(1 << 20);
    let (mut a_recv, mut a_send) = split(a_side);
    let (mut d_recv, mut d_send) = split(d_side);
    let (main_local, foreign_local) = (main.local(), foreign.local());
    let (at_main, at_foreign) = tokio::join!(
        accept(
            &mut a_send,
            &mut a_recv,
            &main.device.replica,
            &main_local,
            foreign.keys.endpoint_id
        ),
        dial(
            &mut d_send,
            &mut d_recv,
            &foreign.device.replica,
            &foreign_local,
            main.keys.endpoint_id
        ),
    );
    // Log the errors the way `endpoint.rs` does, plus everything a careless
    // line could add.
    log::info!("sync: handshake failed: {}", at_main.expect_err("refused"));
    log::info!(
        "sync: handshake failed: {}",
        at_foreign.expect_err("refused")
    );
    log::debug!("sync: keys {:?} {:?}", main.keys, foreign.keys);

    let logs = crate::test_log::all();
    for secret in [
        &*main.keys.device_secret,
        &*main.keys.endpoint_secret,
        &*foreign.keys.device_secret,
        &*foreign.keys.endpoint_secret,
        &*main_vault_secret,
        &*foreign_vault_secret,
    ] {
        for form in secret_forms(secret) {
            assert!(!logs.contains(&form), "a log line shows a secret");
        }
    }
}
