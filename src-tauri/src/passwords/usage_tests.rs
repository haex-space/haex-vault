//! Tests for the registry of haex-vault functions that use entries (spec 034, FR-034): the password
//! manager warns before deleting an entry that a function reports as its own.

use std::sync::Arc;

use super::usage::{EntryUsage, UsageRegistry};

struct Fake {
    name: &'static str,
    claims: Vec<&'static str>,
    asked: std::sync::Mutex<Vec<String>>,
}

impl Fake {
    fn new(name: &'static str, claims: &[&'static str]) -> Arc<Self> {
        Arc::new(Self {
            name,
            claims: claims.to_vec(),
            asked: std::sync::Mutex::new(Vec::new()),
        })
    }
}

impl EntryUsage for Fake {
    fn feature(&self) -> &'static str {
        self.name
    }

    fn uses(&self, item_id: &str) -> bool {
        self.asked.lock().expect("lock").push(item_id.to_string());
        self.claims.contains(&item_id)
    }
}

#[test]
fn with_no_provider_nothing_uses_an_entry() {
    assert!(UsageRegistry::new().used_by("any").is_empty());
}

#[test]
fn it_names_exactly_the_providers_that_claim_the_entry_in_registration_order() {
    let registry = UsageRegistry::new();
    registry.register(Fake::new("s3-storage", &["a", "b"]));
    registry.register(Fake::new("backup", &["b"]));
    registry.register(Fake::new("mail", &["c"]));
    assert_eq!(registry.used_by("a"), ["s3-storage"]);
    assert_eq!(registry.used_by("b"), ["s3-storage", "backup"]);
    assert_eq!(registry.used_by("c"), ["mail"]);
    assert!(registry.used_by("d").is_empty());
}

#[test]
fn a_provider_is_asked_only_about_the_entry_given() {
    let registry = UsageRegistry::new();
    let fake = Fake::new("s3-storage", &["a"]);
    registry.register(fake.clone());
    registry.used_by("a");
    assert_eq!(*fake.asked.lock().expect("lock"), ["a"]);
}

#[test]
fn registering_the_same_name_twice_keeps_one() {
    let registry = UsageRegistry::new();
    registry.register(Fake::new("s3-storage", &["a"]));
    registry.register(Fake::new("s3-storage", &["a"]));
    assert_eq!(registry.used_by("a"), ["s3-storage"]);
    // The later registration replaces the earlier one.
    registry.register(Fake::new("s3-storage", &["z"]));
    assert!(registry.used_by("a").is_empty());
    assert_eq!(registry.used_by("z"), ["s3-storage"]);
}
