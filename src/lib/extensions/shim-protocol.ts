// Messages between holzi and the frame shim of an extension (spec 017, contracts/bridge.md
// §Rahmen-Shim). Pure: `useExtensionFrame` applies the effects to the tab, `scripts/check-extensions-
// shim.ts` tests them. Every shim message comes from the extension's realm and can be forged, so
// each effect only ever touches the frame's own tab, and a shortcut runs only if holzi sent it.
import {
  formatLocation,
  parseLocation,
  type TabLocation,
} from '../wm/navigation.ts'
import type { ShortcutFields } from '../wm/keybindings.ts'

/** Longest tab title an extension can set. */
export const MAX_TITLE_LENGTH = 200

export type ShimEffect =
  | { kind: 'hello'; fresh: boolean }
  | { kind: 'navigate'; location: TabLocation; replace: boolean }
  | { kind: 'title'; title: string }
  | { kind: 'closeGuard'; active: boolean }
  | { kind: 'close' }
  | { kind: 'focus' }
  | { kind: 'shortcut'; actionId: string }

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

/** The effect of one shim message, or `null` for anything malformed or unknown. */
export function readShimMessage(
  data: unknown,
  sentShortcuts: readonly ShortcutFields[],
): ShimEffect | null {
  if (!isRecord(data)) return null
  switch (data.type) {
    case 'hello':
      return typeof data.fresh === 'boolean'
        ? { kind: 'hello', fresh: data.fresh }
        : null
    case 'nav': {
      if (typeof data.path !== 'string' || !data.path.startsWith('/'))
        return null
      const query = typeof data.query === 'string' ? data.query : ''
      return {
        kind: 'navigate',
        location: parseLocation(query ? `${data.path}?${query}` : data.path),
        replace: data.replace === true,
      }
    }
    case 'title':
      return typeof data.text === 'string' &&
        data.text.length <= MAX_TITLE_LENGTH
        ? { kind: 'title', title: data.text }
        : null
    case 'closeGuard':
      return typeof data.active === 'boolean'
        ? { kind: 'closeGuard', active: data.active }
        : null
    case 'close':
      return { kind: 'close' }
    case 'focus':
      return { kind: 'focus' }
    case 'shortcut':
      return typeof data.id === 'string' &&
        sentShortcuts.some((s) => s.id === data.id)
        ? { kind: 'shortcut', actionId: data.id }
        : null
    default:
      return null
  }
}

/** holzi → shim: show `location` (Back and Forward in holzi). */
export function navigateMessage(location: TabLocation): {
  type: 'navigate'
  path: string
  query: string
} {
  const formatted = formatLocation(location)
  const at = formatted.indexOf('?')
  return at === -1
    ? { type: 'navigate', path: formatted, query: '' }
    : {
        type: 'navigate',
        path: formatted.slice(0, at),
        query: formatted.slice(at + 1),
      }
}

/** holzi → shim: the first message, sent with holzi's own port. */
export function initMessage(shortcuts: readonly ShortcutFields[]): {
  type: 'holzi:frame:init'
  shortcuts: readonly ShortcutFields[]
} {
  return { type: 'holzi:frame:init', shortcuts }
}

/** Whether two locations are the same view (no history entry for an echo). */
export function sameLocation(a: TabLocation, b: TabLocation): boolean {
  return formatLocation(a) === formatLocation(b)
}
