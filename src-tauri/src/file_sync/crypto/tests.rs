//! Round A verify tests for the file-content envelope + chunk primitives.
//!
//! Covers the three verify criteria from the plan:
//!   1. seal→open roundtrip
//!   2. tamper detection at chunk granularity
//!   3. `ciphertext_len` / `plaintext_len` are exact inverses (property test
//!      over a wide range, plus the "0 bytes / exactly one chunk / one byte
//!      over the chunk boundary" edge cases)

use super::{
    chunk::{
        chunk_nonce, ciphertext_len, num_chunks, open_chunk, plaintext_len, seal_chunk,
        CryptoError, CHUNK_CIPHERTEXT_SIZE, CHUNK_PLAINTEXT_SIZE, TAG_SIZE,
    },
    envelope::{is_envelope, EnvelopeHeader, ENVELOPE_VERSION, HEADER_SIZE, MAGIC, NONCE_SIZE},
};

// Random per-test helpers. CodeQL flags literal seeds/nonces in test code as
// hard-coded credentials even when the material is only used inside the
// process (see CLAUDE.md `Test- & CI-Konventionen`), so every test draws
// fresh bytes from the OS RNG.
fn random_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    rand::fill(&mut k);
    k
}

fn random_nonce() -> [u8; NONCE_SIZE] {
    let mut n = [0u8; NONCE_SIZE];
    rand::fill(&mut n);
    n
}

// ── Envelope header ─────────────────────────────────────────────────

#[test]
fn header_roundtrip_preserves_all_fields() {
    let hdr = EnvelopeHeader::new(0xdead_beef_cafe_babe, random_nonce());
    let bytes = hdr.to_bytes();
    assert_eq!(bytes.len(), HEADER_SIZE);
    assert_eq!(bytes[..4], MAGIC);
    let parsed = EnvelopeHeader::parse(&bytes).expect("valid header parses");
    assert_eq!(parsed, hdr);
}

#[test]
fn header_write_leaves_trailing_bytes_untouched() {
    let hdr = EnvelopeHeader::new(1, random_nonce());
    let mut buf = vec![0xAAu8; HEADER_SIZE + 8];
    hdr.write(&mut buf).unwrap();
    assert!(buf[HEADER_SIZE..].iter().all(|b| *b == 0xAA));
}

#[test]
fn header_rejects_short_input() {
    let short = vec![0u8; HEADER_SIZE - 1];
    assert!(matches!(
        EnvelopeHeader::parse(&short),
        Err(CryptoError::HeaderTooShort)
    ));
}

#[test]
fn header_rejects_bad_magic() {
    let hdr = EnvelopeHeader::new(1, random_nonce());
    let mut bytes = hdr.to_bytes();
    bytes[0] ^= 0xFF;
    assert!(matches!(
        EnvelopeHeader::parse(&bytes),
        Err(CryptoError::BadMagic)
    ));
}

#[test]
fn header_rejects_unknown_version() {
    // Version is the only migration hook; a future writer MUST bump it, and a
    // current reader MUST refuse to touch a future-versioned object.
    let hdr = EnvelopeHeader::new(1, random_nonce());
    let mut bytes = hdr.to_bytes();
    bytes[4] = ENVELOPE_VERSION.wrapping_add(1);
    assert!(matches!(
        EnvelopeHeader::parse(&bytes),
        Err(CryptoError::UnsupportedVersion(v)) if v == ENVELOPE_VERSION.wrapping_add(1)
    ));
    // Version 0 is likewise unknown to v1 readers.
    bytes[4] = 0;
    assert!(matches!(
        EnvelopeHeader::parse(&bytes),
        Err(CryptoError::UnsupportedVersion(0))
    ));
}

#[test]
fn header_write_fails_on_undersized_buffer() {
    let hdr = EnvelopeHeader::new(1, random_nonce());
    let mut too_small = vec![0u8; HEADER_SIZE - 1];
    assert!(matches!(
        hdr.write(&mut too_small),
        Err(CryptoError::BufferTooSmall)
    ));
}

#[test]
fn is_envelope_detects_magic() {
    assert!(is_envelope(&MAGIC));
    assert!(is_envelope(b"HXFE\x00\x01\x02"));
    assert!(!is_envelope(b"HXF"));
    assert!(!is_envelope(b"OTHER"));
    assert!(!is_envelope(b""));
}

// ── Chunk nonce derivation ──────────────────────────────────────────

#[test]
fn chunk_nonces_are_distinct_across_indices() {
    let fnonce = random_nonce();
    let n0 = chunk_nonce(&fnonce, 0);
    let n1 = chunk_nonce(&fnonce, 1);
    let n_max = chunk_nonce(&fnonce, u64::MAX);
    assert_eq!(n0, fnonce, "chunk 0 leaves nonce untouched (XOR with 0)");
    assert_ne!(n0, n1);
    assert_ne!(n1, n_max);
    // Only the last 8 bytes change; the random prefix is preserved.
    assert_eq!(n0[..16], n1[..16]);
    assert_eq!(n0[..16], n_max[..16]);
}

// ── seal / open roundtrip ───────────────────────────────────────────

fn assert_roundtrip(size: usize) {
    let key = random_key();
    let fnonce = random_nonce();
    let plain: Vec<u8> = (0..size).map(|i| (i as u8).wrapping_mul(31)).collect();
    let ct = seal_chunk(&key, &fnonce, 0, &plain).expect("seal");
    assert_eq!(ct.len(), plain.len() + TAG_SIZE);
    let opened = open_chunk(&key, &fnonce, 0, &ct).expect("open");
    assert_eq!(opened, plain);
}

#[test]
fn roundtrip_empty_chunk() {
    // Primitive stays honest even though the file layer never asks for one.
    assert_roundtrip(0);
}

#[test]
fn roundtrip_one_byte() {
    assert_roundtrip(1);
}

#[test]
fn roundtrip_small_payload() {
    assert_roundtrip(4096);
}

#[test]
fn roundtrip_full_chunk() {
    // "Genau ein Chunk" edge case from the plan.
    assert_roundtrip(CHUNK_PLAINTEXT_SIZE);
}

// ── Chunk-index binding + tamper detection ──────────────────────────

#[test]
fn tampering_a_single_byte_fails_open() {
    let key = random_key();
    let fnonce = random_nonce();
    let plain = b"payload that must survive a bit-flip attempt";
    let mut ct = seal_chunk(&key, &fnonce, 0, plain).unwrap();
    // Flip a bit somewhere in the ciphertext body (not the tag suffix — we
    // want to catch that too, but body flips are the interesting case).
    ct[3] ^= 0x01;
    assert!(matches!(
        open_chunk(&key, &fnonce, 0, &ct),
        Err(CryptoError::OpenFailed)
    ));
}

#[test]
fn tampering_the_tag_fails_open() {
    let key = random_key();
    let fnonce = random_nonce();
    let plain = b"payload";
    let mut ct = seal_chunk(&key, &fnonce, 0, plain).unwrap();
    let last = ct.len() - 1;
    ct[last] ^= 0x80;
    assert!(matches!(
        open_chunk(&key, &fnonce, 0, &ct),
        Err(CryptoError::OpenFailed)
    ));
}

#[test]
fn wrong_chunk_index_fails_open() {
    // Chunk-index binding via nonce derivation: swapping indices at open time
    // is indistinguishable from ciphertext corruption.
    let key = random_key();
    let fnonce = random_nonce();
    let ct = seal_chunk(&key, &fnonce, 0, b"payload").unwrap();
    assert!(matches!(
        open_chunk(&key, &fnonce, 1, &ct),
        Err(CryptoError::OpenFailed)
    ));
}

