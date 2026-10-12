//! The surroundings of the storage tests (spec 038, spec 050): a resolver with fixed answers,
//! placeholder credentials and access, a temporary vault and a `remoteStorage` permission of an
//! installed extension. Re-exported by [`super::test_support`].

use std::collections::HashMap;
use std::io;
use std::net::IpAddr;

use async_trait::async_trait;
use zeroize::Zeroizing;

use super::address::Resolver;
use super::{
    Access, Addressing, Credentials, EndpointScope, Location, Protocol, ProviderKind, S3Credentials,
};

/// Answers names from a fixed table; an unknown name has no address.
pub struct Table(HashMap<String, Vec<IpAddr>>);

/// A resolver that answers `entries` (name, addresses).
pub fn resolver(entries: &[(&str, &[&str])]) -> std::sync::Arc<Table> {
    std::sync::Arc::new(Table(
        entries
            .iter()
            .map(|(name, ips)| {
                (
                    (*name).to_owned(),
                    ips.iter().map(|ip| ip.parse().expect("ip")).collect(),
                )
            })
            .collect(),
    ))
}

#[async_trait]
impl Resolver for Table {
    async fn lookup(&self, host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
        Ok(self.0.get(host).cloned().unwrap_or_default())
    }
}

/// Placeholder credentials; never a real key.
pub fn credentials() -> Credentials {
    Credentials::S3(S3Credentials {
        access_key_id: "AKIDEXAMPLE".to_owned(),
        secret_access_key: Zeroizing::new("placeholder-secret".to_owned()),
        session_token: None,
    })
}

/// Access to `bucket` on a local RustFS of the user.
pub fn access(bucket: &str) -> Access {
    Access {
        location: Location {
            protocol: Protocol::S3,
            provider_kind: ProviderKind::Rustfs,
            endpoint: "http://127.0.0.1:9000".to_owned(),
            endpoint_scope: EndpointScope::Local,
            region: "us-east-1".to_owned(),
            addressing: Addressing::Path,
            bucket: bucket.to_owned(),
            host_key: None,
        },
        credentials: credentials(),
    }
}

/// A fresh temporary vault behind a gate; the directory lives as long as the returned guard.
pub fn vault() -> (tempfile::TempDir, crate::vault_gate::VaultDb) {
    let (dir, db) = crate::passwords::test_support::open_test_vault();
    let db = crate::vault_gate::VaultGate::new()
        .vault_db(std::sync::Arc::new(db))
        .expect("open the gate");
    (dir, db)
}

/// Adds an installed extension `name` with a `remoteStorage` permission for `target`.
pub fn grant_storage(db: &crate::vault_gate::VaultDb, name: &str, target: &str, status: &str) {
    let (name, target, status) = (name.to_owned(), target.to_owned(), status.to_owned());
    db.write_blocking(move |tx| {
        let extension_id = format!("ext-{name}");
        tx.execute(
            "INSERT OR IGNORE INTO extensions (id, public_key, name, installed_at, updated_at) \
             VALUES (?1, 'pk', ?2, 0, 0)",
            haex_crdt::rusqlite::params![extension_id, name],
        )?;
        tx.execute(
            "INSERT INTO extension_permissions \
             (id, extension_id, kind, action, target, status, declared, vault_device_uuid, \
              updated_at) VALUES (?1, ?2, 'remoteStorage', 'read', ?3, ?4, 0, ?5, 0)",
            haex_crdt::rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                extension_id,
                target,
                status,
                crate::identity::VAULT_SCOPE_UUID.to_string()
            ],
        )?;
        Ok(())
    })
    .expect("grant a storage permission");
}
