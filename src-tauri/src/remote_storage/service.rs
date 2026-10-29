//! What the settings do with connections and storages (spec 038 US1, spec 050 US1,
//! contracts/tauri-commands.md):
//! list, save after a passed test, test, preview a removal and remove. The Tauri commands in
//! [`super::commands`] only call this; extensions reach storages through [`StorageService::access_of`]
//! (PR D).
//!
//! A new connection, new credentials and a changed endpoint, region or addressing are tested
//! before anything is saved (FR-003); a failed test saves nothing, also no credentials (SC-004).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use uuid::Uuid;

use super::address::{self, AddressError, Resolver};
use super::model::{
    ConnectionInput, ConnectionView, CredentialsInput, LastTest, RemovalPreview, RemovalTarget,
    StorageInput, StorageOverview, StorageView, StorageWarning, TestResult,
};
use super::probe::probe;
use super::sftp::keys;
use super::{
    credentials, store, Access, ConnectionRow, CredentialsState, EndpointScope, Location,
    ObjectStore, Protocol, StorageRow, TestOutcome,
};
use crate::error::{HolziError, Result};
use crate::passwords::clock;
use crate::passwords::service::PasswordsService;
use crate::vault_gate::VaultDb;

/// How long holzi waits for the addresses of an endpoint when it is set.
const RESOLVE_TIME: Duration = Duration::from_secs(10);

/// Storage connections over the open vault and a provider.
pub struct StorageService {
    db: VaultDb,
    passwords: PasswordsService,
    store: Arc<dyn ObjectStore>,
    resolver: Arc<dyn Resolver>,
}

/// Builds a validation error identifying the rejected input field.
fn invalid(field: &str) -> HolziError {
    HolziError::StorageInvalid {
        field: field.to_owned(),
    }
}

/// A test that did not get as far as the provider: its address is not known.
fn unreachable() -> HolziError {
    HolziError::StorageTestFailed {
        outcome: TestOutcome::Unreachable,
        leftover_key: None,
        host_key: None,
    }
}

/// Accepts a passed probe or returns its failure outcome and possible leftover object key.
fn failed(result: super::probe::ProbeResult) -> Result<()> {
    match result.outcome {
        TestOutcome::Passed => Ok(()),
        outcome => Err(HolziError::StorageTestFailed {
            outcome,
            leftover_key: result.leftover_key,
            host_key: result.host_key,
        }),
    }
}

impl StorageService {
    /// A storage service over the vault, its password manager, a provider and a resolver.
    pub fn new(
        db: VaultDb,
        passwords: PasswordsService,
        store: Arc<dyn ObjectStore>,
        resolver: Arc<dyn Resolver>,
    ) -> Self {
        Self {
            db,
            passwords,
            store,
            resolver,
        }
    }

    /// The scope of the endpoint `url` as it is set now (research R8): an address holzi never
    /// reaches, a mix of local and public ones or `http` to a public host is an invalid endpoint, a
    /// name without addresses an unreachable one.
    async fn scope_of(&self, url: &Url) -> Result<EndpointScope> {
        match tokio::time::timeout(RESOLVE_TIME, address::scope_of(url, self.resolver.as_ref()))
            .await
        {
            Ok(Ok(scope)) => Ok(scope),
            Ok(Err(AddressError::Unresolved)) | Err(_) => Err(unreachable()),
            Ok(Err(AddressError::Invalid | AddressError::NotAllowed)) => Err(invalid("endpoint")),
        }
    }

    /// Every connection and storage with the state of its credentials and its last test here.
    pub async fn overview(&self) -> Result<StorageOverview> {
        self.db
            .read(|q| {
                let tests = store::tests(q)?;
                let mut connections = Vec::new();
                for row in store::connections(q)? {
                    let state = credentials::state_in(q, &row.credentials_item_id, row.protocol)?;
                    let entry = credentials::entry(q, &row.credentials_item_id)?;
                    connections.push(view(&row, state, entry));
                }
                let mut storages = Vec::new();
                for row in store::storages(q)? {
                    storages.push(StorageView {
                        extensions: store::extensions_of(q, &row.id)?,
                        last_test: tests.get(&row.id).map(|(at, outcome)| LastTest {
                            at: at.clone(),
                            outcome: *outcome,
                        }),
                        id: row.id,
                        connection_id: row.connection_id,
                        name: row.name,
                        bucket: row.bucket,
                        warning: None,
                    });
                }
                Ok(StorageOverview {
                    connections,
                    storages,
                })
            })
            .await
    }

