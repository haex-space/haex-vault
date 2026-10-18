# `haex-crdt` — Extraction Plan

**Status**: **Executed — this plan is history, not a to-do.** The crate shipped on 2026-09-06 and
lives in the separate `haexmas/haex-crdt` repository. Holzi's current contract is pinned to
revision `1c069ef0ea19143af2748f40fc41cba05c94dbe1` (package 0.4.0). Read the crate's own source
and README for the delivered API; this document records the intent and the reasoning, and its
future tense is no longer accurate.

Known drift between this plan and what shipped:

- §4.1's `DeviceIdMismatch` contract shipped, but the recorded UUID lives in
  `haex_crdt_configs_no_sync` (key `CONFIG_KEY_DEVICE_ID`), not in `haex_hlc_state` as written
  here. The pinned contract only supports reject-on-mismatch; device-ID adoption for a relocated
  `.db` remains a future haex-crdt revision. See [`2026-09-04-v1-scope-design.md` §5](./2026-09-04-v1-scope-design.md)
  for Holzi's decision.
- The crate's scope came out narrower than `specs/001-frontend-onboarding/` assumed: no identity,
  pairing, or sync transport. holzi owns all of that, plus its own federation-state migrations and
  the v1 sync transport, which calls `haex-crdt`'s scanner and apply APIs. A transport supplied by
  the crate remains a future-consumer option, not part of holzi's v1 contract.
- §12's "next actions" are all discharged.

**Original status**: Draft. Written 2026-09-04. Companion to [`2026-09-04-v1-scope-design.md`](./2026-09-04-v1-scope-design.md), which decided that holzi consumes `haex-crdt` as a Rust crate dependency and that the crate must exist before holzi's first implementation slice.

**Scope of this document**: how `haex-crdt` gets extracted from `haex-vault 2.x (Legacy)` into a standalone repository (`haexmas/haex-crdt`), what its crate boundary is, and what stays behind in `haex-vault 2.x`. Not a spec; a refactor + release plan.

**Ownership**: haex-vault 2.x owns the source. This plan is written from holzi's side because holzi is the consumer whose timeline depends on it. Cross-link into haex-vault 2.x when the extraction actually starts.

---

## 1. Source: what `haex-vault 2.x` has today

`haex-vault 2.x` is a single Rust crate under `src-tauri/` (Tauri app). No workspace, no `crates/` directory. Storage and CRDT logic sit inside the same crate as the Tauri app itself.

Verified today via a code-mapping pass:

- **Storage**: `rusqlite = "0.40"` with the `bundled-sqlcipher-vendored-openssl` + `functions` + `hooks` features. At-rest encryption is **SQLCipher**, activated via `PRAGMA key`. Home-grown migration engine tied to Drizzle-generated SQL files.
- **CRDT model**: home-grown column-level Last-Write-Wins over SQLite triggers, keyed by `uhlc` Hybrid Logical Clocks. Every CRDT-managed table gets three implicit columns (`haex_hlc`, `haex_column_hlcs`, `haex_column_sigs`) via a `CrdtTransformer` that rewrites `CREATE TABLE` statements. No `automerge`/`yrs`/`loro` — the CRDT is a haex-vault 2.x original.
- **Sync transport**: not part of the CRDT core. Iroh QUIC is used for peer sync (`space_delivery/local/`), for large-file transfer (`peer_storage/`), and for backend/cloud file-sync (`file_sync/`). Sync is a separate module that consumes the CRDT layer's scanner and apply APIs.
- **Auth-signed extensions**: `column_sig/` and `registry_row_sig/` add Ed25519 signatures over CRDT column values, tied to UCAN identity. Deeply coupled to `crate::ucan::` and `crate::mls::`.
- **Tauri coupling in CRDT/storage**: narrow. Confined to `HlcService::try_initialize` (reads device ID from `tauri_plugin_store`), the migration loader (`tauri::path::BaseDirectory`), and a scattering of `#[tauri::command]` shims. The internal logic of `trigger.rs`, `scanner.rs`, `cleanup.rs`, `transformer/`, `apply/` uses no `tauri::` imports.
- **Tests**: extensive, mostly inline `#[cfg(test)]` next to the implementation, plus integration tests in `src-tauri/tests/`. The CRDT and storage tests use `Connection::open_in_memory` and `HlcService::new_for_testing` — they do not need a Tauri harness.
- **Change velocity**: the extraction candidates are the calmer surface of haex-vault 2.x. Recent churn (103 commits touching `src-tauri/src/{crdt,database}` in ~8 weeks) is concentrated in `column_sig`, sync transport, DoS defences, and UCAN — all of which stay in `haex-vault 2.x`.

