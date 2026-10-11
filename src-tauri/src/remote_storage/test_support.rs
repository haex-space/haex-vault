//! A [`RemoteStore`] in memory for tests (research R10): objects per bucket, an error to answer
//! per operation, and a record of the calls.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::IpAddr;
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::time::Instant;
use zeroize::Zeroizing;

use super::address::Resolver;
use super::{
    Access, Addressing, Credentials, DirListing, EndpointScope, Location, ObjectHead, ObjectInfo,
    ProviderKind, RemoteStore, StorageError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Put,
    Get,
    List,
    Delete,
    Head,
    GetRange,
    ListDir,
    Copy,
    CreateMultipart,
    UploadPart,
    CompleteMultipart,
    AbortMultipart,
}

/// An upload in parts that is not complete yet: bucket, key and its parts by number.
type Upload = (String, String, BTreeMap<u16, Vec<u8>>);

#[derive(Default)]
pub struct FakeStore {
    objects: Mutex<BTreeMap<(String, String), Vec<u8>>>,
    failures: Mutex<HashMap<Op, StorageError>>,
    /// Errors for the next call of an operation only.
    once: Mutex<HashMap<Op, StorageError>>,
    calls: Mutex<Vec<(Op, String)>>,
    uploads: Mutex<HashMap<String, Upload>>,
}

impl FakeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every later call of `op` answers `error`.
    pub fn fail(&self, op: Op, error: StorageError) {
        self.failures.lock().expect("lock").insert(op, error);
    }

    /// Calls of `op` work again.
    pub fn heal(&self, op: Op) {
        self.failures.lock().expect("lock").remove(&op);
    }

    /// The next call of `op` answers `error`, later ones work again.
    pub fn fail_once(&self, op: Op, error: StorageError) {
        self.once.lock().expect("lock").insert(op, error);
    }

    /// The keys stored in `bucket`.
    pub fn keys(&self, bucket: &str) -> Vec<String> {
        self.objects
            .lock()
            .expect("lock")
            .keys()
            .filter(|(b, _)| b == bucket)
            .map(|(_, k)| k.clone())
            .collect()
    }

    /// Stores `body` at `key` in `bucket` without a recorded call.
    pub fn insert(&self, bucket: &str, key: &str, body: &[u8]) {
        self.objects
            .lock()
            .expect("lock")
            .insert((bucket.to_owned(), key.to_owned()), body.to_vec());
    }

    /// The object at `key` in `bucket`.
    pub fn object(&self, bucket: &str, key: &str) -> Option<Vec<u8>> {
        self.objects
            .lock()
            .expect("lock")
            .get(&(bucket.to_owned(), key.to_owned()))
            .cloned()
    }

    /// The keys of uploads in parts that were started and neither completed nor aborted.
    pub fn open_uploads(&self) -> Vec<String> {
        self.uploads
            .lock()
            .expect("lock")
            .values()
            .map(|(_, key, _)| key.clone())
            .collect()
    }

    /// The operations called so far with their key or prefix.
    pub fn calls(&self) -> Vec<(Op, String)> {
        self.calls.lock().expect("lock").clone()
    }

    fn enter(&self, op: Op, key: &str) -> Result<(), StorageError> {
        self.calls.lock().expect("lock").push((op, key.to_owned()));
        if let Some(error) = self.once.lock().expect("lock").remove(&op) {
            return Err(error);
        }
        match self.failures.lock().expect("lock").get(&op) {
            Some(error) => Err(*error),
            None => Ok(()),
        }
    }
}

/// An ETag that changes with the content, as a provider's does.
fn etag(body: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(body);
    format!("\"{}\"", data_encoding::HEXLOWER.encode(&digest[..8]))
}

fn slot(access: &Access, key: &str) -> (String, String) {
    (access.location.bucket.clone(), key.to_owned())
}

