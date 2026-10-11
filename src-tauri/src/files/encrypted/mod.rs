//! Encrypted folders in storages (spec 048). [`format`] is the pure format `HXEF` v1 (FR-041,
//! FR-042); loading, the tree and the operations on storages build on it.

pub mod cache;
pub mod format;
pub mod redacted;
pub mod vault_keys;

#[cfg(test)]
pub mod test_support;
