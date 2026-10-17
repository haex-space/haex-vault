# Contract: Shared Types (Rust ↔ TypeScript)

**Status**: Draft. All types are generated from Rust `#[derive(ts_rs::TS)]` structs and exported to `src/types/bindings/` via `cargo test` (per haex-vault 2.x's (Legacy) `generate:ts-types` script).

**V1 contract note**: Backup restore is a plain `open_instance` of a copied `.db`. No `Recover` mode, no second database, no restore-pairing handoff.

Frontend imports via a `@bindings/*` path alias:

```ts
import type { InstanceInfo } from '@bindings/InstanceInfo'
```

## Types

### `InstanceInfo`

Metadata for one instance in the list. No secrets.

```rust
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInfo {
    pub name: String,              // Filename basename without `.db`
    pub alias: String,             // Non-secret user-visible label; defaults to `name`
    pub last_access: String,       // ISO-8601, from the database mtime; open_instance refreshes it
    pub size_bytes: u64,           // File size at scan time
}
```

**TypeScript view**:

```ts
export type InstanceInfo = {
  name: string
  alias: string
  lastAccess: string
  sizeBytes: number
}
```

### `ConflictPolicy`

```rust
#[derive(Debug, Clone, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub enum ConflictPolicy {
    Rename,
    Overwrite,
    Abort,
}
```

### `CreateInstanceArgs`, `CreateInstanceResult`, `OpenInstanceArgs`, `ImportInstanceArgs`, `ImportInstanceResult`, `TrashInstanceArgs`

See [`tauri-commands.md`](./tauri-commands.md). All derive `TS` with `#[serde(rename_all = "camelCase")]` for consistent camelCase field names on the wire.

### `HolziError`

See [`tauri-commands.md`](./tauri-commands.md) → "Error type". Serialized as a tagged union with `kind` as the discriminator, matching Rust's `#[serde(tag = "kind")]`. The haex-crdt-mapped variants (`CrdtSqlite`, `CrdtIo`, `CrdtHlc`, `DeviceIdMismatch`, `CrdtSignatureVerificationFailed`, `CrdtUnexpectedSignatureUnderNoop`, `MigrationMissingFromSource`, `MigrationContentDrift`, `MigrationCompatibility`, `CrdtAlreadyInstalled`) plus the residual `CrdtInit` catch-all replace what earlier drafts flattened into a single opaque `CrdtInit { reason: String }`; see [`tauri-commands.md`](./tauri-commands.md) → "Error mapping" for the source-of-truth table. `DeviceIdMismatch` is not expected in normal operation — Holzi uses the pinned `DatabaseBootstrap` hook instead of the crate's removed `reconcile_device_id` arbitration.

## Generation flow

1. Rust structs live in `src-tauri/src/instances/types.rs`, `src-tauri/src/error.rs`, `src-tauri/src/pairing/types.rs`.
2. `#[derive(TS)]` + `#[ts(export)]` writes them to `src-tauri/bindings/` when `cargo test` runs (per `ts-rs` default; the `generate:ts-types` script wraps this).
3. A post-generation step (npm script or Rust build helper) copies or symlinks `src-tauri/bindings/` to `src/types/bindings/` so the frontend's `@bindings/*` alias resolves without walking out of `src/`.
4. CI has a check that the generated files under `src/types/bindings/` match a fresh regeneration — drift fails the build. haex-vault 2.x uses `test:constants` for the same purpose.

## Naming rules

- Rust struct names are `PascalCase`; TS exports the same names.
- Enum variants: `PascalCase` on both sides; `#[serde(tag = "type")]` for internally tagged discriminant `type` (data enums) and `#[serde(tag = "kind")]` for `HolziError` — chosen to keep error-vs-mode discriminators visually distinct in logs.
- Field names: Rust `snake_case`, wire `camelCase` (via `#[serde(rename_all = "camelCase")]`), TS `camelCase`.