    /// Loads a connection by ID, returning `StorageNotFound` when it is absent.
    pub async fn connection(&self, id: &str) -> Result<ConnectionRow> {
        let id = id.to_owned();
        self.db
            .read(move |q| Ok(store::connection(q, &id)?))
            .await?
            .ok_or(HolziError::StorageNotFound)
    }

    /// Loads a storage by ID, returning `StorageNotFound` when it is absent.
    pub async fn storage(&self, id: &str) -> Result<StorageRow> {
        let id = id.to_owned();
        self.db
            .read(move |q| Ok(store::storage(q, &id)?))
            .await?
            .ok_or(HolziError::StorageNotFound)
    }

    /// What a call needs to reach the storage `id`: its location and the connection's
    /// credentials, or [`HolziError::StorageCredentialsUnavailable`].
    pub async fn access_of(&self, storage_id: &str) -> Result<Access> {
        let storage = self.storage(storage_id).await?;
        let connection = self.connection(&storage.connection_id).await?;
        let credentials = credentials::read(
            &self.passwords,
            &self.db,
            &connection.credentials_item_id,
            connection.protocol,
        )
        .await?;
        Ok(Access {
            location: Location::of(&connection, &storage.bucket),
            credentials,
        })
    }

    /// Saves a connection of the user (see the module). A new or changed endpoint gets its scope
    /// anew (research R8); the user may set a local one.
    pub async fn save_connection(&self, input: ConnectionInput) -> Result<ConnectionView> {
        self.save_connection_in(input, None).await
    }

    /// A proposal's new connection, only while its endpoint lies in the `scope` the user confirmed:
    /// one that resolves elsewhere since (DNS rebinding) is invalid before any test (research R8).
    pub async fn save_proposed_connection(
        &self,
        input: ConnectionInput,
        scope: EndpointScope,
    ) -> Result<ConnectionView> {
        self.save_connection_in(input, Some(scope)).await
    }

