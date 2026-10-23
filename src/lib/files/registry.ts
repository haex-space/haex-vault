// File browser registry (spec 044, data-model.md "Ort eines Tabs", research R13): where a tab
// stands, as a tab location, so session restore (spec 022) brings it back. Pure — the Vue side
// attaches the component in `components/wm/appRoutes.ts`.
import type { SourceRef } from '../../types/bindings/SourceRef.ts'

import type { TabLocation } from '../wm/navigation.ts'

/** The app id in `lib/wm/apps.ts`. */
export const FILES_APP_ID = 'system.files'

/** Where a tab of the file browser stands. `path` is `null` at the start (the home folder). */
export type FilesPlace = {
  source: SourceRef
  path: string | null
  /** The file open in the viewer, by name within `path`. */
  open?: string
  /** The search from `path` down (FR-027). */
  q?: string
  /** The filter (`lib/files/filters.ts`): types, size range, time range. */
  t?: string
  s?: string
  d?: string
}

/** Route patterns below `/`: the start, the device, a storage and another own device (spec 056).
 * One mounted root component reads the location, so moving between folders never remounts the
 * browser. */
export const FILES_ROUTE_CHILDREN = [
  { path: '' },
  { path: 'device', titleKey: 'wm.files.device' },
  { path: 'storage/:id', titleKey: 'wm.files.storage' },
  { path: 'peer/:id', titleKey: 'wm.files.peer' },
] as const

/** The query keys a place may carry; anything else is dropped. */
const KEYS = ['p', 'open', 'q', 't', 's', 'd'] as const

/** The keys a place keeps as they are. */
const PASSED = ['q', 't', 's', 'd'] as const

const OPAQUE_ID = /^[A-Za-z0-9-]{1,64}$/

/** The prefix of another own device's storage id (spec 056). */
export const PEER_PREFIX = 'peer:'

/** Whether `storageId` names another own device. */
export function isPeerStorage(storageId: string): boolean {
  return storageId.startsWith(PEER_PREFIX)
}

/** The tab location of `place`. */
export function filesLocation(place: FilesPlace): TabLocation {
  const query: Record<string, string> = {}
  if (place.path !== null) query.p = place.path
  if (place.open) query.open = place.open
  for (const key of PASSED) {
    const value = place[key]
    if (value) query[key] = value
  }
  const path =
    place.source.kind === 'device'
      ? '/device'
      : isPeerStorage(place.source.storageId)
        ? `/peer/${place.source.storageId.slice(PEER_PREFIX.length)}`
        : `/storage/${place.source.storageId}`
  return { path, query }
}

/** The place of a tab location; `undefined` for a location the browser does not know. */
export function parseFilesPlace(location: TabLocation): FilesPlace | undefined {
  const query: Record<string, string> = {}
  for (const key of KEYS) {
    const value = location.query[key]
    if (typeof value === 'string' && value !== '') query[key] = value
  }
  const path = query.p ?? null
  const open = query.open && !/[\\/]/.test(query.open) ? query.open : undefined
  if (location.path === '/' || location.path === '') {
    return { source: { kind: 'device' }, path: null }
  }
  const withOpen = (place: FilesPlace): FilesPlace => {
    const out: FilesPlace = open ? { ...place, open } : { ...place }
    for (const key of PASSED) {
      const value = query[key]
      if (value) out[key] = value
    }
    return out
  }
  if (location.path === '/device') {
    return withOpen({ source: { kind: 'device' }, path })
  }
  const storage = /^\/storage\/([^/]+)$/.exec(location.path)
  if (storage?.[1] && OPAQUE_ID.test(storage[1])) {
    return withOpen({
      source: { kind: 'storage', storageId: storage[1] },
      path,
    })
  }
  const peer = /^\/peer\/([^/]+)$/.exec(location.path)
  if (peer?.[1] && OPAQUE_ID.test(peer[1])) {
    return withOpen({
      source: { kind: 'storage', storageId: `${PEER_PREFIX}${peer[1]}` },
      path,
    })
  }
  return undefined
}
