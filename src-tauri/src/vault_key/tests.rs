//! Unit tests for the vault_key transport commands.
//!
//! The command bodies do the work; the Tauri wrappers only add
//! `State<AppState>` extraction. Testing the wrappers would require an
//! `App` mock, so the tests here reach directly into `AppState` and
//! exercise the same hex-decode + slot-mutate logic the commands do —
//! wrapped in a tiny local helper so a future rename of the pure logic
//! into a testable helper stays a one-line change here.

use std::sync::{Arc, Mutex};

// Recreate the command bodies against a raw slot handle so we don't need
// a Tauri `AppHandle` in unit tests. Any drift in the real commands must
// be mirrored here — the two are one paragraph each, so this is cheap.
fn set_impl(
    slot: &Arc<Mutex<Option<zeroize::Zeroizing<[u8; 32]>>>>,
    key_hex: &str,
) -> Result<(), String> {
    let bytes: [u8; 32] = hex::decode(key_hex)
        .map_err(|e| format!("vault_key_set: invalid hex: {e}"))?
        .try_into()
        .map_err(|v: Vec<u8>| format!("vault_key_set: expected 32 bytes, got {}", v.len()))?;
    let mut guard = slot
        .lock()
        .map_err(|e| format!("vault_key_set: lock poisoned: {e}"))?;
    *guard = Some(zeroize::Zeroizing::new(bytes));
    Ok(())
}

fn clear_impl(slot: &Arc<Mutex<Option<zeroize::Zeroizing<[u8; 32]>>>>) -> Result<(), String> {
    let mut guard = slot
        .lock()
        .map_err(|e| format!("vault_key_clear: lock poisoned: {e}"))?;
    *guard = None;
    Ok(())
}

fn random_key_bytes() -> [u8; 32] {
    let mut k = [0u8; 32];
    rand::fill(&mut k);
    k
}

fn fresh_slot() -> Arc<Mutex<Option<zeroize::Zeroizing<[u8; 32]>>>> {
    Arc::new(Mutex::new(None))
}

#[test]
fn set_then_clear_leaves_slot_none() {
    let slot = fresh_slot();
    let key = random_key_bytes();
    set_impl(&slot, &hex::encode(key)).expect("set accepts valid 32-byte hex");
    // Verify the stored bytes actually match — sanity check that the
    // hex decode and Zeroizing wrap did not corrupt the buffer.
    {
        let guard = slot.lock().unwrap();
        let stored = guard.as_ref().expect("slot populated after set");
        assert_eq!(**stored, key, "stored key mismatches source");
    }
    clear_impl(&slot).expect("clear always succeeds on a healthy mutex");
    let guard = slot.lock().unwrap();
    assert!(guard.is_none(), "slot must be None after clear");
}

#[test]
fn set_rejects_non_hex() {
    let slot = fresh_slot();
    // 64 chars long (right length for 32 bytes) but not valid hex — this
    // isolates the "invalid character" branch from the "wrong length"
    // branch below.
    let err = set_impl(&slot, &"z".repeat(64)).expect_err("non-hex must fail");
    assert!(
        err.contains("invalid hex"),
        "expected hex-decode failure, got: {err}",
    );
    let guard = slot.lock().unwrap();
    assert!(guard.is_none(), "slot must stay empty on rejected input");
}

#[test]
fn set_rejects_wrong_length() {
    let slot = fresh_slot();
    // 31 bytes (62 hex chars) — decodes fine but wrong length.
    let short = hex::encode([0u8; 31]);
    let err = set_impl(&slot, &short).expect_err("31-byte input must fail");
    assert!(
        err.contains("expected 32 bytes"),
        "expected length error, got: {err}",
    );
    // 33 bytes (66 hex chars) — same branch, from the other side.
    let long = hex::encode([0u8; 33]);
    let err = set_impl(&slot, &long).expect_err("33-byte input must fail");
    assert!(
        err.contains("expected 32 bytes"),
        "expected length error, got: {err}",
    );
    let guard = slot.lock().unwrap();
    assert!(
        guard.is_none(),
        "slot must remain untouched by rejected input",
    );
}

#[test]
fn set_overwrites_previous_key() {
    // Two sequential sets must land the newer key — mirrors the vault-
    // reopen path (close_database clears the slot, but a re-open before
    // clear-through would rely on overwrite semantics).
    let slot = fresh_slot();
    let first = random_key_bytes();
    let second = random_key_bytes();
    assert_ne!(first, second, "random keys collided — reroll");
    set_impl(&slot, &hex::encode(first)).expect("first set");
    set_impl(&slot, &hex::encode(second)).expect("second set");
    let guard = slot.lock().unwrap();
    let stored = guard.as_ref().expect("slot populated");
    assert_eq!(**stored, second, "overwrite must land the newer key");
}
