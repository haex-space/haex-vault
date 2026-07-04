import { invoke } from '@tauri-apps/api/core'

/**
 * Wire-level types for the `share_storage_backend` and `revoke_storage_share`
 * Tauri commands. Field casing mirrors the Rust
 * `#[serde(rename_all = "camelCase")]` attribute on the request / response
 * structs — see `src-tauri/src/remote_storage/share_command/mod.rs` and
 * `src-tauri/src/remote_storage/revoke_command/mod.rs`.
 */

/** Provider identity accepted by the share flow. Matches Rust `ProviderKind`
 * (serialised lowercase). MinIO is accepted at the enum layer but rejected by
 * the share flow with `UnsupportedProvider` — see plan §Errors. */
export type StorageProviderKind = 'aws' | 'wasabi' | 'minio'

/** Frontend-provided IAM-admin credential. Only populated on a retry after
 * the initial call returned `IamAdminCredMissing` — the vault stores it and
 * then proceeds with the share flow. */
export interface IamAdminCredHint {
  accessKeyId: string
  secretAccessKey: string
  providerType: StorageProviderKind
}

/** Arguments to `share_storage_backend`. Matches Rust
 * `ShareStorageBackendArgs`. */
export interface ShareStorageBackendArgs {
  /** Owner-side `haex_s3_backends.id` to share. */
  storageId: string
  /** Target space's id. */
  spaceId: string
  /** Key-prefix scope. `undefined` = whole bucket. */
  prefix?: string
  /** Single-object scope. v1 rejects this with
   *  `ObjectScopeNotYetSupported`. */
  objectKey?: string
  /** Bitmap over LIST | GET | PUT | DELETE. Must be non-zero. */
  accessFlags: number
  /** Populated on retry after `IamAdminCredMissing`. */
  iamAdminCredHint?: IamAdminCredHint
}

/** The newly-written (or existing, on idempotent re-invocation) shared row as
 * returned by `share_storage_backend`. Matches Rust `SharedStorageBackend`. */
export interface SharedStorageBackend {
  id: string
  type: string
  name: string
  /** Scoped IAM user name — the frontend surfaces it so a subsequent revoke
   *  can find the right user to tear down. */
  iamUserName: string
}

/** Discriminant on the wire-format for `StorageError` (Rust
 *  `#[serde(tag = "type", content = "details")]`). See
 *  `src-tauri/src/remote_storage/error.rs`. */
export type StorageErrorType =
  | 'BackendNotFound'
  | 'ConnectionFailed'
  | 'UploadFailed'
  | 'DownloadFailed'
  | 'DeleteFailed'
  | 'ObjectNotFound'
  | 'InvalidConfig'
  | 'DatabaseError'
  | 'Internal'
  | 'InvalidArgs'
  | 'StorageNotFound'
  | 'IamAdminCredMissing'
  | 'IamAdminInsufficient'
  | 'UnsupportedProvider'
  | 'IamOperationFailed'
  | 'ObjectScopeNotYetSupported'
  | 'NotAShareRow'
  | 'ParentBackendMissing'

/** Tauri-serialised `StorageError`. Frontend routes off `type` (e.g.
 *  `IamAdminCredMissing` → open IAM-cred modal). `details` field casing is
 *  Rust struct-field default (snake_case) because the enum has no
 *  `rename_all` attribute. */
export interface StorageError {
  type: StorageErrorType
  details?: Record<string, unknown>
}

/** Composable wrapping the two Tauri commands used by the S3-bucket-sharing
 *  flow. Kept thin — no business logic, no toasts, no store writes. Callers
 *  layer UX (modals, toasts, listing refresh) on top. */
export function useStorageSharing() {
  const shareBackend = async (
    args: ShareStorageBackendArgs,
  ): Promise<SharedStorageBackend> => {
    return await invoke<SharedStorageBackend>('share_storage_backend', { args })
  }

  const revokeBackend = async (sharedBackendId: string): Promise<void> => {
    await invoke<void>('revoke_storage_share', { sharedBackendId })
  }

  return { shareBackend, revokeBackend }
}
