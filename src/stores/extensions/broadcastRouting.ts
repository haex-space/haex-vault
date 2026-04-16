/**
 * Broadcast Routing — security-critical target selection + dispatch.
 *
 * These helpers decide which iframe instances receive a given broadcast and
 * perform the actual `postMessage` dispatch. They are extracted from the
 * store so the routing rules can be exercised by integration tests with
 * real iframes, independent of Pinia, Tauri, or the Nuxt runtime.
 *
 * Contracts enforced here:
 *   - `readerExtensionIds` on file-change events is the *only* authorisation
 *     signal — missing/empty ⇒ zero fan-out (fail-closed).
 *   - `extensionId` on shell events scopes delivery to the session owner
 *     only; no other extension sees stdout, not even those sharing an origin.
 *   - The `readerExtensionIds` field is stripped before forwarding so an
 *     iframe cannot learn which *other* extensions share the same permission.
 *   - An iframe without a live `contentWindow` is silently skipped.
 */

import { HAEXTENSION_EVENTS, type FileChangePayload } from '@haex-space/vault-sdk'

export interface RoutableInstance {
  extension: { id: string }
}

/** Iframe-like — has a `contentWindow` that can receive `postMessage`. */
export interface RoutableIframe {
  contentWindow: { postMessage: (message: unknown, targetOrigin: string) => void } | null
}

export interface FileChangedBroadcastInput extends FileChangePayload {
  readerExtensionIds?: string[]
}

export interface BroadcastResult<TIframe> {
  /** Iframes that were actually posted to. */
  postedTo: TIframe[]
  /** The message that was broadcast (null if no fan-out happened). */
  message: Record<string, unknown> | null
}

/**
 * Dispatch a file-change event to exactly those registered iframes whose
 * extension appears in `readerExtensionIds`. The readers list is server-side
 * computed against DB + session permissions and is treated as the ground
 * truth for this event.
 */
export const dispatchFileChangedBroadcast = <
  TIframe extends RoutableIframe,
  TInstance extends RoutableInstance,
>(
  payload: FileChangedBroadcastInput,
  entries: Iterable<readonly [TIframe, TInstance]>,
  now: () => number = Date.now,
): BroadcastResult<TIframe> => {
  const readers = payload.readerExtensionIds ?? []
  if (readers.length === 0) {
    return { postedTo: [], message: null }
  }

  const message: Record<string, unknown> = {
    type: HAEXTENSION_EVENTS.FILE_CHANGED,
    ruleId: payload.ruleId,
    changeType: payload.changeType,
    path: payload.path,
    timestamp: now(),
  }

  const readerSet = new Set(readers)
  const postedTo: TIframe[] = []
  for (const [iframe, instance] of entries) {
    if (!readerSet.has(instance.extension.id)) continue
    if (!iframe.contentWindow) continue
    iframe.contentWindow.postMessage(message, '*')
    postedTo.push(iframe)
  }

  return { postedTo, message }
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
 * receive (or need to see) metadata that only matters for routing.
 */
export const dispatchShellEventBroadcast = <
  TIframe extends RoutableIframe,
  TInstance extends RoutableInstance,
>(
  type: string,
  payload: ShellEventBroadcastInput,
  entries: Iterable<readonly [TIframe, TInstance]>,
  now: () => number = Date.now,
): BroadcastResult<TIframe> => {
  const { extensionId, ...rest } = payload
  if (!extensionId) {
    return { postedTo: [], message: null }
  }

  const message: Record<string, unknown> = {
    type,
    ...rest,
    timestamp: now(),
  }

  const postedTo: TIframe[] = []
  for (const [iframe, instance] of entries) {
    if (instance.extension.id !== extensionId) continue
    if (!iframe.contentWindow) continue
    iframe.contentWindow.postMessage(message, '*')
    postedTo.push(iframe)
  }

  return { postedTo, message }
}