## 2. Target: what `haex-crdt` is

A standalone Rust crate published as `haexmas/haex-crdt` on GitHub, versioned independently. Consumed by `haex-vault 2.x` and `holzi` as a git-tagged dependency (crates.io is deferred until the API stabilizes).

The crate provides:

- Encrypted SQLite storage (SQLCipher).
- A migration engine.
- HLC service (uhlc-based Hybrid Logical Clocks with SQLite-persisted state).
- Column-level LWW CRDT infrastructure: the `CrdtTransformer` that rewrites schemas, the trigger installer that instruments tables, the scanner that reads local changes, the apply pipeline that merges remote changes.
- Cleanup / retention utilities for deleted-row logs.
- A pluggable `SignatureProvider` trait so consumers can add per-column signing without the crate itself depending on a particular identity/auth system.
- A pluggable `DeviceIdProvider` trait so consumers supply the device UUID without the crate depending on a particular OS keystore.
- A migration SQL loader closure so consumers control where migration SQL comes from (bundled resource, disk, embedded, remote fetch).

The crate does **not** provide:

- Sync transport (iroh, WebSocket, cloud). Sync is the consumer's job; the crate exposes scanner + apply APIs that any transport can wire up.
- Identity, UCAN, or MLS. `SignatureProvider` is an interface, not an implementation.
- Any Tauri or async framework opinion beyond what `rusqlite` already implies (blocking I/O; consumers wrap in `spawn_blocking` if they want async).
- Frontend or IPC surface.

## 3. What stays in `haex-vault 2.x`

Left behind in `haex-vault 2.x` after the extraction:

- `crdt/column_sig/` and `crdt/registry_row_sig/` — Ed25519 signing coupled to `crate::ucan::`. In the new world, these become `haex-vault 2.x`'s implementation of `haex-crdt`'s `SignatureProvider` trait, plus their own local storage tables and UCAN plumbing.
- `space_delivery/local/` — iroh + MLS sync loop. Uses `haex-crdt`'s scanner/apply. Stays as `haex-vault 2.x`'s sync transport.
- `peer_storage/` — iroh QUIC for large-file peer transfer. Unrelated to CRDT.
- `file_sync/` — cloud/S3-backed backup sync. Consumes CRDT data but is not part of it.
- `owner_sync/`, `mls/`, `ucan/` — identity and encryption plane.
- `external_bridge/`, `extension/` — Tauri-app-scoped concerns.
- All `#[tauri::command]` shims — remain in `haex-vault 2.x` as the Tauri entry-point layer over `haex-crdt`'s pure-Rust API.

## 4. Abstraction points

Three coupling surfaces between the extraction candidates and `haex-vault 2.x`'s wider system need to be broken by introducing traits/callbacks in the new crate.

### 4.1 `DeviceIdProvider`

Currently `HlcService::try_initialize` reaches through an `&AppHandle` into `tauri_plugin_store`'s `instance.json` to get the persistent device UUID.

New trait in `haex-crdt`:

```rust
pub trait DeviceIdProvider {
    fn device_id(&self) -> Result<Uuid>;
}
```