#[test]
fn wrong_key_fails_open() {
    let key = random_key();
    let mut bad_key = random_key();
    bad_key[0] ^= 0x01;
    let fnonce = random_nonce();
    let ct = seal_chunk(&key, &fnonce, 0, b"payload").unwrap();
    assert!(matches!(
        open_chunk(&bad_key, &fnonce, 0, &ct),
        Err(CryptoError::OpenFailed)
    ));
}

#[test]
fn seal_rejects_oversized_chunk() {
    let key = random_key();
    let fnonce = random_nonce();
    let too_big = vec![0u8; CHUNK_PLAINTEXT_SIZE + 1];
    assert!(matches!(
        seal_chunk(&key, &fnonce, 0, &too_big),
        Err(CryptoError::ChunkTooLarge { got }) if got == CHUNK_PLAINTEXT_SIZE + 1
    ));
}

#[test]
fn open_rejects_ciphertext_shorter_than_tag() {
    let key = random_key();
    let fnonce = random_nonce();
    let too_short = vec![0u8; TAG_SIZE - 1];
    assert!(matches!(
        open_chunk(&key, &fnonce, 0, &too_short),
        Err(CryptoError::CiphertextTooShort { .. })
    ));
}

// ── Size arithmetic ─────────────────────────────────────────────────

#[test]
fn num_chunks_edge_cases() {
    assert_eq!(num_chunks(0), 0);
    assert_eq!(num_chunks(1), 1);
    assert_eq!(num_chunks(CHUNK_PLAINTEXT_SIZE as u64 - 1), 1);
    assert_eq!(num_chunks(CHUNK_PLAINTEXT_SIZE as u64), 1);
    assert_eq!(num_chunks(CHUNK_PLAINTEXT_SIZE as u64 + 1), 2);
    assert_eq!(num_chunks(2 * CHUNK_PLAINTEXT_SIZE as u64), 2);
    assert_eq!(num_chunks(2 * CHUNK_PLAINTEXT_SIZE as u64 + 1), 3);
}

#[test]
fn ciphertext_len_matches_seal_for_zero() {
    // 0 bytes → 0 chunks → header only. Confirms the size formula on the
    // pure-primitive side (there is no `seal_file` yet — that comes in Round
    // D — but the formula must already agree with what such a routine would
    // produce).
    assert_eq!(ciphertext_len(0), HEADER_SIZE as u64);
}

#[test]
fn ciphertext_len_matches_seal_for_one_byte() {
    let expected = HEADER_SIZE as u64 + 1 + TAG_SIZE as u64;
    assert_eq!(ciphertext_len(1), expected);
    // And the primitive agrees: one 1-byte chunk gives (plaintext + tag) bytes.
    let ct = seal_chunk(&random_key(), &random_nonce(), 0, &[0xAB]).unwrap();
    assert_eq!(HEADER_SIZE as u64 + ct.len() as u64, expected);
}

#[test]
fn ciphertext_len_matches_seal_for_full_chunk() {
    let expected = HEADER_SIZE as u64 + CHUNK_CIPHERTEXT_SIZE as u64;
    assert_eq!(ciphertext_len(CHUNK_PLAINTEXT_SIZE as u64), expected);
}

#[test]
fn ciphertext_len_matches_seal_for_one_byte_over_chunk() {
    // "Ein Byte über der Chunk-Grenze": 1 full chunk + 1-byte tail chunk.
    let expected = HEADER_SIZE as u64 + CHUNK_CIPHERTEXT_SIZE as u64 + 1 + TAG_SIZE as u64;
    assert_eq!(ciphertext_len(CHUNK_PLAINTEXT_SIZE as u64 + 1), expected);
}

#[test]
fn plaintext_len_rejects_impossibly_short() {
    assert!(matches!(
        plaintext_len(0),
        Err(CryptoError::MalformedCiphertext(_))
    ));
    assert!(matches!(
        plaintext_len(HEADER_SIZE as u64 - 1),
        Err(CryptoError::MalformedCiphertext(_))
    ));
}

#[test]
fn plaintext_len_rejects_zero_plaintext_tail_chunk() {
    // A body that would decode to `full_chunks + 1` chunks where the tail
    // chunk carries zero plaintext bytes is impossible: the file layer never
    // emits such a chunk (num_chunks(len) never leaves a zero-plaintext
    // tail). Any ciphertext-size in that shape is malformed.
    let malformed = HEADER_SIZE as u64 + TAG_SIZE as u64;
    assert!(matches!(
        plaintext_len(malformed),
        Err(CryptoError::MalformedCiphertext(_))
    ));
    let malformed_after_full = HEADER_SIZE as u64 + CHUNK_CIPHERTEXT_SIZE as u64 + TAG_SIZE as u64;
    assert!(matches!(
        plaintext_len(malformed_after_full),
        Err(CryptoError::MalformedCiphertext(_))
    ));
}

// ── Property test: ciphertext_len / plaintext_len are exact inverses ─

#[test]
fn size_arithmetic_is_bijective_across_wide_range() {
    // Load-bearing test — see plan §"Warum der Diff nicht so bricht, wie man
    // erwartet". A wrong answer here is a silent endless re-upload.
    let boundary = CHUNK_PLAINTEXT_SIZE as u64;
    let mut candidates: Vec<u64> = Vec::new();

    // Dense sweep near zero and small values.
    candidates.extend(0..=8192);

    // Windows around each of the first few chunk boundaries.
    for k in 1..=8u64 {
        let center = k * boundary;
        for delta in -8i64..=8 {
            let v = (center as i64 + delta) as u64;
            candidates.push(v);
        }
    }

    // Assorted non-boundary values spanning the multi-chunk regime, chosen so
    // the loop still runs in well under a second in debug.
    for k in [1, 3, 7, 13, 17, 31, 33, 127, 1000, 4096] {
        for m in [0u64, 1, 17, 4096, boundary / 3, boundary / 2, boundary - 1] {
            candidates.push(k * boundary + m);
        }
    }

    for len in candidates {
        let ct = ciphertext_len(len);
        let back = plaintext_len(ct).unwrap_or_else(|e| panic!("plaintext_len({ct}) failed: {e}"));
        assert_eq!(
            back, len,
            "roundtrip failed for plaintext_len={len} (ct={ct})"
        );
    }
}

// ── Key resolver (Round B) ──────────────────────────────────────────

mod key_resolver {
    use std::sync::{Arc, Mutex};

    use base64::Engine;
    use rusqlite::{params, Connection};
    use uuid::Uuid;

    use super::super::key_resolver::{
        clear_key_cache, derive_file_key, resolve_key, resolve_latest, KeyError, KEY_LEN,
    };
    use crate::database::DbConnection;

    // Tests hit the DB directly through rusqlite — they exercise the key
    // resolver's SELECT + cache, not the CRDT execute pipeline. Using
    // `core::execute` here would drag in the CRDT dirty-tracking machinery
    // and require a much larger schema (haex_crdt_configs_no_sync et al.)
    // than what this module actually queries.
    fn setup_db() -> DbConnection {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        conn.execute_batch(
            "CREATE TABLE haex_mls_sync_keys (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                epoch INTEGER NOT NULL,
                key_data TEXT NOT NULL,
                authored_by_did TEXT
            );",
        )
        .expect("create haex_mls_sync_keys");
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    fn with_conn<R>(db: &DbConnection, f: impl FnOnce(&Connection) -> R) -> R {
        let guard = db.0.lock().expect("db lock");
        let conn = guard.as_ref().expect("db open");
        f(conn)
    }