#[async_trait]
impl RemoteStore for FakeStore {
    async fn put(
        &self,
        access: &Access,
        key: &str,
        body: Vec<u8>,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::Put, key)?;
        self.objects
            .lock()
            .expect("lock")
            .insert(slot(access, key), body);
        Ok(())
    }

    async fn get(
        &self,
        access: &Access,
        key: &str,
        max_bytes: usize,
        _deadline: Instant,
    ) -> Result<Vec<u8>, StorageError> {
        self.enter(Op::Get, key)?;
        let body = self
            .objects
            .lock()
            .expect("lock")
            .get(&slot(access, key))
            .cloned()
            .ok_or(StorageError::NotFound)?;
        if body.len() > max_bytes {
            return Err(StorageError::TooLarge);
        }
        Ok(body)
    }

    async fn list(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        _deadline: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        self.enter(Op::List, prefix)?;
        let objects: Vec<ObjectInfo> = self
            .objects
            .lock()
            .expect("lock")
            .iter()
            .filter(|((b, k), _)| *b == access.location.bucket && k.starts_with(prefix))
            .map(|((_, k), body)| ObjectInfo {
                key: k.clone(),
                size: body.len() as u64,
                last_modified: "2026-10-06T10:00:00.000Z".to_owned(),
                etag: etag(body),
            })
            .collect();
        if objects.len() > max {
            return Err(StorageError::TooLarge);
        }
        Ok(objects)
    }

    async fn delete(
        &self,
        access: &Access,
        key: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::Delete, key)?;
        self.objects
            .lock()
            .expect("lock")
            .remove(&slot(access, key));
        Ok(())
    }
    async fn head(
        &self,
        access: &Access,
        key: &str,
        _deadline: Instant,
    ) -> Result<ObjectHead, StorageError> {
        self.enter(Op::Head, key)?;
        let (size, etag) = self
            .objects
            .lock()
            .expect("lock")
            .get(&slot(access, key))
            .map(|body| (body.len() as u64, etag(body)))
            .ok_or(StorageError::NotFound)?;
        Ok(ObjectHead {
            size,
            last_modified: Some("Tue, 06 Oct 2026 10:00:00 GMT".to_owned()),
            etag: Some(etag),
        })
    }

    async fn get_range(
        &self,
        access: &Access,
        key: &str,
        start: u64,
        len: u64,
        _deadline: Instant,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, StorageError> {
        self.enter(Op::GetRange, key)?;
        let body = self
            .objects
            .lock()
            .expect("lock")
            .get(&slot(access, key))
            .cloned()
            .ok_or(StorageError::NotFound)?;
        let from = usize::try_from(start).unwrap_or(usize::MAX).min(body.len());
        let to = from
            .saturating_add(usize::try_from(len).unwrap_or(usize::MAX))
            .min(body.len());
        Ok(Box::new(std::io::Cursor::new(body[from..to].to_vec())))
    }

    async fn list_dir(
        &self,
        access: &Access,
        prefix: &str,
        max: usize,
        _deadline: Instant,
    ) -> Result<DirListing, StorageError> {
        self.enter(Op::ListDir, prefix)?;
        let mut listing = DirListing::default();
        for ((bucket, key), body) in self.objects.lock().expect("lock").iter() {
            if *bucket != access.location.bucket || key == prefix {
                continue;
            }
            let Some(rest) = key.strip_prefix(prefix) else {
                continue;
            };
            match rest.find('/') {
                Some(slash) => {
                    let deeper = format!("{prefix}{}", &rest[..=slash]);
                    if listing.prefixes.last() != Some(&deeper) {
                        listing.prefixes.push(deeper);
                    }
                }
                None => listing.objects.push(ObjectInfo {
                    key: key.clone(),
                    size: body.len() as u64,
                    last_modified: "2026-10-06T10:00:00.000Z".to_owned(),
                    etag: etag(body),
                }),
            }
        }
        listing.prefixes.dedup();
        if listing.objects.len() + listing.prefixes.len() > max {
            return Err(StorageError::TooLarge);
        }
        Ok(listing)
    }

    async fn copy(
        &self,
        access: &Access,
        from: &str,
        to: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::Copy, from)?;
        let mut objects = self.objects.lock().expect("lock");
        let body = objects
            .get(&slot(access, from))
            .cloned()
            .ok_or(StorageError::NotFound)?;
        objects.insert(slot(access, to), body);
        Ok(())
    }

    async fn create_multipart(
        &self,
        access: &Access,
        key: &str,
        _deadline: Instant,
    ) -> Result<String, StorageError> {
        self.enter(Op::CreateMultipart, key)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.uploads.lock().expect("lock").insert(
            id.clone(),
            (
                access.location.bucket.clone(),
                key.to_owned(),
                BTreeMap::new(),
            ),
        );
        Ok(id)
    }

    async fn upload_part(
        &self,
        _access: &Access,
        key: &str,
        upload_id: &str,
        number: u16,
        body: Vec<u8>,
        _deadline: Instant,
    ) -> Result<String, StorageError> {
        self.enter(Op::UploadPart, key)?;
        let mut uploads = self.uploads.lock().expect("lock");
        let (_, _, parts) = uploads.get_mut(upload_id).ok_or(StorageError::NotFound)?;
        parts.insert(number, body);
        Ok(format!("\"etag-{number}\""))
    }

    async fn complete_multipart(
        &self,
        _access: &Access,
        key: &str,
        upload_id: &str,
        etags: &[String],
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::CompleteMultipart, key)?;
        let (bucket, key, parts) = self
            .uploads
            .lock()
            .expect("lock")
            .remove(upload_id)
            .ok_or(StorageError::NotFound)?;
        if parts.len() != etags.len() {
            return Err(StorageError::Network);
        }
        let body = parts.into_values().flatten().collect();
        self.objects
            .lock()
            .expect("lock")
            .insert((bucket, key), body);
        Ok(())
    }

    async fn abort_multipart(
        &self,
        _access: &Access,
        key: &str,
        upload_id: &str,
        _deadline: Instant,
    ) -> Result<(), StorageError> {
        self.enter(Op::AbortMultipart, key)?;
        self.uploads.lock().expect("lock").remove(upload_id);
        Ok(())
    }
}

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
    Credentials {
        access_key_id: "AKIDEXAMPLE".to_owned(),
        secret_access_key: Zeroizing::new("placeholder-secret".to_owned()),
        session_token: None,
    }
}

/// Access to `bucket` on a local RustFS of the user.
pub fn access(bucket: &str) -> Access {
    Access {
        location: Location {
            provider_kind: ProviderKind::Rustfs,
            endpoint: "http://127.0.0.1:9000".to_owned(),
            endpoint_scope: EndpointScope::Local,
            region: "us-east-1".to_owned(),
            addressing: Addressing::Path,
            bucket: bucket.to_owned(),
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
