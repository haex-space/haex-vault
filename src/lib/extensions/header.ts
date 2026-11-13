// The frontend's side of an extension's header (spec 090-app-header-bar, contracts/extension-header.md,
// research R9): what to accept from the host's `extension-header-set` event, the message that carries a
// header event to the extension, and the table of pictures kept for the frame session. Pure — relative
// `.ts` imports, so the Node check script (`scripts/check-extensions-header.ts`) loads it.
import type { WmHeaderControl } from '../../types/bindings/WmHeaderControl.ts'
import type { WmHeaderDescription } from '../../types/bindings/WmHeaderDescription.ts'
import type { WmHeaderEvent } from '../../types/bindings/WmHeaderEvent.ts'

/** The SDK's event for a used control. */
export const HEADER_EVENT = 'haextension:header:event'

/** `{frame, seq, header}` of the host's event, as the frame composable receives it. */
export type HeaderSetPayload = {
  frame: string
  seq: number
  header: WmHeaderDescription
}

/**
 * The payload if it is for this frame and well formed, else `null`. The frame id comes from holzi's
 * session, never from the extension, so a payload naming another frame changes nothing (FR-019).
 */
export function acceptHeaderSet(
  ownFrame: string | null,
  payload: unknown,
): { seq: number; header: WmHeaderDescription } | null {
  if (ownFrame === null || typeof payload !== 'object' || payload === null)
    return null
  const { frame, seq, header } = payload as Partial<HeaderSetPayload>
  if (frame !== ownFrame) return null
  if (typeof seq !== 'number' || !Number.isFinite(seq)) return null
  if (typeof header !== 'object' || header === null || Array.isArray(header))
    return null
  return { seq, header }
}

/** What the SDK's port gets when the user uses a control. */
export function headerEventMessage(
  event: WmHeaderEvent,
  timestamp: number,
): { type: typeof HEADER_EVENT; data: WmHeaderEvent; timestamp: number } {
  return { type: HEADER_EVENT, data: event, timestamp }
}

/** The picture paths a description names (controls, menu entries, options). */
export function iconNames(header: WmHeaderDescription): Set<string> {
  const names = new Set<string>()
  const note = (icon: string | undefined) => {
    if (icon) names.add(icon)
  }
  const controls: WmHeaderControl[] = [
    ...(header.start ?? []),
    ...(header.center ?? []),
    ...(header.end ?? []),
  ]
  for (const control of controls) {
    note(control.icon)
    if (control.type === 'menu')
      for (const item of control.items)
        if (item.type === 'item') note(item.icon)
    if (control.type === 'choice')
      for (const option of control.options) note(option.icon)
  }
  return names
}

/**
 * Merges the pictures a description brings into the table kept for the frame session and keeps
 * only those the description names: Rust forgets the ones a new description no longer names, so
 * both sides always agree on what was sent. Returns the new table and the description with that
 * table as its `icons`.
 */
export function applyIcons(
  table: Readonly<Record<string, string>>,
  header: WmHeaderDescription,
): { table: Record<string, string>; header: WmHeaderDescription } {
  const merged = { ...table, ...(header.icons ?? {}) }
  const kept: Record<string, string> = {}
  for (const name of iconNames(header)) {
    const url = merged[name]
    if (url !== undefined) kept[name] = url
  }
  return { table: kept, header: { ...header, icons: kept } }
}