    fn seed_key_row(db: &DbConnection, space_id: &str, epoch: u64, key_data_b64: &str) {
        with_conn(db, |conn| {
            conn.execute(
                "INSERT INTO haex_mls_sync_keys (id, space_id, epoch, key_data) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    space_id,
                    epoch as i64,
                    key_data_b64
                ],
            )
            .expect("insert key row");
        });
    }

    fn seed_key(db: &DbConnection, space_id: &str, epoch: u64, key: &[u8; KEY_LEN]) {
        let b64 = base64::engine::general_purpose::STANDARD.encode(key);
        seed_key_row(db, space_id, epoch, &b64);
    }

    fn delete_key(db: &DbConnection, space_id: &str, epoch: u64) {
        with_conn(db, |conn| {
            conn.execute(
                "DELETE FROM haex_mls_sync_keys WHERE space_id = ?1 AND epoch = ?2",
                params![space_id, epoch as i64],
            )
            .expect("delete key row");
        });
    }

    // Fresh key material and a fresh space_id per test — CodeQL flags literal
    // keys as hard-coded credentials (see CLAUDE.md `Test- & CI-Konventionen`)
    // and fresh IDs isolate the process-wide KEY_CACHE across tests.
    fn random_key() -> [u8; KEY_LEN] {
        let mut k = [0u8; KEY_LEN];
        rand::fill(&mut k);
        k
    }

    fn fresh_space_id() -> String {
        Uuid::new_v4().to_string()
    }

    // KEY_CACHE is process-wide and `clear_key_cache` wipes every entry, so a
    // test that relies on its entry surviving must not run while another test
    // clears the cache. A fresh space_id isolates the *key*, not the flush.
    static CACHE_TESTS: Mutex<()> = Mutex::new(());

    fn lock_cache_tests() -> std::sync::MutexGuard<'static, ()> {
        CACHE_TESTS.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn resolve_key_returns_domain_separated_key() {
        // The resolver hands out BLAKE3(FILE_KEY_CONTEXT, sync_key), never the
        // stored bytes — those are the CRDT sync-payload key (push.ts /
        // pull/apply.ts) and must not double as the file-content AEAD key.
        let db = setup_db();
        let space = fresh_space_id();
        let sync_key = random_key();
        seed_key(&db, &space, 3, &sync_key);
        let got = resolve_key(&space, 3, &db).expect("resolve");
        assert_eq!(got, derive_file_key(&sync_key));
        assert_ne!(got, sync_key, "file key must not equal the sync key");
    }

    #[test]
    fn derive_file_key_is_injective_over_distinct_sync_keys() {
        let a = random_key();
        let mut b = a;
        b[0] ^= 0x01;
        assert_ne!(derive_file_key(&a), derive_file_key(&b));
        assert_eq!(derive_file_key(&a), derive_file_key(&a));
    }

    #[test]
    fn resolve_key_unknown_epoch_errors_cleanly() {
        // Confidentiality guarantee: never fall back to a different epoch.
        // A row exists for epoch 5, but asking for epoch 7 must surface a
        // clean EpochNotFound — not a silently-substituted key.
        let db = setup_db();
        let space = fresh_space_id();
        seed_key(&db, &space, 5, &random_key());
        let err = resolve_key(&space, 7, &db).unwrap_err();
        assert!(
            matches!(&err, KeyError::EpochNotFound { epoch: 7, space_id } if space_id == &space),
            "unexpected: {err}",
        );
    }

    #[test]
    fn resolve_key_unknown_space_errors_cleanly() {
        let db = setup_db();
        let space = fresh_space_id();
        // Seed on a different space so the table isn't empty and we know the
        // WHERE clause is doing the filtering, not "table has no rows".
        seed_key(&db, &fresh_space_id(), 1, &random_key());
        let err = resolve_key(&space, 1, &db).unwrap_err();
        assert!(
            matches!(err, KeyError::EpochNotFound { .. }),
            "unexpected: {err}",
        );
    }

    #[test]
    fn resolve_latest_never_picks_the_db_max_epoch() {
        // Attack shape: `haex_mls_sync_keys` is a membership-system table, so
        // any member holding only Cap::Read can push a row for this space and
        // `owner_column_for` applies no per-row ownership check. If the seal
        // path picked `ORDER BY epoch DESC LIMIT 1`, one row with an absurd
        // epoch would pin every future seal to an attacker-chosen key.
        //
        // The epoch must come from the local MLS group instead. There is no
        // group here, so the resolver has to refuse — not hand back epoch
        // 4611686018427387904.
        let db = setup_db();
        let space = fresh_space_id();
        seed_key(&db, &space, 1, &random_key());
        seed_key(&db, &space, 4_611_686_018_427_387_904, &random_key());
        let err = resolve_latest(&space, &db).unwrap_err();
        assert!(
            matches!(&err, KeyError::MlsEpochUnavailable { space_id, .. } if space_id == &space),
            "seal path must not read the epoch from the DB, got: {err}",
        );
    }

    #[test]
    fn resolve_latest_without_local_group_errors_cleanly() {
        let db = setup_db();
        let space = fresh_space_id();
        let err = resolve_latest(&space, &db).unwrap_err();
        assert!(
            matches!(&err, KeyError::MlsEpochUnavailable { space_id, .. } if space_id == &space),
            "unexpected: {err}",
        );
    }

    #[test]
    fn resolve_key_rejects_conflicting_rows() {
        // No UNIQUE(space_id, epoch) exists and each device mints its own row
        // `id`, so a forged second row for an epoch replicates alongside the
        // honest one. Picking whichever the scan reaches first would be a coin
        // flip between the real key and the attacker's.
        let db = setup_db();
        let space = fresh_space_id();
        seed_key(&db, &space, 9, &random_key());
        seed_key(&db, &space, 9, &random_key());
        let err = resolve_key(&space, 9, &db).unwrap_err();
        assert!(
            matches!(&err, KeyError::AmbiguousKey { epoch: 9, count: 2, space_id } if space_id == &space),
            "unexpected: {err}",
        );
    }

    #[test]
    fn resolve_key_accepts_identical_duplicate_rows() {
        // Two members exporting the same epoch before either has seen the
        // other's row is normal — both derive the same exporter output, so the
        // duplicate is benign and must not be treated as a conflict.
        let db = setup_db();
        let space = fresh_space_id();
        let sync_key = random_key();
        seed_key(&db, &space, 9, &sync_key);
        seed_key(&db, &space, 9, &sync_key);
        let got = resolve_key(&space, 9, &db).expect("benign duplicate");
        assert_eq!(got, derive_file_key(&sync_key));
    }

    #[test]
    fn clear_key_cache_forces_a_db_reread() {
        // Vault close drops the cache (see database::create::close_database),
        // so a key whose row is gone must no longer resolve afterwards.
        let _guard = lock_cache_tests();
        let db = setup_db();
        let space = fresh_space_id();
        let key = random_key();
        seed_key(&db, &space, 11, &key);
        resolve_key(&space, 11, &db).expect("first");
        delete_key(&db, &space, 11);
        clear_key_cache();
        let err = resolve_key(&space, 11, &db).unwrap_err();
        assert!(
            matches!(err, KeyError::EpochNotFound { epoch: 11, .. }),
            "unexpected: {err}",
        );
    }

    #[test]
    fn resolve_key_caches_after_first_lookup() {
        // Cache-hit verify: after a successful lookup, delete the DB row and
        // ask again. The cached value must still be served — otherwise the
        // second call would surface EpochNotFound.
        //
        // This is deliberately *not* a revocation boundary: deleting the row
        // does not evict the key. `clear_key_cache` is the eviction hook, and
        // vault close is the only caller — see
        // `clear_key_cache_forces_a_db_reread`.
        //
        // The fresh UUID space_id isolates this test's cache entry from every
        // other test's, so no explicit KEY_CACHE reset is needed.
        let _guard = lock_cache_tests();
        let db = setup_db();
        let space = fresh_space_id();
        let key = random_key();
        seed_key(&db, &space, 42, &key);
        let first = resolve_key(&space, 42, &db).expect("first");
        assert_eq!(first, derive_file_key(&key));
        delete_key(&db, &space, 42);
        let second = resolve_key(&space, 42, &db).expect("cached second");
        assert_eq!(second, first);
    }

    #[test]
    fn resolve_key_rejects_wrong_length_blob() {
        let db = setup_db();
        let space = fresh_space_id();
        // 16 bytes = 128 bits, not the 256 bits XChaCha20Poly1305 needs.
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        seed_key_row(&db, &space, 1, &short);
        let err = resolve_key(&space, 1, &db).unwrap_err();
        assert!(
            matches!(err, KeyError::InvalidKeyLength { len: 16, .. }),
            "unexpected: {err}",
        );
    }

    #[test]
    fn resolve_key_rejects_invalid_base64() {
        let db = setup_db();
        let space = fresh_space_id();
        seed_key_row(&db, &space, 1, "not!valid!base64!!!");
        let err = resolve_key(&space, 1, &db).unwrap_err();
        assert!(matches!(err, KeyError::Decode { .. }), "unexpected: {err}");
    }
}

