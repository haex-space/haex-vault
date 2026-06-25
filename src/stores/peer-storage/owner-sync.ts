import { invoke } from '@tauri-apps/api/core'

/** Start owner-device P2P sync (idempotent; no-op Rust-side if no other devices). */
export const startOwnerSyncAsync = (): Promise<void> => invoke('owner_sync_start')

/** Stop all owner-device sync loops. */
export const stopOwnerSyncAsync = (): Promise<void> => invoke('owner_sync_stop')

/** Wake all running owner-device sync loops for an immediate cycle. */
export const forceOwnerSyncAsync = (): Promise<void> => invoke('owner_sync_force')

/** Default-ON: only an explicit 'false' disables autostart (mirrors peerStorageAutostart). */
export const ownerSyncAutostartEnabled = (value: string | undefined | null): boolean =>
  value !== 'false'
