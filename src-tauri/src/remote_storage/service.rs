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
use zeroize::Zeroizing;

use super::address::{self, AddressError, Resolver};
use super::model::{
    ConnectionInput, ConnectionView, CredentialsInput, LastTest, RemovalPreview, RemovalTarget,
    StorageInput, StorageOverview, StorageView, TestResult,
};
use super::probe::probe;
use super::{
    credentials, store, Access, ConnectionRow, Credentials, CredentialsState, EndpointScope,
    Location, PasswordCredentials, Protocol, RemoteStore, S3Credentials, SshKeyCredentials,
    StorageRow, TestOutcome,
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
    store: Arc<dyn RemoteStore>,
    resolver: Arc<dyn Resolver>,
}

/// Builds a validation error identifying the rejected input field.
fn invalid(field: &str) -> HolziError {
    HolziError::StorageInvalid {
        field: field.to_owned(),
    }
}

/// Builds the public connection view with credential availability and the HTTP warning flag.
fn view(row: &ConnectionRow, credentials: super::CredentialsState) -> ConnectionView {
    ConnectionView {
        id: row.id.clone(),
        protocol: row.protocol,
        provider_name: row.provider_name.clone(),
        provider_kind: row.provider_kind,
        endpoint: row.endpoint.clone(),
        region: row.region.clone(),
        addressing: row.addressing,
        insecure: Url::parse(&row.endpoint).is_ok_and(|url| address::is_insecure(&url)),
        endpoint_scope: row.endpoint_scope,
        credentials,
    }
}

/// Validates required credential fields for `protocol` (spec 050 contracts/tauri-commands.md) and
/// normalizes the key id, the user name and the optional token. Secrets are kept exactly as entered.
fn credentials_of(input: CredentialsInput, protocol: Protocol) -> Result<Credentials> {
    let nonempty = |text: &Zeroizing<String>| !text.is_empty();
    match (input, protocol) {
        (
            CredentialsInput::S3 {
                access_key_id,
                secret_access_key,
                session_token,
            },
            Protocol::S3,
        ) => {
            let access_key_id = access_key_id.trim().to_owned();
            if access_key_id.is_empty() || !nonempty(&secret_access_key) {
                return Err(invalid("credentials"));
            }
            Ok(Credentials::S3(S3Credentials {
                access_key_id,
                secret_access_key,
                session_token: session_token
                    .filter(|token| !token.is_empty())
                    .map(|token| Zeroizing::new(token.trim().to_owned())),
            }))
        }
        (CredentialsInput::Password { username, password }, Protocol::WebDav | Protocol::Sftp) => {
            let username = username.trim().to_owned();
            if username.is_empty() || !nonempty(&password) {
                return Err(invalid("credentials"));
            }
            Ok(Credentials::Password(PasswordCredentials {
                username,
                password,
            }))
        }
        (
            CredentialsInput::SshKey {
                username,
                private_key,
                passphrase,
            },
            Protocol::Sftp,
        ) => {
            let username = username.trim().to_owned();
            if username.is_empty() || private_key.trim().is_empty() {
                return Err(invalid("credentials"));
            }
            Ok(Credentials::SshKey(SshKeyCredentials {
                username,
                private_key,
                passphrase: passphrase.filter(|text| !text.is_empty()),
            }))
        }
        _ => Err(invalid("credentials")),
    }
}

/// A test that did not get as far as the provider: its address is not known.
fn unreachable() -> HolziError {
    HolziError::StorageTestFailed {
        outcome: TestOutcome::Unreachable,
        leftover_key: None,
    }
}

/// Accepts a passed probe or returns its failure outcome and possible leftover object key.
fn failed(result: super::probe::ProbeResult) -> Result<()> {
    match result.outcome {
        TestOutcome::Passed => Ok(()),
        outcome => Err(HolziError::StorageTestFailed {
            outcome,
            leftover_key: result.leftover_key,
        }),
    }
}

impl StorageService {
    /// A storage service over the vault, its password manager, a provider and a resolver.
    pub fn new(
        db: VaultDb,
        passwords: PasswordsService,
        store: Arc<dyn RemoteStore>,
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
                    let state = credentials::state_in(q, &row.credentials_item_id)?;
                    connections.push(view(&row, state));
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
        let endpoint = input.endpoint.as_deref().unwrap_or("").trim().to_owned();
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
        store::check_connection(&row)?;
        let url = address::endpoint_url(&Location::of(&row, &input.bucket_for_test))
            .map_err(|_| invalid("endpoint"))?;

        let new_credentials = input
            .credentials
            .map(|credentials| credentials_of(credentials, row.protocol))
            .transpose()?;
        let moved = existing.as_ref().is_none_or(|e| {
            e.endpoint != row.endpoint || e.region != row.region || e.addressing != row.addressing
        });
        let test_with = match (&new_credentials, &existing) {
            (Some(credentials), _) => Some(credentials.clone()),
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
            return Err(invalid("bucket"));
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
            let result = probe(self.store.as_ref(), &access).await;
            self.record_for_bucket(&row.id, &input.bucket_for_test, result.outcome)
                .await?;
            failed(result)?;
        }

        let old_item = existing.as_ref().map(|e| e.credentials_item_id.clone());
        if let Some(credentials) = &new_credentials {
            row.credentials_item_id = credentials::create(
                &self.passwords,
                row.protocol,
                &row.provider_name,
                &row.endpoint,
                credentials,
            )
            .await?;
        }
        let written = row.clone();
        if let Err(error) = self
            .db
            .write(move |tx| Ok(store::put_connection(tx, &written)?))
            .await
        {
            if new_credentials.is_some() {
                credentials::delete(&self.passwords, &row.credentials_item_id).await?;
            }
            return Err(error);
        }
        if let (Some(_), Some(old)) = (&new_credentials, old_item) {
            self.delete_credentials_if_unused(&old).await;
        }
        let state = credentials::state(&self.db, &row.credentials_item_id).await?;
        Ok(view(&row, state))
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
            let result = probe(self.store.as_ref(), &access).await;
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
        })
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