// ── Sidecar payload (Round C) ───────────────────────────────────────

mod sidecar {
    use super::super::chunk::{CryptoError, CHUNK_PLAINTEXT_SIZE};
    use super::super::envelope::NONCE_SIZE;
    use super::super::sidecar::{open_sidecar, seal_sidecar, SidecarError, SidecarPayload};

    fn random_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        rand::fill(&mut k);
        k
    }

    fn random_nonce() -> [u8; NONCE_SIZE] {
        let mut n = [0u8; NONCE_SIZE];
        rand::fill(&mut n);
        n
    }

    fn sample_payload() -> SidecarPayload {
        SidecarPayload {
            relative_path: "docs/report.pdf".to_string(),
            size: 123_456,
            modified_at: 1_700_000_000,
            content_type: Some("application/pdf".to_string()),
            blake3: "a".repeat(64),
        }
    }

    #[test]
    fn seal_open_roundtrip_preserves_payload() {
        let key = random_key();
        let ct = seal_sidecar(&key, 7, random_nonce(), &sample_payload()).expect("seal");
        let (header, payload) = open_sidecar(&key, &ct).expect("open");
        assert_eq!(header.epoch, 7);
        assert_eq!(payload, sample_payload());
    }

    #[test]
    fn seal_is_deterministic_for_same_inputs() {
        // Struct field order (not a HashMap) drives serde_json output, so two
        // seals of the same payload/key/nonce/epoch must produce identical
        // bytes — this is what lets the AEAD step be a pure function of its
        // inputs.
        let key = random_key();
        let nonce = random_nonce();
        let a = seal_sidecar(&key, 3, nonce, &sample_payload()).unwrap();
        let b = seal_sidecar(&key, 3, nonce, &sample_payload()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn open_rejects_wrong_key() {
        let key = random_key();
        let mut wrong_key = random_key();
        wrong_key[0] ^= 0x01;
        let ct = seal_sidecar(&key, 1, random_nonce(), &sample_payload()).unwrap();
        assert!(matches!(
            open_sidecar(&wrong_key, &ct),
            Err(SidecarError::Crypto(CryptoError::OpenFailed))
        ));
    }

    #[test]
    fn open_rejects_tampered_ciphertext() {
        let key = random_key();
        let mut ct = seal_sidecar(&key, 1, random_nonce(), &sample_payload()).unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0x80;
        assert!(matches!(
            open_sidecar(&key, &ct),
            Err(SidecarError::Crypto(CryptoError::OpenFailed))
        ));
    }

    #[test]
    fn open_rejects_non_envelope_bytes() {
        let key = random_key();
        // Must be >= HEADER_SIZE (37 bytes) so parsing reaches the magic
        // check instead of short-circuiting on length first.
        let not_an_envelope = vec![0u8; 40];
        assert!(matches!(
            open_sidecar(&key, &not_an_envelope),
            Err(SidecarError::Crypto(CryptoError::BadMagic))
        ));
    }

    #[test]
    fn open_rejects_too_short_bytes() {
        let key = random_key();
        assert!(matches!(
            open_sidecar(&key, b"short"),
            Err(SidecarError::Crypto(CryptoError::HeaderTooShort))
        ));
    }

    #[test]
    fn seal_spans_multiple_chunks_for_oversized_payload() {
        // Sidecars are always small in practice, but the primitive stays
        // honest for a payload that happens to straddle a chunk boundary —
        // e.g. an unusually long content_type or relative_path.
        let key = random_key();
        let mut payload = sample_payload();
        payload.relative_path = "x".repeat(CHUNK_PLAINTEXT_SIZE + 1000);
        let ct = seal_sidecar(&key, 1, random_nonce(), &payload).unwrap();
        let (_, opened) = open_sidecar(&key, &ct).unwrap();
        assert_eq!(opened, payload);
    }
}

// ── Object-key cache + bootstrap (Round C) ──────────────────────────

mod object_key {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex as StdMutex};

    use async_trait::async_trait;
    use base64::Engine;
    use rusqlite::{params, Connection};
    use uuid::Uuid;

    use super::super::key_resolver::derive_file_key;
    use super::super::object_key::{
        bootstrap_object_key_cache, generate_object_key, object_key_known, sidecar_key_for,
        upsert_bootstrap_entry,
    };
    use super::super::sidecar::SidecarPayload;
    use crate::database::DbConnection;
    use crate::remote_storage::backend::StorageBackend;
    use crate::remote_storage::error::StorageError;
    use crate::remote_storage::types::StorageObjectInfo;

    fn random_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        rand::fill(&mut k);
        k
    }

    fn random_nonce() -> [u8; super::super::envelope::NONCE_SIZE] {
        let mut n = [0u8; super::super::envelope::NONCE_SIZE];
        rand::fill(&mut n);
        n
    }

    // ── In-memory DB matching engine::state's test schema, plus
    //    haex_mls_sync_keys for the key resolver and the object_key column
    //    this module's migration adds. ────────────────────────────────────
    fn setup_db() -> DbConnection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        conn.execute_batch(
            "CREATE TABLE haex_crdt_configs_no_sync (
                key TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                value TEXT NOT NULL
            );
            CREATE TABLE haex_crdt_dirty_tables_no_sync (
                table_name TEXT PRIMARY KEY,
                last_modified TEXT
            );
            CREATE TABLE haex_sync_state_no_sync (
                id TEXT PRIMARY KEY NOT NULL,
                rule_id TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                modified_at INTEGER NOT NULL,
                synced_at TEXT NOT NULL,
                deleted INTEGER DEFAULT 0 NOT NULL,
                hash TEXT,
                object_key TEXT
            );
            CREATE UNIQUE INDEX haex_sync_state_rule_path_unique
                ON haex_sync_state_no_sync (rule_id, relative_path);
            CREATE TABLE haex_mls_sync_keys (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                epoch INTEGER NOT NULL,
                key_data TEXT NOT NULL,
                authored_by_did TEXT
            );",
        )
        .expect("schema setup");
        DbConnection(Arc::new(StdMutex::new(Some(conn))))
    }

    fn seed_mls_key(db: &DbConnection, space_id: &str, epoch: u64, key: &[u8; 32]) {
        let guard = db.0.lock().expect("db lock");
        let conn = guard.as_ref().expect("db open");
        conn.execute(
            "INSERT INTO haex_mls_sync_keys (id, space_id, epoch, key_data) VALUES (?1, ?2, ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                space_id,
                epoch as i64,
                base64::engine::general_purpose::STANDARD.encode(key)
            ],
        )
        .expect("seed mls key");
    }

    // ── Fake backend: in-memory key -> bytes map, only `list`/`download`
    //    are exercised by bootstrap. ─────────────────────────────────────
    struct FakeBackend {
        objects: HashMap<String, Vec<u8>>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                objects: HashMap::new(),
            }
        }

        fn put(&mut self, key: &str, bytes: Vec<u8>) {
            self.objects.insert(key.to_string(), bytes);
        }
    }

    #[async_trait]
    impl StorageBackend for FakeBackend {
        fn backend_type(&self) -> &'static str {
            "fake"
        }

        async fn test_connection(&self) -> Result<(), StorageError> {
            Ok(())
        }

        async fn upload(&self, _key: &str, _data: &[u8]) -> Result<(), StorageError> {
            unimplemented!("bootstrap never uploads")
        }

        async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
            self.objects
                .get(key)
                .cloned()
                .ok_or_else(|| StorageError::ObjectNotFound {
                    key: key.to_string(),
                })
        }

        async fn delete(&self, _key: &str) -> Result<(), StorageError> {
            unimplemented!("bootstrap never deletes")
        }

        async fn exists(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self.objects.contains_key(key))
        }

        async fn list(&self, prefix: Option<&str>) -> Result<Vec<StorageObjectInfo>, StorageError> {
            let prefix = prefix.unwrap_or("");
            Ok(self
                .objects
                .keys()
                .filter(|k| k.starts_with(prefix))
                .map(|k| StorageObjectInfo {
                    key: k.clone(),
                    size: self.objects[k].len() as u64,
                    last_modified: None,
                })
                .collect())
        }
        // list_dir / upload_from_path / download_to_path: default impls
        // suffice — bootstrap only ever calls `list` and `download`, and the
        // defaults route through those two (or the `unimplemented!` stubs
        // above) anyway, so there's no need to override them here.
    }

    fn seal_and_put(
        backend: &mut FakeBackend,
        key: &str,
        sync_key: &[u8; 32],
        epoch: u64,
        payload: &SidecarPayload,
    ) {
        // Seal under the *derived* file-content key, matching what
        // `resolve_key` (production code) actually hands back — sealing
        // under the raw sync key would make every bootstrap decrypt fail,
        // since `recover_sidecar` opens with the derived key. See the
        // module-level "Key separation" doc on `key_resolver`.
        let aead_key = derive_file_key(sync_key);
        let ct = super::super::sidecar::seal_sidecar(&aead_key, epoch, random_nonce(), payload)
            .expect("seal sidecar");
        backend.put(&sidecar_key_for(key), ct);
        // Content object itself is opaque to bootstrap — any bytes suffice.
        backend.put(key, vec![0u8; 8]);
    }

    #[test]
    fn generate_object_key_has_expected_shape() {
        let key = generate_object_key();
        assert!(key.starts_with("o/"));
        assert_eq!(key.len(), 2 + 32, "prefix + 32 hex chars for 128 bits");
        assert!(key[2..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_object_key_is_random() {
        let a = generate_object_key();
        let b = generate_object_key();
        assert_ne!(a, b);
    }

    #[test]
    fn sidecar_key_for_appends_suffix() {
        assert_eq!(sidecar_key_for("o/abc"), "o/abc.m");
    }

    #[tokio::test]
    async fn bootstrap_recovers_paired_objects() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 5, &sync_key);

        let mut backend = FakeBackend::new();
        let object_key = "o/deadbeef";
        seal_and_put(
            &mut backend,
            object_key,
            &sync_key,
            5,
            &SidecarPayload {
                relative_path: "notes/todo.md".to_string(),
                size: 42,
                modified_at: 1_700_000_000,
                content_type: Some("text/markdown".to_string()),
                blake3: "b".repeat(64),
            },
        );

        let report = bootstrap_object_key_cache(&backend, "", &space, &rule_id, &db)
            .await
            .expect("bootstrap");

        assert_eq!(report.recovered, 1);
        assert_eq!(report.already_known, 0);
        assert!(report.orphan_content.is_empty());
        assert!(report.orphan_sidecar.is_empty());
        assert!(report.failed_sidecars.is_empty());
        assert!(object_key_known(&db, &rule_id, object_key).unwrap());
    }

    #[tokio::test]
    async fn bootstrap_skips_already_known_object_keys() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 1, &sync_key);

        let mut backend = FakeBackend::new();
        let object_key = "o/already";
        seal_and_put(
            &mut backend,
            object_key,
            &sync_key,
            1,
            &SidecarPayload {
                relative_path: "a.txt".to_string(),
                size: 1,
                modified_at: 1,
                content_type: None,
                blake3: "c".repeat(64),
            },
        );

        // Pre-seed the cache so bootstrap must not re-download the sidecar.
        upsert_bootstrap_entry(&db, &rule_id, "a.txt", object_key, 1, 1, &"c".repeat(64)).unwrap();

        let report = bootstrap_object_key_cache(&backend, "", &space, &rule_id, &db)
            .await
            .expect("bootstrap");

        assert_eq!(report.recovered, 0);
        assert_eq!(report.already_known, 1);
    }

    #[tokio::test]
    async fn bootstrap_reports_orphan_content_without_deleting() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();

        let mut backend = FakeBackend::new();
        backend.put("o/lonely", vec![1, 2, 3]);

        let report = bootstrap_object_key_cache(&backend, "", &space, &rule_id, &db)
            .await
            .expect("bootstrap");

        assert_eq!(report.orphan_content, vec!["o/lonely".to_string()]);
        assert_eq!(report.recovered, 0);
        // Defined action for orphan content is deferred to Round D — the
        // object must still be present, bootstrap never deletes.
        assert!(backend.objects.contains_key("o/lonely"));
    }

    #[tokio::test]
    async fn bootstrap_reports_orphan_sidecar_and_ignores_it() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 1, &sync_key);

        let mut backend = FakeBackend::new();
        let ct = super::super::sidecar::seal_sidecar(
            &derive_file_key(&sync_key),
            1,
            random_nonce(),
            &SidecarPayload {
                relative_path: "orphaned.txt".to_string(),
                size: 0,
                modified_at: 0,
                content_type: None,
                blake3: "d".repeat(64),
            },
        )
        .unwrap();
        backend.put("o/nomatch.m", ct);

        let report = bootstrap_object_key_cache(&backend, "", &space, &rule_id, &db)
            .await
            .expect("bootstrap");

        assert_eq!(report.orphan_sidecar, vec!["o/nomatch.m".to_string()]);
        assert_eq!(report.recovered, 0);
        assert_eq!(report.already_known, 0);
    }

    #[tokio::test]
    async fn bootstrap_records_failure_without_aborting_other_entries() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let good_key_material = random_key();
        // Seed epoch 1 only — the second sidecar is sealed under epoch 9,
        // which has no row, so recovery must fail for it specifically.
        seed_mls_key(&db, &space, 1, &good_key_material);

        let mut backend = FakeBackend::new();
        seal_and_put(
            &mut backend,
            "o/good",
            &good_key_material,
            1,
            &SidecarPayload {
                relative_path: "good.txt".to_string(),
                size: 5,
                modified_at: 5,
                content_type: None,
                blake3: "e".repeat(64),
            },
        );
        seal_and_put(
            &mut backend,
            "o/bad",
            &random_key(),
            9,
            &SidecarPayload {
                relative_path: "bad.txt".to_string(),
                size: 5,
                modified_at: 5,
                content_type: None,
                blake3: "f".repeat(64),
            },
        );

        let report = bootstrap_object_key_cache(&backend, "", &space, &rule_id, &db)
            .await
            .expect("bootstrap must not abort on a single bad sidecar");

        assert_eq!(report.recovered, 1, "the good sidecar still recovers");
        assert_eq!(report.failed_sidecars.len(), 1);
        assert_eq!(report.failed_sidecars[0].0, "o/bad.m");
        assert!(object_key_known(&db, &rule_id, "o/good").unwrap());
        assert!(!object_key_known(&db, &rule_id, "o/bad").unwrap());
    }

    #[tokio::test]
    async fn bootstrap_scopes_list_by_prefix() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 1, &sync_key);

        let mut backend = FakeBackend::new();
        seal_and_put(
            &mut backend,
            "rule-a/o/one",
            &sync_key,
            1,
            &SidecarPayload {
                relative_path: "one.txt".to_string(),
                size: 1,
                modified_at: 1,
                content_type: None,
                blake3: "1".repeat(64),
            },
        );
        // Object under an unrelated prefix must not be visited at all.
        backend.put("rule-b/o/other", vec![9, 9, 9]);

        let report = bootstrap_object_key_cache(&backend, "rule-a/", &space, &rule_id, &db)
            .await
            .expect("bootstrap");

        assert_eq!(report.recovered, 1);
        assert!(report.orphan_content.is_empty());
    }
}

