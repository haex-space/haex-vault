/**
 * Typed Rust→TS event layer.
 *
 * Event name strings are the single source of truth in
 * src/constants/eventNames.json, which is also read by build.rs to
 * generate the matching Rust constants. Import RUST_EVENTS here instead
 * of using string literals so a rename propagates to both sides.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import eventNames from '@/constants/eventNames.json'

// ---------------------------------------------------------------------------
// Event name registry — derived from the shared JSON, not hardcoded strings
// ---------------------------------------------------------------------------

export const RUST_EVENTS = {
  peerStorageStateChanged: eventNames.peer.storageStateChanged,
  localSyncCompleted: eventNames.localSync.completed,
  localSyncError: eventNames.localSync.error,
} as const

// ---------------------------------------------------------------------------
// Payload types — mirror the serde_json payloads emitted on the Rust side
// ---------------------------------------------------------------------------

export interface PeerStorageStateEvent {
  running: boolean
  /** Why the state changed. */
  reason: 'endpoint-closed' | 'user-stopped'
  /** How long the endpoint was alive before it closed, in seconds. */
  uptimeSecs: number
}

export interface LocalSyncCompletedEvent {
  spaceId: string
  tables: string[]
}

export interface LocalSyncErrorEvent {
  spaceId: string
  error: string
  reconnecting: boolean
  endpointClosed: boolean
  attempt: number
}

// ---------------------------------------------------------------------------
// RustEventGroup — registers multiple listeners that clean up together
// ---------------------------------------------------------------------------

/**
 * Manages a set of Tauri event listeners that are registered and disposed
 * as a unit. Typical usage:
 *
 *   const events = new RustEventGroup()
 *   await events.on<LocalSyncCompletedEvent>(RUST_EVENTS.localSyncCompleted, handler)
 *   // later:
 *   events.dispose()
 */
export class RustEventGroup {
  private unlisteners: UnlistenFn[] = []

  async on<T>(
    name: string,
    handler: (payload: T) => void | Promise<void>,
  ): Promise<void> {
    const unlisten = await listen<T>(name, event => handler(event.payload))
    this.unlisteners.push(unlisten)
  }

  dispose(): void {
    for (const unlisten of this.unlisteners) unlisten()
    this.unlisteners = []
  }
}
