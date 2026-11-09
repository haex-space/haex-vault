//! The context setting of a model as a row of the vault, against a real temporary vault.

use uuid::Uuid;

use super::{key, read, validate, write, ContextLength, ContextSetting};
use crate::llm::llamacpp::choice::{CacheType, ContextChoice};
use crate::remote_storage::test_support::vault;
use crate::storage::preferences::{self, PrefScope};
use crate::vault_gate::VaultDb;

const MODEL: &str = "qwen3-4b-instruct-q4_k_m";

fn stored(db: &VaultDb, device: Uuid, model: &'static str) -> ContextSetting {
    db.read_blocking(move |q| read(q, device, model)).unwrap()
}

fn put_raw(db: &VaultDb, device: Uuid, value: &'static str) {
    db.write_blocking(move |tx| {
        preferences::insert_or_update(tx, PrefScope::Device(device), &key(MODEL), value)
    })
    .unwrap();
}

fn eight_bit_at(tokens: u32) -> ContextSetting {
    ContextSetting {
        context: ContextLength::Tokens(tokens),
        cache: CacheType::Q8,
    }
}

#[test]
fn without_a_row_the_setting_is_auto_and_standard() {
    let (_dir, db) = vault();
    let setting = stored(&db, db.device_id(), MODEL);
    assert_eq!(setting, ContextSetting::default());
    assert_eq!(setting.choice(), ContextChoice::Auto);
    assert_eq!(setting.cache, CacheType::Standard);
}

#[test]
fn a_setting_is_kept_per_model_and_per_device() {
    let (_dir, db) = vault();
    let device = db.device_id();
    let kept = eight_bit_at(65_536);
    db.write_blocking(move |tx| write(tx, device, MODEL, &kept))
        .unwrap();

    assert_eq!(stored(&db, device, MODEL), kept);
    assert_eq!(kept.choice(), ContextChoice::Fixed(65_536));
    assert_eq!(
        stored(&db, device, "gemma-4-12b-instruct-q4_k_m"),
        ContextSetting::default()
    );
    assert_eq!(
        stored(&db, Uuid::new_v4(), MODEL),
        ContextSetting::default()
    );
}

#[test]
fn choosing_auto_and_the_standard_cache_removes_the_row() {
    let (_dir, db) = vault();
    let device = db.device_id();
    db.write_blocking(move |tx| write(tx, device, MODEL, &eight_bit_at(65_536)))
        .unwrap();
    db.write_blocking(move |tx| write(tx, device, MODEL, &ContextSetting::default()))
        .unwrap();

    let rows = db
        .read_blocking(move |q| preferences::get(q, PrefScope::Device(device), &key(MODEL)))
        .unwrap();
    assert_eq!(rows, None);
}

#[test]
fn a_row_that_cannot_be_used_counts_as_absent() {
    let (_dir, db) = vault();
    let device = db.device_id();
    for broken in [
        "{ not json",
        "[]",
        r#"{"context":"sometimes","cache":"standard"}"#,
        r#"{"context":1024,"cache":"standard"}"#,
        r#"{"context":"auto","cache":"q2"}"#,
    ] {
        put_raw(&db, device, broken);
        assert_eq!(
            stored(&db, device, MODEL),
            ContextSetting::default(),
            "{broken}"
        );
    }
}

#[test]
fn the_row_is_json_with_auto_as_text_and_a_length_as_number() {
    assert_eq!(
        serde_json::to_string(&ContextSetting::default()).unwrap(),
        r#"{"context":"auto","cache":"standard"}"#
    );
    assert_eq!(
        serde_json::to_string(&eight_bit_at(131_072)).unwrap(),
        r#"{"context":131072,"cache":"q8"}"#
    );
}

#[test]
fn a_setting_is_checked_against_the_model_it_is_for() {
    use crate::llm::gguf_header::GgufSummary;
    use crate::llm::llamacpp::choice::PlanError;
    use crate::llm::llamacpp::memory::ModelShape;

    // Qwen3 4B as its file says: trained for 32768.
    let shape = ModelShape::from_header(
        &GgufSummary {
            architecture: "qwen3".to_owned(),
            block_count: Some(36),
            head_count: Some(32),
            head_count_kv: Some(8),
            key_length: Some(128),
            context_length: Some(32_768),
            ..Default::default()
        },
        2_400 << 20,
    )
    .unwrap();
    assert_eq!(validate(Some(&shape), &ContextSetting::default()), Ok(()));
    assert_eq!(validate(Some(&shape), &eight_bit_at(131_072)), Ok(()));
    assert!(matches!(
        validate(Some(&shape), &eight_bit_at(131_073)),
        Err(PlanError::BeyondTraining { .. })
    ));
    assert!(matches!(
        validate(Some(&shape), &eight_bit_at(1024)),
        Err(PlanError::BelowFloor { .. })
    ));
    // A header that cannot be sized takes any length from the floor up.
    assert_eq!(validate(None, &eight_bit_at(500_000)), Ok(()));
    assert!(validate(None, &eight_bit_at(100)).is_err());
}
