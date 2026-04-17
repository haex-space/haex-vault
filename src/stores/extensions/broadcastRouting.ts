/**
 * Broadcast Routing — security-critical target selection + dispatch.
 *
 * These helpers decide which iframe instances receive a given broadcast and
 * perform the actual `postMessage` dispatch. Extracted from the store so
 * the routing rules can be exercised by integration tests with real
 * MessagePorts, independent of Pinia, Tauri, or the Nuxt runtime.
 *
 * Transport note: as of SDK 3.0 all extension fan-out flows through a
 * dedicated `MessagePort` per iframe (established during registration).
 * Dispatchers post on `port1`; if the extension's SDK hasn't finished its
 * handshake yet, the entry buffers the message and the store flushes on
 * PORT_READY. This keeps the early-events-during-startup case deterministic.
 *
 * Contracts enforced here:
 *   - `readerExtensionIds` on file-change events is the *only* authorisation
 *     signal — missing/empty ⇒ zero fan-out (fail-closed).
 *   - `extensionId` on shell events scopes delivery to the session owner
 *     only; no other extension sees stdout, not even those sharing an origin.
 *   - `readerExtensionIds` / routing `extensionId` are stripped from the
 *     forwarded payload so iframes can never learn who else has access.
 */

import { HAEXTENSION_EVENTS, type FileChangePayload } from '@haex-space/vault-sdk'

/** A routable entry: extension identity + port + ready/buffer state. */
export interface RoutableEntry<TPort extends RoutablePort = RoutablePort> {
  instance: { extension: { id: string } }
  port: TPort
  ready: boolean
  buffer: Array<Record<string, unknown>>
}

/** Minimal MessagePort surface — `postMessage` is all the router touches. */
export interface RoutablePort {
  postMessage: (message: unknown) => void
}

export interface FileChangedBroadcastInput extends FileChangePayload {
  readerExtensionIds?: string[]
}

export interface BroadcastResult<TEntry> {
  /** Entries the message was actually delivered to (posted on the live port). */
  postedTo: TEntry[]
  /** Entries that buffered the message because their port is not yet ready. */
  buffered: TEntry[]
  /** The message that was broadcast (null if no fan-out happened). */
  message: Record<string, unknown> | null
}

/**
 * Dispatch a file-change event to the entries whose extension appears in
 * `readerExtensionIds`. The readers list is server-side computed against
 * DB + session permissions and is treated as the ground truth.
 *
 * Delivery rule:
 *   - entry.ready === true  → `entry.port.postMessage(message)`
 *   - entry.ready === false → `entry.buffer.push(message)` (store will flush on ACK)
 */
export const dispatchFileChangedBroadcast = <TEntry extends RoutableEntry>(
  payload: FileChangedBroadcastInput,
  entries: Iterable<TEntry>,
  now: () => number = Date.now,
): BroadcastResult<TEntry> => {
  const readers = payload.readerExtensionIds ?? []
  if (readers.length === 0) {
    return { postedTo: [], buffered: [], message: null }
  }

  const message: Record<string, unknown> = {
    type: HAEXTENSION_EVENTS.FILE_CHANGED,
    ruleId: payload.ruleId,
    changeType: payload.changeType,
    path: payload.path,
    timestamp: now(),
  }

  const readerSet = new Set(readers)
  const postedTo: TEntry[] = []
  const buffered: TEntry[] = []
  for (const entry of entries) {
    if (!readerSet.has(entry.instance.extension.id)) continue
    deliver(entry, message, postedTo, buffered)
  }

  return { postedTo, buffered, message }
}

export interface ShellEventBroadcastInput {
  extensionId: string
  [field: string]: unknown
}

/**
 * Dispatch a shell PTY event only to iframes of the owning extension.
 * The owning `extensionId` is set by the Rust PTY layer and is always
 * derived from the session's stored owner.
 *
 * `extensionId` is stripped from the forwarded payload — iframes should not
 * receive metadata that only matters for routing.
 */
export const dispatchShellEventBroadcast = <TEntry extends RoutableEntry>(
  type: string,
  payload: ShellEventBroadcastInput,
  entries: Iterable<TEntry>,
  now: () => number = Date.now,
): BroadcastResult<TEntry> => {
  const { extensionId, ...rest } = payload
  if (!extensionId) {
    return { postedTo: [], buffered: [], message: null }
  }

  const message: Record<string, unknown> = {
    type,
    ...rest,
    timestamp: now(),
  }

  const postedTo: TEntry[] = []
  const buffered: TEntry[] = []
  for (const entry of entries) {
    if (entry.instance.extension.id !== extensionId) continue
    deliver(entry, message, postedTo, buffered)
  }

  return { postedTo, buffered, message }
}

const deliver = <TEntry extends RoutableEntry>(
  entry: TEntry,
  message: Record<string, unknown>,
  postedTo: TEntry[],
  buffered: TEntry[],
): void => {
  if (entry.ready) {
    entry.port.postMessage(message)
    postedTo.push(entry)
  }
  else {
    entry.buffer.push(message)
    buffered.push(entry)
  }
}