    async fn save_connection_in(
        &self,
        input: ConnectionInput,
        confirmed_scope: Option<EndpointScope>,
    ) -> Result<ConnectionView> {
        let mut endpoint = input.endpoint.as_deref().unwrap_or("").trim().to_owned();
        // A WebDAV address is a folder; with its `/` no server redirects (spec 050 research R2).
        if input.protocol == Protocol::WebDav && !endpoint.is_empty() && !endpoint.ends_with('/') {
            endpoint.push('/');
        }
        let existing = match &input.id {
            Some(id) => Some(self.connection(id).await?),
            None => None,
        };
        // The protocol of a saved connection never changes (spec 050 FR-001).
        if existing
            .as_ref()
            .is_some_and(|e| e.protocol != input.protocol)
        {
            return Err(invalid("protocol"));
        }
        let now = clock::now();
        let mut row = ConnectionRow {
            id: input
                .id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            protocol: input.protocol,
            provider_name: input.provider_name.trim().to_owned(),
            provider_kind: input.provider_kind,
            endpoint,
            endpoint_scope: existing
                .as_ref()
                .map_or(EndpointScope::Public, |e| e.endpoint_scope),
            region: input.region.trim().to_owned(),
            addressing: input.addressing,
            host_key: existing.as_ref().and_then(|e| e.host_key.clone()),
            host_key_confirmed_at: existing
                .as_ref()
                .and_then(|e| e.host_key_confirmed_at.clone()),
            credentials_item_id: existing
                .as_ref()
                .map(|e| e.credentials_item_id.clone())
                .unwrap_or_else(|| "pending".to_owned()),
            created_at: existing
                .as_ref()
                .map(|e| e.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        // The host key the user confirmed (SFTP, research R5) is the one accepted from now on.
        if let Some(line) = input.host_key.as_deref() {
            if row.protocol != Protocol::Sftp {
                return Err(invalid("hostKey"));
            }
            let key = keys::parse_host_key(line).ok_or_else(|| invalid("hostKey"))?;
            let line = keys::host_key_line(&key);
            if row.host_key.as_deref() != Some(line.as_str()) {
                row.host_key = Some(line);
                row.host_key_confirmed_at = Some(row.updated_at.clone());
            }
        }
        store::check_draft(&row)?;
        let url = address::endpoint_url(&Location::of(&row, &input.bucket_for_test))
            .map_err(|_| invalid("endpoint"))?;

        let generated = match &input.credentials {
            Some(CredentialsInput::GeneratedKey { key_id, .. }) => Some(key_id.clone()),
            _ => None,
        };
        let new_credentials = match input.credentials {
            Some(credentials) => Some(self.resolve(credentials, row.protocol).await?),
            None => None,
        };
        let moved = existing.as_ref().is_none_or(|e| {
            e.endpoint != row.endpoint
                || e.region != row.region
                || e.addressing != row.addressing
                || e.host_key != row.host_key
        });
        let test_with = match (&new_credentials, &existing) {
            (Some(resolved), _) => Some(resolved.credentials().clone()),
            (None, None) => return Err(invalid("credentials")),
            (None, Some(e)) if moved => Some(
                credentials::read(
                    &self.passwords,
                    &self.db,
                    &e.credentials_item_id,
                    e.protocol,
                )
                .await?,
            ),
            (None, Some(_)) => None,
        };
        if test_with.is_some() && !store::target_ok(row.protocol, &input.bucket_for_test) {
            return Err(invalid(store::target_field(row.protocol)));
        }
        if moved {
            row.endpoint_scope = self.scope_of(&url).await?;
        }
        if confirmed_scope.is_some_and(|scope| scope != row.endpoint_scope) {
            return Err(invalid("endpoint"));
        }
        if let Some(credentials) = test_with {
            let access = Access {
                location: Location::of(&row, &input.bucket_for_test),
                credentials,
            };
            let result = self.probe_creating(&access, input.create_folder).await?;
            self.record_for_bucket(&row.id, &input.bucket_for_test, result.outcome)
                .await?;
            failed(result)?;
        }

        let old_item = existing.as_ref().map(|e| e.credentials_item_id.clone());
        if let Some(resolved) = &new_credentials {
            row.credentials_item_id = self.link(&row, resolved).await?;
        }
        let written = row.clone();
        if let Err(error) = self
            .db
            .write(move |tx| Ok(store::put_connection(tx, &written)?))
            .await
        {
            if new_credentials
                .as_ref()
                .is_some_and(link::Resolved::creates)
            {
                credentials::delete(&self.passwords, &row.credentials_item_id).await?;
            }
            return Err(error);
        }
        if let Some(resolved) = &new_credentials {
            self.tag_chosen(resolved).await;
        }
        if let (Some(_), Some(old)) = (&new_credentials, old_item) {
            if old != row.credentials_item_id {
                self.delete_credentials_if_unused(&old).await;
            }
        }
        // A generated key is in the password manager now; the process forgets it (FR-008).
        if let Some(key_id) = generated {
            keys::pending().take(&key_id);
        }
        self.view_of(&row).await
    }

    /// Remembers `outcome` for the storages of the connection on `bucket`.
    async fn record_for_bucket(
        &self,
        connection_id: &str,
        bucket: &str,
        outcome: TestOutcome,
    ) -> Result<()> {
        let (connection_id, bucket) = (connection_id.to_owned(), bucket.to_owned());
        self.db
            .write(move |tx| {
                let now = clock::now();
                for storage in store::storages(tx)? {
                    if storage.connection_id == connection_id && storage.bucket == bucket {
                        store::record_test(tx, &storage.id, outcome, &now)?;
                    }
                }
                Ok(())
            })
            .await
    }

    /// Attempts to delete an unreferenced credential entry, logging deletion failures.
    /// A failed reference lookup leaves the entry in place.
    async fn delete_credentials_if_unused(&self, item_id: &str) {
        let id = item_id.to_owned();
        let in_use = self
            .db
            .read(move |q| Ok(store::credentials_in_use(q, &id)?))
            .await;
        if let Ok(false) = in_use {
            if let Err(error) = credentials::delete(&self.passwords, item_id).await {
                log::warn!("remote storage: old credentials entry not deleted: {error}");
            }
        }
    }

    /// Saves a storage; a new one or a new bucket is tested first.
    pub async fn save_storage(&self, input: StorageInput) -> Result<StorageView> {
        let existing = match &input.id {
            Some(id) => Some(self.storage(id).await?),
            None => None,
        };
        let connection_id = existing.as_ref().map_or_else(
            || input.connection_id.clone(),
            |storage| storage.connection_id.clone(),
        );
        let connection = self.connection(&connection_id).await?;
        let now = clock::now();
        let row = StorageRow {
            id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            connection_id,
            name: input.name.trim().to_owned(),
            bucket: input.bucket.trim().to_owned(),
            created_at: existing
                .as_ref()
                .map_or_else(|| now.clone(), |e| e.created_at.clone()),
            updated_at: now.clone(),
        };
        store::check_storage(&row, connection.protocol)?;
        let tested = if existing.as_ref().is_none_or(|e| e.bucket != row.bucket) {
            let credentials = credentials::read(
                &self.passwords,
                &self.db,
                &connection.credentials_item_id,
                connection.protocol,
            )
            .await?;
            let access = Access {
                location: Location::of(&connection, &row.bucket),
                credentials,
            };
            let result = self.probe_creating(&access, input.create_folder).await?;
            failed(result)?;
            Some(LastTest {
                at: now,
                outcome: TestOutcome::Passed,
            })
        } else {
            None
        };
        let written = row.clone();
        let record = tested.clone();
        self.db
            .write(move |tx| {
                store::put_storage(tx, &written)?;
                if let Some(test) = &record {
                    store::record_test(tx, &written.id, test.outcome, &test.at)?;
                }
                Ok(())
            })
            .await?;
        let nested = self.nested_folder(&row).await?;
        let (id, last) = (row.id.clone(), tested);
        let (extensions, last_test) = self
            .db
            .read(move |q| {
                let last_test = match last {
                    Some(test) => Some(test),
                    None => store::tests(q)?
                        .remove(&id)
                        .map(|(at, outcome)| LastTest { at, outcome }),
                };
                Ok((store::extensions_of(q, &id)?, last_test))
            })
            .await?;
        Ok(StorageView {
            id: row.id,
            connection_id: row.connection_id,
            name: row.name,
            bucket: row.bucket,
            last_test,
            extensions,
            warning: nested.then_some(StorageWarning::NestedFolder),
        })
    }

    /// Whether another storage of the same WebDAV or SFTP connection has its folder inside or
    /// around the folder of `row` (spec 050 FR-003): a warning, not an error.
    async fn nested_folder(&self, row: &StorageRow) -> Result<bool> {
        let row = row.clone();
        self.db
            .read(move |q| {
                let tree = store::connection(q, &row.connection_id)?
                    .is_some_and(|connection| connection.protocol.is_tree());
                let within = |inner: &str, outer: &str| {
                    outer.is_empty() || inner == outer || inner.starts_with(&format!("{outer}/"))
                };
                Ok(tree
                    && store::storages(q)?.iter().any(|other| {
                        other.id != row.id
                            && other.connection_id == row.connection_id
                            && (within(&other.bucket, &row.bucket)
                                || within(&row.bucket, &other.bucket))
                    }))
            })
            .await
    }

    /// Creates the missing folder of the saved storage `storage_id` on the user's word (spec 050
    /// FR-006), with missing folders above it, then tests it and remembers the outcome.
    pub async fn create_folder(&self, storage_id: &str) -> Result<TestResult> {
        let access = self.access_of(storage_id).await?;
        let result = self.probe_creating(&access, true).await?;
        self.record(storage_id, result.outcome).await?;
        Ok(TestResult {
            outcome: result.outcome,
            leftover_key: result.leftover_key,
            host_key: result.host_key,
        })
    }

    /// The test of `access`; when it finds the folder of a WebDAV or SFTP storage missing and
    /// `create` says so, holzi makes the folder (with missing folders above it) and tests again.
    /// Never makes a folder without `create` (FR-006).
    async fn probe_creating(
        &self,
        access: &Access,
        create: bool,
    ) -> Result<super::probe::ProbeResult> {
        let first = probe(self.store.as_ref(), access).await;
        let folder = access.location.bucket.clone();
        if first.outcome != TestOutcome::FolderMissing || !create || folder.is_empty() {
            return Ok(first);
        }
        let mut root = access.clone();
        root.location.bucket = String::new();
        let deadline = tokio::time::Instant::now() + super::probe::TEST_TIME;
        if let Err(error) = self
            .store
            .put(&root, &format!("{folder}/"), Vec::new(), deadline)
            .await
        {
            log::warn!("remote storage: folder not created: {error}");
            return Ok(super::probe::ProbeResult {
                host_key: None,
                outcome: super::probe::outcome_of_write(error, true),
                leftover_key: None,
            });
        }
        Ok(probe(self.store.as_ref(), access).await)
    }

    /// What the storage holds and may hold, for the sidebar of the file browser. `None` where the
    /// provider cannot tell (S3) or does not answer in time: the sidebar then shows nothing. A
    /// storage whose credentials are missing is an error, as for every other call.
    pub async fn space(&self, storage_id: &str) -> Result<Option<super::StorageSpace>> {
        let access = self.access_of(storage_id).await?;
        let deadline = tokio::time::Instant::now() + super::probe::TEST_TIME;
        Ok(self
            .store
            .space(&access, deadline)
            .await
            .unwrap_or_else(|error| {
                log::debug!("remote storage: the space of a storage is not known: {error}");
                None
            }))
    }

    /// Tests a storage and remembers the outcome on this device.
    pub async fn test_storage(&self, storage_id: &str) -> Result<TestResult> {
        let access = match self.access_of(storage_id).await {
            Ok(access) => access,
            Err(error @ HolziError::StorageCredentialsUnavailable { state }) => {
                if state == CredentialsState::Missing {
                    self.record(storage_id, TestOutcome::AccessDenied).await?;
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let result = probe(self.store.as_ref(), &access).await;
        self.record(storage_id, result.outcome).await?;
        Ok(TestResult {
            outcome: result.outcome,
            leftover_key: result.leftover_key,
            host_key: result.host_key,
        })
    }

    /// Remembers the outcome of a test or of a call of an extension (data-model.md).
    pub async fn record(&self, storage_id: &str, outcome: TestOutcome) -> Result<()> {
        let id = storage_id.to_owned();
        self.db
            .write(move |tx| Ok(store::record_test(tx, &id, outcome, &clock::now())?))
            .await
    }

    /// The storages and extensions a removal takes with it (FR-007).
    pub async fn removal_preview(&self, target: RemovalTarget) -> Result<RemovalPreview> {
        self.db
            .read(move |q| {
                let storages: Vec<StorageRow> = match (&target.connection_id, &target.storage_id) {
                    (_, Some(id)) => {
                        vec![store::storage(q, id)?.ok_or(HolziError::StorageNotFound)?]
                    }
                    (Some(id), None) => {
                        store::connection(q, id)?.ok_or(HolziError::StorageNotFound)?;
                        store::storages(q)?
                            .into_iter()
                            .filter(|s| s.connection_id == *id)
                            .collect()
                    }
                    (None, None) => return Err(HolziError::StorageNotFound.into()),
                };
                let mut extensions = BTreeSet::new();
                for storage in &storages {
                    extensions.extend(store::extensions_of(q, &storage.id)?);
                }
                Ok(RemovalPreview {
                    storages: storages.into_iter().map(|s| s.name).collect(),
                    extensions: extensions.into_iter().collect(),
                })
            })
            .await
    }

    /// Removes a storage and the permissions that name it, in one write.
    pub async fn remove_storage(&self, storage_id: &str) -> Result<()> {
        self.storage(storage_id).await?;
        let id = storage_id.to_owned();
        self.db
            .write(move |tx| Ok(store::remove_storage(tx, &id)?))
            .await
    }

    /// Removes a connection with its storages, then its credentials entry for good when no other
    /// connection uses it.
    pub async fn remove_connection(&self, connection_id: &str) -> Result<()> {
        let id = connection_id.to_owned();
        let unused = self
            .db
            .write(move |tx| Ok(store::remove_connection(tx, &id)?))
            .await?;
        if let Some(item_id) = unused {
            credentials::delete(&self.passwords, &item_id).await?;
        }
        Ok(())
    }
}

#[path = "service_host_key.rs"]
mod host_key;

#[path = "service_credentials.rs"]
mod credentials_input;
use credentials_input::credentials_of;

#[path = "service_link.rs"]
mod link;
use link::view;
