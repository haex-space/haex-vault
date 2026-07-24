use super::key_cache::SpaceKeyCache;
use ed25519_dalek::SigningKey;

fn random_key() -> SigningKey {
    let seed: [u8; 32] = rand::random();
    SigningKey::from_bytes(&seed)
}

#[test]
fn empty_cache_returns_none() {
    let cache = SpaceKeyCache::new();
    assert!(cache.get("space_1").is_none());
}

#[test]
fn insert_and_get_roundtrip() {
    let cache = SpaceKeyCache::new();
    let key = random_key();
    cache.insert("space_1", key.clone());
    let out = cache.get("space_1").expect("present");
    assert_eq!(out.to_bytes(), key.to_bytes());
}

#[test]
fn remove_deletes_entry() {
    let cache = SpaceKeyCache::new();
    let key = random_key();
    cache.insert("space_1", key);
    cache.remove("space_1");
    assert!(cache.get("space_1").is_none());
}

#[test]
fn cache_is_send_sync_clone_via_arc() {
    let cache = SpaceKeyCache::new();
    let cache2 = cache.clone();
    let key = random_key();
    cache2.insert("space_1", key.clone());
    // Same inner Arc<RwLock<..>> so the clone sees the write.
    assert!(cache.get("space_1").is_some());
}