Consumers implement it however they wish. `haex-vault 2.x` wraps its existing store-based lookup; `holzi` provides one backed by its own device-key generation. `haex-crdt` ships a `StaticDeviceId(Uuid)` implementation for tests and simple consumers.

**Contract**:

- The returned `Uuid` MUST be durable per physical device and stable across process restarts, OS reboots, and library upgrades within the same install. `uhlc::ID` uniqueness invariants depend on this, and `haex-crdt` persists HLC state (`haex_hlc_state`) keyed to this identity.
- The provider MUST NOT return a freshly generated `Uuid` on each call. Consumers that don't yet have a persisted device UUID are responsible for minting and persisting one _before_ handing a provider to `haex-crdt`.
- `Store::open` records the `device_id` observed on first successful open in `haex_hlc_state`. On subsequent opens, if the supplied `DeviceIdProvider` returns a `Uuid` that differs from the recorded one, `Store::open` returns `Error::DeviceIdMismatch { expected, supplied }` rather than silently rewriting HLC state — mismatch is treated as a consumer bug or a moved-database scenario, and recovery is the consumer's decision.

### 4.2 `SignatureProvider`

Currently `crdt/scanner.rs` and `crdt/commands/apply/` call directly into `crate::ucan::` for signing/verifying per-column Ed25519 signatures. This coupling is what makes the current code un-extractable.

New trait in `haex-crdt`:

```rust
pub trait SignatureProvider {
    fn sign_column(&self, preimage: &[u8]) -> Result<Vec<u8>>;
    fn verify_column(&self, preimage: &[u8], sig: &[u8], author: &AuthorId) -> Result<()>;
    fn author_id(&self) -> AuthorId;

    /// Row-level / batch-level policy hook. Called by `apply_remote_changes`
    /// **before** any write, inside the apply transaction. Returning `Err`
    /// rejects the entire batch; the transaction rolls back.
    fn on_before_apply(&self, changes: &RemoteChanges) -> Result<()> {
        let _ = changes;
        Ok(())
    }
}

pub struct NoopSignatureProvider;
// NoopSignatureProvider inherits the default no-op `on_before_apply`,
// signs with empty payloads, and accepts empty incoming sigs on verify.
impl SignatureProvider for NoopSignatureProvider {
    fn sign_column(&self, _preimage: &[u8]) -> Result<Vec<u8>> { Ok(Vec::new()) }
    fn verify_column(&self, _preimage: &[u8], sig: &[u8], _author: &AuthorId) -> Result<()> {
        if sig.is_empty() { Ok(()) } else { Err(Error::UnexpectedSignatureUnderNoop) }
    }
    fn author_id(&self) -> AuthorId { AuthorId::anonymous() }
}
```