// ── Content sealing/opening (Round D) ───────────────────────────────

mod content {
    use tokio::io::AsyncReadExt;

    use super::super::chunk::{CHUNK_CIPHERTEXT_SIZE, CHUNK_PLAINTEXT_SIZE};
    use super::super::content::{open_bytes, open_stream, seal_bytes, seal_stream};
    use super::super::envelope::{HEADER_SIZE, NONCE_SIZE};

    fn random_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        rand::fill(&mut k);
        k
    }

    fn random_nonce() -> [u8; NONCE_SIZE] {
        let mut n = [0u8; NONCE_SIZE];
        rand::fill(&mut n);
        n
    }

    fn deterministic_plaintext(len: usize) -> Vec<u8> {
        // Non-repeating byte pattern so chunk-boundary bugs surface as a
        // decrypted output that clearly diverges from the input, not as a
        // lucky "all zeros == all zeros" pass.
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(7))
            .collect()
    }

    #[test]
    fn full_buffer_roundtrip_across_chunk_boundaries() {
        // Exact sizes that stress the num_chunks/last_chunk arithmetic —
        // zero, one, small, exactly one chunk, one over, exactly two, and
        // a random-ish "in the middle of chunk 3" length.
        for &len in &[
            0usize,
            1,
            4096,
            CHUNK_PLAINTEXT_SIZE,
            CHUNK_PLAINTEXT_SIZE + 1,
            2 * CHUNK_PLAINTEXT_SIZE,
            2 * CHUNK_PLAINTEXT_SIZE + 17,
        ] {
            let key = random_key();
            let nonce = random_nonce();
            let plain = deterministic_plaintext(len);
            let ct = seal_bytes(&key, 3, nonce, &plain).expect("seal");
            let (hdr, back) = open_bytes(&key, &ct).expect("open");
            assert_eq!(hdr.epoch, 3, "len={len}");
            assert_eq!(back, plain, "roundtrip failed at len={len}");
        }
    }

    #[tokio::test]
    async fn streaming_roundtrip_matches_full_buffer() {
        // seal_stream and seal_bytes must produce byte-identical output
        // for the same inputs — the streaming path is not allowed to
        // silently rearrange chunks or emit extra framing.
        let key = random_key();
        let nonce = random_nonce();
        let plain = deterministic_plaintext(3 * CHUNK_PLAINTEXT_SIZE + 42);

        let expected = seal_bytes(&key, 5, nonce, &plain).expect("seal_bytes");

        let mut src = std::io::Cursor::new(plain.clone());
        let mut dst: Vec<u8> = Vec::new();
        seal_stream(&key, 5, nonce, plain.len() as u64, &mut src, &mut dst)
            .await
            .expect("seal_stream");
        assert_eq!(dst, expected, "seal_stream diverged from seal_bytes");

        let mut ct_reader = std::io::Cursor::new(expected.clone());
        let mut pt_writer: Vec<u8> = Vec::new();
        let hdr = open_stream(&key, expected.len() as u64, &mut ct_reader, &mut pt_writer)
            .await
            .expect("open_stream");
        assert_eq!(hdr.epoch, 5);
        assert_eq!(pt_writer, plain, "open_stream did not recover the input");
    }

    #[tokio::test]
    async fn streaming_rejects_short_reader() {
        // Announcing a plaintext_len larger than the reader can supply
        // must fail cleanly, not silently truncate: a growing/shrinking
        // source file would otherwise produce a valid-looking envelope
        // that decrypts to the wrong bytes.
        let key = random_key();
        let nonce = random_nonce();
        let short = deterministic_plaintext(100);
        let mut src = std::io::Cursor::new(short.clone());
        let mut dst: Vec<u8> = Vec::new();
        let err = seal_stream(&key, 1, nonce, 200, &mut src, &mut dst)
            .await
            .expect_err("must fail on truncated source");
        assert!(
            format!("{err}").contains("io error"),
            "expected io error, got: {err}",
        );
    }

    #[tokio::test]
    async fn streaming_open_matches_ciphertext_len_arithmetic() {
        // The ciphertext_len an open_stream caller must announce equals
        // HEADER + Σ (chunk_pt + TAG). If the arithmetic is off by one
        // AEAD tag the last chunk decrypts fine but body_remaining lands
        // on a value ≤ TAG_SIZE, which open_stream must reject.
        let key = random_key();
        let nonce = random_nonce();
        let plain = deterministic_plaintext(CHUNK_PLAINTEXT_SIZE + 1);
        let ct = seal_bytes(&key, 1, nonce, &plain).unwrap();

        let true_len = ct.len() as u64;
        assert_eq!(
            true_len,
            HEADER_SIZE as u64
                + CHUNK_CIPHERTEXT_SIZE as u64
                + 1
                + super::super::chunk::TAG_SIZE as u64,
            "sanity check on the size formula for len=CHUNK+1",
        );

        // Correct length round-trips.
        let mut r = std::io::Cursor::new(ct.clone());
        let mut w: Vec<u8> = Vec::new();
        open_stream(&key, true_len, &mut r, &mut w).await.unwrap();
        assert_eq!(w.len(), plain.len());

        // Overstated length forces open_stream to think there is one
        // more chunk than really exists — the extra read must fail.
        let mut r_bad = std::io::Cursor::new(ct);
        let mut w_bad: Vec<u8> = Vec::new();
        let err = open_stream(&key, true_len + 100, &mut r_bad, &mut w_bad)
            .await
            .expect_err("overstated len must fail");
        // Either the extra read fails EOF, or the trailing "body" runs
        // out too small for a chunk — both are correct rejections.
        let _ = err;
    }

    #[tokio::test]
    async fn streaming_writer_stays_within_one_chunk_in_ram() {
        // Regression: seal_stream must never buffer more than one
        // plaintext chunk. Enforced structurally — a read_exact against
        // a `[u8; CHUNK_PLAINTEXT_SIZE]` buffer, one chunk at a time —
        // rather than by measuring RSS, but the test still asserts that
        // a 4-chunk file goes through with a bounded, single-chunk read
        // burst per iteration by hooking the reader.
        struct Counting<R: AsyncReadExt + Unpin> {
            inner: R,
            max_single_read: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }

        impl<R: AsyncReadExt + Unpin> tokio::io::AsyncRead for Counting<R> {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                let this = self.get_mut();
                let before = buf.filled().len();
                let pinned = std::pin::Pin::new(&mut this.inner);
                let poll = pinned.poll_read(cx, buf);
                let after = buf.filled().len();
                let delta = after - before;
                let mut cur = this
                    .max_single_read
                    .load(std::sync::atomic::Ordering::Relaxed);
                while delta > cur {
                    match this.max_single_read.compare_exchange(
                        cur,
                        delta,
                        std::sync::atomic::Ordering::Relaxed,
                        std::sync::atomic::Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(actual) => cur = actual,
                    }
                }
                poll
            }
        }

        let key = random_key();
        let nonce = random_nonce();
        let plain = deterministic_plaintext(4 * CHUNK_PLAINTEXT_SIZE);
        let src_inner = std::io::Cursor::new(plain.clone());
        let max_single = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut counting = Counting {
            inner: src_inner,
            max_single_read: max_single.clone(),
        };
        let mut dst: Vec<u8> = Vec::new();
        seal_stream(&key, 9, nonce, plain.len() as u64, &mut counting, &mut dst)
            .await
            .expect("seal_stream");

        assert!(
            max_single.load(std::sync::atomic::Ordering::Relaxed) <= CHUNK_PLAINTEXT_SIZE,
            "single read exceeded CHUNK_PLAINTEXT_SIZE — streaming lost its RAM bound",
        );
    }
}

