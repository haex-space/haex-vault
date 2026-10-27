// The split view of the file browser (spec 062): two panels side by side. Pure, so
// `scripts/check-files-split.ts` runs it without vue. The state lives in the tab location
// (`contracts/split-location.md`): the plain keys (`p`, `open`, `q`, `t`, `s`, `d` and the route's
// source) belong to the *active* panel, `split`, `side` and `o.*` mirror the rest. Reading drops what
// it does not know, and writing leaves the plain keys alone.
import type { SourceRef } from '../../types/bindings/SourceRef.ts'

import {
  createHistory,
  replace,
  type TabHistory,
  type TabLocation,
} from '../wm/navigation.ts'
import {
  type FilesPlace,
  OPAQUE_ID,
  parseFilesPlace,
  PEER_PREFIX,
  SHARE_PREFIX,
} from './registry.ts'

/** Which side of the surface a panel stands on. */
export type SplitSide = 'left' | 'right'

/** What the frame knows about the split (data-model.md "SplitState"). */
export type SplitState = {
  /** The tab shows two panels (`split=1`). */
  on: boolean
  /** The side the *active* panel stands on; it stays as the side of the remaining panel when the
   * split is off. */
  side: SplitSide
  /** The place of the active panel: the plain keys of the tab location. */
  active: FilesPlace
  /** The place of the other panel, remembered while the split is off; `null` if there never was one.
   * It never carries a file open in the viewer: only the active panel navigates. */
  other: FilesPlace | null
}

/** Each panel is at least this wide (px); two panels need twice that (research R9). */
export const SPLIT_PANEL_MIN_WIDTH = 360

/** Whether two panels fit into a panel area `width` px wide (FR-022). */
export function fitsSplit(width: number): boolean {
  return width >= 2 * SPLIT_PANEL_MIN_WIDTH
}

/** The query keys of the other panel's place, without the `o.` prefix. */
const OTHER_PASSED = ['q', 't', 's', 'd'] as const

const SPLIT_KEYS = [
  'split',
  'side',
  'o.src',
  'o.p',
  ...OTHER_PASSED.map((key) => `o.${key}`),
]

/** `device`, `storage:<id>`, `peer:<id>` or `share:<id>`: a source as one string of the location. */
export function sourceToken(source: SourceRef): string {
  if (source.kind === 'device') return 'device'
  const { storageId } = source
  return storageId.startsWith(PEER_PREFIX) || storageId.startsWith(SHARE_PREFIX)
    ? storageId
    : `storage:${storageId}`
}

/** The source of `token`; `undefined` for anything `sourceToken` does not make. */
export function sourceFromToken(
  token: string | undefined,
): SourceRef | undefined {
  if (token === 'device') return { kind: 'device' }
  const match = /^(storage|peer|share):(.+)$/.exec(token ?? '')
  const id = match?.[2]
  if (!match || !id || !OPAQUE_ID.test(id)) return undefined
  return {
    kind: 'storage',
    storageId: match[1] === 'storage' ? id : `${match[1]}:${id}`,
  }
}

/** The place without a file open in the viewer. */
function withoutOpen(place: FilesPlace): FilesPlace {
  const { open: _open, ...rest } = place
  return rest
}

/** The split state of a tab location; a location without split keys is a tab without split. */
export function readSplit(location: TabLocation): SplitState {
  const query = location.query
  const active = parseFilesPlace(location) ?? {
    source: { kind: 'device' } as const,
    path: null,
  }
  const source = sourceFromToken(query['o.src'])
  let other: FilesPlace | null = null
  if (source) {
    other = { source, path: query['o.p'] || null }
    for (const key of OTHER_PASSED) {
      const value = query[`o.${key}`]
      if (value) other[key] = value
    }
  }
  return {
    on: query.split === '1',
    side: query.side === 'r' ? 'right' : 'left',
    active,
    other,
  }
}

/** The split keys of `state`: only what differs from "no split", never an empty value. */
export function splitQuery(state: SplitState): Record<string, string> {
  const query: Record<string, string> = {}
  if (state.on) query.split = '1'
  if (state.side === 'right') query.side = 'r'
  const other = state.other
  if (other) {
    query['o.src'] = sourceToken(other.source)
    if (other.path !== null) query['o.p'] = other.path
    for (const key of OTHER_PASSED) {
      const value = other[key]
      if (value) query[`o.${key}`] = value
    }
  }
  return query
}

/** `location` without any split key: the plain place of the active panel. */
export function stripSplit(location: TabLocation): TabLocation {
  const query = Object.fromEntries(
    Object.entries(location.query).filter(([key]) => !SPLIT_KEYS.includes(key)),
  )
  return { path: location.path, query }
}

/** `location` with the split keys of `state`; its plain keys stay as they are (contract rule 3). */
export function withSplit(
  location: TabLocation,
  state: SplitState,
): TabLocation {
  const plain = stripSplit(location)
  return { path: plain.path, query: { ...plain.query, ...splitQuery(state) } }
}

/** Turns the split on: the other panel shows the remembered place, or the active one (FR-005). */
export function enableSplit(state: SplitState): SplitState {
  return {
    ...state,
    on: true,
    other: state.other ?? {
      source: state.active.source,
      path: state.active.path,
    },
  }
}

/** Turns the split off: the active panel stays the only view, the other place is remembered
 * (FR-006), the side stays. */
export function disableSplit(state: SplitState): SplitState {
  return { ...state, on: false }
}

/** The other panel becomes the active one: the places change roles and the side flips, so no panel
 * moves on the surface. Without another place nothing changes. */
export function swapActive(state: SplitState): SplitState {
  if (!state.other) return state
  return {
    ...state,
    side: state.side === 'left' ? 'right' : 'left',
    active: state.other,
    other: withoutOpen(state.active),
  }
}

/** The history a panel brings along when it becomes the active one: its lane with the current entry
 * set to `location` (the place the frame knows, with the split keys of the swap), or a history of
 * that one entry when the panel has none yet (research R2). */
export function incomingHistory(
  lane: TabHistory | null,
  location: TabLocation,
): TabHistory {
  return lane ? replace(lane, location) : createHistory(location)
}

/** Whether this platform offers two panels at all: not on a phone (FR-021). Until the platform is
 * known, and in a plain browser preview, it does. */
export function splitAvailable(platform: string | undefined): boolean {
  return platform !== 'android' && platform !== 'ios'
}