`haex-vault 2.x` implements the trait against its UCAN/DID stack (reusing today's `column_sig` + `registry_row_sig` code, now living inside haex-vault 2.x). `holzi` uses `NoopSignatureProvider` in v1 (no per-column signing needed while the closed federation trusts every attested device equally); it can graduate to a real provider post-v1 if needed. The crate's storage schema reserves the `haex_column_sigs` column even for no-op providers, so a future upgrade does not require a migration.

**Trust contract**:

- `apply_remote_changes` is **all-or-nothing**. The full sequence — pre-apply hook, verification of every column change, then writes — runs inside a single `IMMEDIATE` transaction:
  1. Call `SignatureProvider::on_before_apply(&changes)`. On `Err`, roll back and return.
  2. **Preflight-verify every incoming column change** via `SignatureProvider::verify_column(preimage, sig, author)` before writing anything. Any verification failure rolls back the transaction and returns `Error::SignatureVerificationFailed { first_failed_change, .. }`. No column is written before every column has been verified.
  3. Apply the writes. Any apply-side error (constraint violation, disk error, HLC clock issue) rolls back the transaction and returns the corresponding `Error`; no earlier writes remain.
- The provider decides whether an empty `sig` is acceptable — the crate itself has no policy. `NoopSignatureProvider::sign_column` returns `Ok(Vec::new())`, so local writes persist an empty byte string in `haex_column_sigs`; `NoopSignatureProvider::verify_column` accepts empty signatures (`sig.is_empty() → Ok`) and rejects non-empty ones it cannot verify. Local scan and remote apply between two `NoopSignatureProvider` stores therefore round-trip cleanly. It performs no transport attestation itself; consumers using `NoopSignatureProvider` MUST deliver remote changes over an already-authenticated transport (an MLS group in `haex-vault 2.x`, an authenticated iroh channel in `holzi`). The crate documents this precondition prominently so a consumer cannot silently accept unsigned changes from an unauthenticated source.
- A signing provider (haex-vault 2.x's UCAN-backed one) rejects empty signatures on already-signed vaults, so downgrading to `NoopSignatureProvider` on an existing signed vault is not silently possible.
- **Row-level trust** — today's `registry_row_sig/` — is **not** something `haex-crdt`'s apply pipeline validates on its own. Since `registry_row_sig` stays in `haex-vault 2.x`, the trait exposes the pre-apply hook (`on_before_apply`) declared above; `haex-vault 2.x`'s hook enforces registry-row-sig policy against the incoming batch; `NoopSignatureProvider`'s inherited default is a no-op.

### 4.3 Migration SQL loader

Currently the migration engine calls `tauri::path::BaseDirectory::Resource` to locate SQL files bundled into the Tauri app.

New API in `haex-crdt`:

```rust
pub trait MigrationSource {
    fn load_migration(&self, name: &MigrationName) -> Result<String>;
    fn list_migrations(&self) -> Result<Vec<MigrationName>>;
}
```

`haex-vault 2.x` wraps its Tauri resource lookup. `holzi` provides an implementation backed by its own bundled resources (Tauri or otherwise). A `StaticMigrationSource(BTreeMap<MigrationName, String>)` ships with the crate for tests.

**Contract**:

- `list_migrations` MUST return unique names in a stable, total order (lexicographic on `MigrationName`, which encodes the applied ordinal). Two calls at the same version of a shipped consumer MUST return identical sequences.
- `load_migration` MUST return byte-identical content for the same name across releases of the consumer. Once a migration has been applied by any deployed instance, its content is frozen; new work goes into a new migration name.
- The engine stores a SHA-256 digest of each applied migration's SQL in the journal. On subsequent starts, a mismatch between the stored digest and the `load_migration` result aborts with `Error::MigrationContentDrift { name, expected, found }` rather than re-running or silently continuing.
- **Journal reconciliation on every open, scoped per journal**: before executing any pending migrations, each journal is reconciled independently against its own source. `haex_crdt_migrations` is reconciled against the crate's built-in bookkeeping list (compiled into `haex-crdt`); `haex_app_migrations` is reconciled against the consumer's `MigrationSource`. Missing entries in either journal abort `Store::open` with `Error::MigrationMissingFromSource { journal, name }` — the `journal` field distinguishes crate-owned vs consumer-owned so misdiagnosis is impossible. This scoping prevents a valid crate-owned CRDT migration from being wrongly reported as missing from a consumer source that does not (and must not) contain it. Recovery is the consumer's decision (restore the migration, or ship an explicit forward-only migration that supersedes it).

**Two migration sets, two journals**:

- **Crate-owned CRDT bookkeeping migrations** (`haex_hlc_state`, tombstone tables, other CRDT bookkeeping tables) are compiled into `haex-crdt` itself. Consumers do **not** supply them. They are journaled in `haex_crdt_migrations` and versioned with the crate.
- **Consumer-owned schema migrations** (the consumer's own tables, plus any CRDT triggers installed via `install_crdt` on those tables) come from the consumer's `MigrationSource` and are journaled separately in `haex_app_migrations`. Their identifiers live in a namespace controlled by the consumer.
- On `Store::open`, crate-owned migrations run first, then consumer-owned migrations. Both share the same connection and transaction discipline, but their journal rows never collide.

### 4.4 Optional: event callbacks

Today `crdt::mod::notify_dirty_tables_changed` emits a Tauri event when a scan detects dirty tables. In the crate this becomes a callback the consumer registers:

```rust
pub type DirtyTablesCallback = Arc<dyn Fn(&[String]) + Send + Sync>;
```

`haex-vault 2.x` registers a callback that fires a Tauri event. `holzi` registers one that pokes its own state store or MCP surface.

## 5. Module inventory (haex-vault 2.x → haex-crdt)

Working proposal; final paths decided at extraction time.

| haex-vault 2.x path                                                                                                                     | haex-crdt path                   | Notes                                      |
| --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------- | ------------------------------------------ |
| `src-tauri/src/database/mod.rs` (minus `#[tauri::command]`s)                                                                            | `src/db/mod.rs`                  | Re-exports adjusted                        |
| `src-tauri/src/database/core/*`                                                                                                         | `src/db/core/*`                  | Whole subtree                              |
| `src-tauri/src/database/vault_lock.rs`                                                                                                  | `src/db/lock.rs`                 | `fs2`-based file lock                      |
| `src-tauri/src/database/migrations/*` (minus Tauri loader)                                                                              | `src/db/migrations/*`            | Loader becomes trait consumer              |
| `src-tauri/src/database/connection_context.rs`                                                                                          | `src/db/connection_context.rs`   |                                            |
| `src-tauri/src/database/row.rs`, `stats.rs`, `constants.rs`, `paths.rs`, `listing.rs`, `maintenance.rs`, `import_delete.rs`, `error.rs` | `src/db/*`                       | Direct move                                |
| `src-tauri/src/crdt/hlc.rs` (minus AppHandle path)                                                                                      | `src/crdt/hlc.rs`                | Uses `DeviceIdProvider`                    |
| `src-tauri/src/crdt/trigger.rs`                                                                                                         | `src/crdt/trigger.rs`            | Direct move                                |
| `src-tauri/src/crdt/scanner.rs`                                                                                                         | `src/crdt/scanner.rs`            | Direct move                                |
| `src-tauri/src/crdt/cleanup.rs`                                                                                                         | `src/crdt/cleanup.rs`            | Direct move                                |
| `src-tauri/src/crdt/transformer/*`                                                                                                      | `src/crdt/transformer/*`         | Direct move                                |
| `src-tauri/src/crdt/insert_transformer.rs`                                                                                              | `src/crdt/insert_transformer.rs` | Direct move                                |
| `src-tauri/src/crdt/commands/apply/*` (minus command shims)                                                                             | `src/crdt/apply/*`               | Rename from `commands/apply`               |
| `src-tauri/src/crdt/column_sig/*`                                                                                                       | **stays in haex-vault 2.x**      | Becomes `SignatureProvider` impl           |
| `src-tauri/src/crdt/registry_row_sig/*`                                                                                                 | **stays in haex-vault 2.x**      | Same                                       |
| Table name constants used by CRDT (`TABLE_CRDT_*`, `COL_CRDT_*`)                                                                        | `src/db/constants.rs`            | These name generic CRDT bookkeeping tables |
| Vault-specific constants (`vault_settings_key`)                                                                                         | **stays in haex-vault 2.x**      |                                            |

Tests travel with the modules. Integration tests in `src-tauri/tests/` that touch column_sig or registry_row_sig split: pure CRDT round-trips move to `tests/` in the new crate; sig-specific vectors stay in haex-vault 2.x.

## 6. Public API sketch

Not final; a working shape for reviewers.

```rust
// Opening + configuring a store
pub struct StoreConfig {
    pub path: PathBuf,
    pub key: SqlCipherKey,      // wrapper around a byte string
    pub create_if_missing: bool,
    pub device_id: Arc<dyn DeviceIdProvider>,
    pub signature_provider: Arc<dyn SignatureProvider>,
    pub migration_source: Arc<dyn MigrationSource>,
    pub on_dirty_tables: Option<DirtyTablesCallback>,
}

pub struct Store { /* opaque */ }

impl Store {
    pub fn open(config: StoreConfig) -> Result<Self>;
    pub fn with_connection<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R>;
    pub fn hlc(&self) -> &HlcService;
    pub fn apply_migrations(&self) -> Result<()>;
    pub fn install_crdt(&self, table: &str) -> Result<()>; // installs columns + triggers
    pub fn scan_local_changes(&self, scope: ScanScope) -> Result<LocalChanges>;
    pub fn apply_remote_changes(&self, changes: RemoteChanges) -> Result<ApplyReport>;
    pub fn cleanup_deleted_rows(&self, retention: RetentionPolicy) -> Result<CleanupReport>;
}

// SQL rewriting for CRDT-managed tables
pub struct CrdtTransformer;
impl CrdtTransformer {
    pub fn transform_create_table(sql: &str) -> Result<String>;
    pub fn transform_insert(sql: &str) -> Result<String>;
}
```

Consumers wire `Store` up, define their own tables (running each `CREATE TABLE` through `CrdtTransformer::transform_create_table` first, or via `install_crdt` after creation for existing tables), and rely on the `Store` for both local operations and sync-scanning.

**`Store::with_connection` and the `rusqlite` dependency contract**:

`with_connection` exposes a `&rusqlite::Connection`. Two consumers linking different `rusqlite` versions would see the crate's `Connection` type and their own as distinct types, so their `ToSql`/`FromSql` values could not be passed through this callback. Two paths, decided during Step 1:

- **Preferred**: `haex-crdt` declares a single supported `rusqlite` version range in its `Cargo.toml` and both consumers resolve to the same crate instance (workspace `[dependencies] rusqlite = { version = "X", features = [...] }`). `with_connection` stays on the public API for consumers that accept this contract.
- **Fallback if the shared-dependency contract proves impractical**: `with_connection` moves behind a `#[cfg(feature = "raw-connection")]` feature (default off), and the public API adds crate-owned typed operations (`Store::execute_stmt(sql, params)`, `Store::query_rows(sql, params, mapper)`) that take primitive types and never leak the `Connection`.

Either way, `Store::hlc`, `apply_migrations`, `install_crdt`, `scan_local_changes`, `apply_remote_changes`, and `cleanup_deleted_rows` stay on the stable public API and do not expose `rusqlite` types.

**`install_crdt` backfill contract**:

`install_crdt(table)` on a table that already contains rows MUST:

1. Run inside a single `IMMEDIATE` transaction. Either the columns, triggers, and backfill all commit, or none do — a partial install that leaves rows without HLC metadata is not a reachable state.
2. Add `haex_hlc`, `haex_column_hlcs`, `haex_column_sigs` if missing, then populate them for every pre-existing row: `haex_hlc` set to a freshly-issued HLC timestamp taken _inside_ the transaction (all pre-existing rows share one causal instant, per the `HlcService`); `haex_column_hlcs` set to the same HLC for every non-metadata column; `haex_column_sigs` populated by calling `SignatureProvider::sign_column` per column (empty payload for `NoopSignatureProvider`).
3. Record every backfilled row in the local-changes journal so the next `scan_local_changes` returns them. `install_crdt` on an existing table is equivalent, sync-wise, to "these rows were just created here, first time"; peers receive them via the normal apply path.
4. Refuse (`Error::CrdtAlreadyInstalled`) if the three metadata columns already exist, unless the caller passed `InstallCrdtOptions::allow_reinstall = true`, in which case only triggers are re-installed and no backfill runs.

## 7. Extraction sequence (source-of-change side, in `haex-vault 2.x`)

Concrete steps whoever owns the extraction will take. Not this PR's work; captured so the timing is legible.

1. **Refactor pass in `haex-vault 2.x`, no new repo yet.** Introduce the three traits (`DeviceIdProvider`, `SignatureProvider`, `MigrationSource`) inside `haex-vault 2.x` itself. Rewrite `HlcService`, migration loader, and column_sig call-sites to go through the traits. Ship no functional change. Verify with existing tests.
2. **Cut the crate as a `haex-vault 2.x` sub-crate first.** Turn `haex-vault 2.x` into a Cargo workspace; create `crates/haex-crdt/` and move the identified modules there. `haex-vault 2.x` imports it as a `path` dependency. All existing tests keep passing.
3. **Move to standalone repo.** Create `haexmas/haex-crdt` on GitHub, copy the crate over with its full git history if possible (`git filter-repo` or manual), open a first release tag (`v0.1.0`). `haex-vault 2.x` switches its dependency from `path` to `git = "...", tag = "v0.1.0"`.
4. **First integration for `holzi`.** `holzi`'s first implementation slice adds `haex-crdt` as a git dependency, implements its own `DeviceIdProvider`, uses `NoopSignatureProvider`, supplies a `MigrationSource` for its own schema.

Steps 1 and 2 are the invasive ones inside `haex-vault 2.x`. Step 3 is a repo-mechanics operation. Step 4 is holzi's job.

## 8. Testing strategy

- The crate's own test suite is the inline `#[cfg(test)]` tests moved from `haex-vault 2.x` plus the pure-CRDT integration tests. All use in-memory SQLite (`Connection::open_in_memory`) or a temp-file SQLCipher DB.
- Tests that require UCAN identity or MLS group state stay in `haex-vault 2.x`.
- After Step 2 above, `haex-vault 2.x`'s full test suite must still pass with `haex-crdt` as a workspace member. This is the acceptance bar for Step 3.
- **Standalone-git-tag acceptance test (gates Step 3 → Step 4)**: before `haex-vault 2.x` flips its dependency from `path = "crates/haex-crdt"` to `git = "...", tag = "v0.1.0"`, a throwaway consumer crate in a clean directory (no workspace, no path deps) pulls `haex-crdt` from the tagged commit and exercises the following. The setup opens **two independent stores** on **two separate SQLCipher database files** in a `tempdir`, each configured with its own durable `DeviceIdProvider` returning a **distinct**, stable `Uuid` (`device_a`, `device_b`). Both stores share the same `MigrationSource` and use `NoopSignatureProvider`.
  - (a) `Store::open` succeeds on both fresh databases.
  - (b) Crate-owned bookkeeping migrations run on both (journal in `haex_crdt_migrations`).
  - (c) A consumer-owned toy migration runs on both (journal in `haex_app_migrations`).
  - (d) The toy CRDT-managed table is created on **both** stores through the shared `MigrationSource` (so the plain table exists on A and B). Store A is then pre-populated with rows via direct inserts _before_ `install_crdt` runs on it, so step (d) exercises the backfill contract on A: `install_crdt(A, "toy")` succeeds inside its `IMMEDIATE` transaction, and `scan_local_changes(A)` returns the backfilled rows.
  - (d′) Store B calls `install_crdt(B, "toy")` on the (empty) toy table before any apply, so B has the CRDT metadata columns, triggers, and local-changes journal wiring in place. Without this, apply on B would either fail (missing metadata columns) or succeed against a store that isn't actually CRDT-managed — neither would validate remote application.
  - (e) End-to-end apply flow: store A does a local write against the CRDT-managed toy table; `scan_local_changes(A)` returns exactly that change (**assert scanned payload matches**); the change is handed to `apply_remote_changes(B)` (**assert it succeeds**); a fresh read from store B returns the applied row with A's HLC and author metadata (**assert readback**). Store A's HLC state remains bound to `device_a`; store B's to `device_b` — reopening either with the wrong `DeviceIdProvider` MUST return `Error::DeviceIdMismatch`.

  This is what proves the published git artifact is actually consumable, that remote-application works across independent stores, and that device-identity isolation holds — none of which the workspace-member test on its own establishes.

- `holzi` writes its own small integration test that opens a `haex-crdt` `Store`, defines a toy CRDT-managed table, writes and re-reads a row. Once the standalone-git-tag test above exists, holzi's own test builds on it rather than re-scaffolding.

## 9. Versioning, release, distribution

- Semver from the start.
- `v0.1.0` is the initial release once `haex-vault 2.x` fully uses the standalone repo. Behavior is expected to be identical to pre-extraction `haex-vault 2.x`; the version reflects "this is a new artifact and its API may still change" rather than an API bump.
- Git tags on `haexmas/haex-crdt` are the distribution mechanism. Both consumers use `git = ..., tag = "vX.Y.Z"` in their `Cargo.toml`.
- crates.io publication is deferred until the API has been used by more than one non-example consumer and has settled. Not v1-blocking for holzi.
- License: matches `haex-vault 2.x`'s license (to confirm during extraction).

## 10. Open questions

- **Ed25519 signature schema forward-compat.** Resolved in §4.2: `haex_column_sigs` is always present as a column and populated (empty byte string under `NoopSignatureProvider`). Schema is provider-independent so a vault can graduate to a real provider without a schema migration. `NoopSignatureProvider` rejects non-empty incoming sigs it cannot verify.
- **`AuthorId` shape.** UCAN uses DIDs; a simpler `holzi` federation could use device pubkeys directly. Whether `haex-crdt`'s `AuthorId` is a `String`, an enum, or a trait matters for how tightly downstream code binds to it. Working proposal: `String` newtype, treated as opaque by the crate.
- **Retention / tombstone policy.** `haex-vault 2.x` has its own retention logic in `crdt/cleanup.rs` tuned to space membership. Does the crate expose a `RetentionPolicy` enum with variants like `TimeBased`, `Manual`, or does it hand the tombstone table to the consumer to reap? Working proposal: parameterized retention, defaults to time-based.
- **Async story.** `rusqlite` is blocking. `haex-vault 2.x` today wraps calls in Tokio `spawn_blocking` at the command boundary. Does `haex-crdt` stay blocking-only and let consumers wrap, or provide a thin async wrapper? Working proposal: blocking-only in v0; async wrapper post-v1 if needed.
- **Multi-consumer compatibility for the migration engine.** Resolved in §4.3: crate-owned CRDT bookkeeping migrations live in `haex_crdt_migrations` (shipped by the crate); consumer-owned schema migrations live in `haex_app_migrations` (shipped by the consumer). Remaining sub-question for Step 1: whether `haex_app_migrations` is one table or is further partitioned when a single database is opened by two different consumer identities — not a concern for v1 (one database is opened by one consumer).
- **Repo-history preservation.** `git filter-repo` to keep commit history of the moved files vs a clean-start commit in the new repo. Working proposal: preserve history when reasonably practical; clean-start is the fallback.

## 11. What this plan does not do

- Does not schedule the work. Timing is up to whoever owns `haex-vault 2.x`.
- Does not commit to the exact API. The traits and `Store` sketch are working proposals; the final shape is decided during Step 1.
- Does not decide whether `holzi` writes its own tests against `haex-crdt`'s pre-1.0 API or waits. Working assumption: it does, and lives with the churn.

## 12. Next actions

For holzi (this repo), immediate:

- None until Step 1 or Step 2 above has begun in `haex-vault 2.x`.

For `haex-vault 2.x` (whoever owns it):

- Step 1 (introduce the three traits inside `haex-vault 2.x`, no new repo yet) is the smallest reversible move and unblocks everything after it. It is a self-contained refactor that ships with no functional change and can happen at any time.

For the operator:

- When ready to trigger the extraction, coordinate with the `haex-vault 2.x` project. This plan can be cross-linked from a `haex-vault 2.x` issue or ADR at that point.
