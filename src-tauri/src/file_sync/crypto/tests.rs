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