// ── Encrypting SyncProvider decorator (Round D) ─────────────────────

mod provider {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex as StdMutex};

    use async_trait::async_trait;
    use base64::Engine;
    use rusqlite::{params, Connection};
    use uuid::Uuid;

    use super::super::key_resolver::derive_file_key;
    use super::super::provider::{EncryptingSyncProvider, FileKeySource};
    use super::super::sidecar::{seal_sidecar, SidecarPayload};
    use crate::database::DbConnection;
    use crate::file_sync::provider::{ReadFileResult, SyncProvider, SyncProviderError};
    use crate::file_sync::types::FileState;

    fn random_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        rand::fill(&mut k);
        k
    }

    fn random_nonce() -> [u8; super::super::envelope::NONCE_SIZE] {
        let mut n = [0u8; super::super::envelope::NONCE_SIZE];
        rand::fill(&mut n);
        n
    }

    fn setup_db() -> DbConnection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        conn.execute_batch(
            "CREATE TABLE haex_crdt_configs_no_sync (
                key TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                value TEXT NOT NULL
            );
            CREATE TABLE haex_crdt_dirty_tables_no_sync (
                table_name TEXT PRIMARY KEY,
                last_modified TEXT
            );
            CREATE TABLE haex_sync_state_no_sync (
                id TEXT PRIMARY KEY NOT NULL,
                rule_id TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                modified_at INTEGER NOT NULL,
                synced_at TEXT NOT NULL,
                deleted INTEGER DEFAULT 0 NOT NULL,
                hash TEXT,
                object_key TEXT
            );
            CREATE UNIQUE INDEX haex_sync_state_rule_path_unique
                ON haex_sync_state_no_sync (rule_id, relative_path);
            CREATE TABLE haex_mls_sync_keys (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                epoch INTEGER NOT NULL,
                key_data TEXT NOT NULL,
                authored_by_did TEXT
            );",
        )
        .expect("schema setup");
        DbConnection(Arc::new(StdMutex::new(Some(conn))))
    }

    fn seed_mls_key(db: &DbConnection, space_id: &str, epoch: u64, key: &[u8; 32]) {
        let guard = db.0.lock().expect("db lock");
        let conn = guard.as_ref().expect("db open");
        conn.execute(
            "INSERT INTO haex_mls_sync_keys (id, space_id, epoch, key_data) VALUES (?1, ?2, ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                space_id,
                epoch as i64,
                base64::engine::general_purpose::STANDARD.encode(key)
            ],
        )
        .expect("seed mls key");
    }

    fn seed_current_epoch(_db: &DbConnection, _space_id: &str, _epoch: u64) {
        // The decorator's write path resolves the current epoch via
        // `MlsManager::current_epoch`, which reads a live MLS group.
        // These tests use a lower-level path (see `WriteFixture` below)
        // that bypasses `resolve_latest` in favour of `resolve_key`, so
        // no MLS group is stood up here.
    }

    /// In-memory `SyncProvider` used to observe what the decorator
    /// actually calls on the inner backend. Each key is stored verbatim
    /// so the tests can assert on the opaque `o/…` and `o/….m` keys the
    /// decorator produces.
    #[derive(Default)]
    struct FakeProvider {
        objects: StdMutex<HashMap<String, Vec<u8>>>,
    }

    impl FakeProvider {
        fn snapshot_keys(&self) -> Vec<String> {
            let mut keys: Vec<String> = self.objects.lock().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        }
    }

    #[async_trait]
    impl SyncProvider for FakeProvider {
        fn display_name(&self) -> String {
            "fake".into()
        }

        async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
            Ok(self
                .objects
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| FileState {
                    relative_path: k.clone(),
                    size: v.len() as u64,
                    modified_at: 0,
                    is_directory: false,
                    hash: None,
                    chunk_size: None,
                    chunk_hashes: None,
                })
                .collect())
        }

        async fn read_file(&self, key: &str) -> Result<Vec<u8>, SyncProviderError> {
            self.objects
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .ok_or_else(|| SyncProviderError::NotFound {
                    path: key.to_string(),
                })
        }

        async fn read_file_to_path(
            &self,
            key: &str,
            output_path: &std::path::Path,
            _expected_chunks: Option<crate::file_sync::hashing::ChunkedHash>,
            on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
        ) -> Result<ReadFileResult, SyncProviderError> {
            let data = self.read_file(key).await?;
            tokio::fs::write(output_path, &data)
                .await
                .map_err(SyncProviderError::Io)?;
            let n = data.len() as u64;
            on_progress(n, n);
            Ok(ReadFileResult {
                bytes: n,
                hash: None,
            })
        }

        async fn write_file(&self, key: &str, data: &[u8]) -> Result<(), SyncProviderError> {
            self.objects
                .lock()
                .unwrap()
                .insert(key.to_string(), data.to_vec());
            Ok(())
        }

        async fn write_file_from_path(
            &self,
            key: &str,
            source_path: &std::path::Path,
        ) -> Result<(), SyncProviderError> {
            let data = tokio::fs::read(source_path)
                .await
                .map_err(SyncProviderError::Io)?;
            self.write_file(key, &data).await
        }

        async fn delete_file(&self, key: &str, _to_trash: bool) -> Result<(), SyncProviderError> {
            self.objects
                .lock()
                .unwrap()
                .remove(key)
                .map(|_| ())
                .ok_or_else(|| SyncProviderError::NotFound {
                    path: key.to_string(),
                })
        }

        async fn create_directory(&self, _key: &str) -> Result<(), SyncProviderError> {
            Ok(())
        }

        fn supports_directories(&self) -> bool {
            false
        }
    }

    /// Helper: seed a paired (content, sidecar) object under a fresh
    /// `o/<hex>` key, sealed under `epoch`. Returns the object key.
    async fn seed_paired_object(
        inner: &FakeProvider,
        sync_key: &[u8; 32],
        epoch: u64,
        relative_path: &str,
        plaintext: &[u8],
    ) -> String {
        let object_key = format!("o/{}", Uuid::new_v4().simple());
        let aead_key = derive_file_key(sync_key);
        let content_ct =
            super::super::content::seal_bytes(&aead_key, epoch, random_nonce(), plaintext)
                .expect("seal content");
        let payload = SidecarPayload {
            relative_path: relative_path.to_string(),
            size: plaintext.len() as u64,
            modified_at: 1_700_000_000,
            content_type: None,
            blake3: blake3::hash(plaintext).to_hex().to_string(),
        };
        let sidecar_ct =
            seal_sidecar(&aead_key, epoch, random_nonce(), &payload).expect("seal sidecar");
        inner
            .write_file(&object_key, &content_ct)
            .await
            .expect("put content");
        inner
            .write_file(
                &super::super::object_key::sidecar_key_for(&object_key),
                &sidecar_ct,
            )
            .await
            .expect("put sidecar");
        object_key
    }

    #[tokio::test]
    async fn manifest_bootstraps_and_returns_plaintext_view() {
        // A paired (content, sidecar) object landed in the bucket by
        // another device: after the first `manifest()` the decorator's
        // local cache must know it, and the returned FileState must
        // carry the plaintext size — not the (larger) ciphertext size.
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 1, &sync_key);
        seed_current_epoch(&db, &space, 1);

        let inner = Arc::new(FakeProvider::default());
        let plaintext = b"hello from a peer".to_vec();
        seed_paired_object(&inner, &sync_key, 1, "docs/note.md", &plaintext).await;

        let dec = EncryptingSyncProvider::new(
            inner.clone(),
            FileKeySource::SpaceEpoch {
                space_id: space.clone(),
            },
            rule_id.clone(),
            DbConnection(db.0.clone()),
        );

        let manifest = dec.manifest().await.expect("manifest");
        assert_eq!(manifest.len(), 1);
        assert_eq!(manifest[0].relative_path, "docs/note.md");
        assert_eq!(
            manifest[0].size,
            plaintext.len() as u64,
            "manifest must report plaintext size, not ciphertext size — \
             otherwise unchanged files re-upload forever",
        );
    }

    #[tokio::test]
    async fn unchanged_file_produces_no_diff_action() {
        // THE regression test of Phase 4 (see plan §"Warum der Diff nicht
        // so bricht"): after `manifest()` twice on the same corpus, the
        // diff engine must see identical FileStates so `files_equal`
        // yields "no action".
        use crate::file_sync::diff::compute_sync_actions;
        use crate::file_sync::types::{DeleteMode, SyncDirection};

        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 1, &sync_key);
        seed_current_epoch(&db, &space, 1);

        let inner = Arc::new(FakeProvider::default());
        seed_paired_object(&inner, &sync_key, 1, "a.bin", b"content").await;

        let dec = EncryptingSyncProvider::new(
            inner,
            FileKeySource::SpaceEpoch { space_id: space },
            rule_id,
            DbConnection(db.0.clone()),
        );

        let first = dec.manifest().await.expect("m1");
        let second = dec.manifest().await.expect("m2");
        let actions =
            compute_sync_actions(&first, &second, SyncDirection::OneWay, DeleteMode::Ignore);
        assert!(
            actions.to_download.is_empty()
                && actions.to_upload.is_empty()
                && actions.to_delete.is_empty(),
            "unchanged bucket must yield zero diff actions, got: {actions:?}",
        );
    }

    #[tokio::test]
    async fn read_file_roundtrips_seeded_content() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 1, &sync_key);
        seed_current_epoch(&db, &space, 1);

        let inner = Arc::new(FakeProvider::default());
        let plaintext = b"round-trip me".to_vec();
        seed_paired_object(&inner, &sync_key, 1, "hello.txt", &plaintext).await;

        let dec = EncryptingSyncProvider::new(
            inner,
            FileKeySource::SpaceEpoch { space_id: space },
            rule_id,
            DbConnection(db.0.clone()),
        );
        // manifest must run first so the cache learns the object key.
        dec.manifest().await.expect("bootstrap");

        let got = dec.read_file("hello.txt").await.expect("read");
        assert_eq!(got, plaintext);
    }

    #[tokio::test]
    async fn read_file_streaming_matches_full_buffer() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 1, &sync_key);

        let inner = Arc::new(FakeProvider::default());
        // Multi-chunk plaintext so the streaming reassembly is
        // exercised, not just a single-chunk trivial case.
        let plaintext: Vec<u8> = (0..(super::super::chunk::CHUNK_PLAINTEXT_SIZE + 137))
            .map(|i| (i as u8).wrapping_mul(11))
            .collect();
        seed_paired_object(&inner, &sync_key, 1, "big.bin", &plaintext).await;

        let dec = EncryptingSyncProvider::new(
            inner,
            FileKeySource::SpaceEpoch { space_id: space },
            rule_id,
            DbConnection(db.0.clone()),
        );
        dec.manifest().await.expect("bootstrap");

        let tmp_dir = tempfile::tempdir().expect("tempdir");
        let out_path = tmp_dir.path().join("out.bin");
        let progress: Arc<dyn Fn(u64, u64) + Send + Sync> = Arc::new(|_, _| {});
        let info = dec
            .read_file_to_path("big.bin", &out_path, None, progress)
            .await
            .expect("stream read");
        assert_eq!(info.bytes as usize, plaintext.len());
        let got = tokio::fs::read(&out_path).await.expect("read out");
        assert_eq!(got, plaintext);
    }

    #[tokio::test]
    async fn delete_removes_both_content_and_sidecar() {
        let db = setup_db();
        let space = Uuid::new_v4().to_string();
        let rule_id = Uuid::new_v4().to_string();
        let sync_key = random_key();
        seed_mls_key(&db, &space, 1, &sync_key);

        let inner = Arc::new(FakeProvider::default());
        let object_key = seed_paired_object(&inner, &sync_key, 1, "gone.txt", b"bye").await;

        let dec = EncryptingSyncProvider::new(
            inner.clone(),
            FileKeySource::SpaceEpoch { space_id: space },
            rule_id,
            DbConnection(db.0.clone()),
        );
        dec.manifest().await.expect("bootstrap");
        dec.delete_file("gone.txt", false).await.expect("delete");

        let remaining = inner.snapshot_keys();
        assert!(
            !remaining.iter().any(|k| k == &object_key),
            "content object still present after delete: {remaining:?}",
        );
        assert!(
            !remaining
                .iter()
                .any(|k| k == &super::super::object_key::sidecar_key_for(&object_key)),
            "sidecar object still present after delete: {remaining:?}",
        );
    }

    #[tokio::test]
    async fn own_vault_key_source_errors_cleanly() {
        // Placeholder branch — Rust holds no vault key today, so calling
        // any content path with `VaultKey` must surface a clear error
        // rather than deadlock or panic.
        //
        // Use `write_file` here, not `read_file`: `read_file` calls
        // `object_key_for_read` first and fails with `MissingObjectKey`
        // before touching key resolution, which masks the VaultKey path.
        // `write_file` calls `seal_key()` up front, so `OwnVaultNotWired`
        // is the actual error surfaced.
        let db = setup_db();
        let inner = Arc::new(FakeProvider::default());
        let dec = EncryptingSyncProvider::new(
            inner,
            FileKeySource::VaultKey,
            "rule".to_string(),
            DbConnection(db.0.clone()),
        );
        let err = dec
            .write_file("anything", b"payload")
            .await
            .expect_err("must fail");
        assert!(
            format!("{err}").contains("own-vault"),
            "expected the own-vault not-wired error, got: {err}",
        );
    }
}
