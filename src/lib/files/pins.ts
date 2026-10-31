// The pinned folders of the file browser sidebar (spec 071): shortcuts in "Orte" to any folder of any
// source: this device, a cloud storage, another own device, a contact's share. Pure, so `scripts/check-files-pins.ts` runs it without vue. A pin is stored as
// JSON in a device preference (`useFilesPrefs`); what the file holds is never trusted.
import type { SourceRef } from '../../types/bindings/SourceRef.ts'

import type { PinCommand } from './menus.ts'
import { breadcrumbs } from './state.ts'
import { sameSource } from './clipboard.ts'
import { sourceFromToken, sourceToken } from './split.ts'

export type FilesPin = { source: SourceRef; path: string }

/** Every source the sidebar lists may be pinned (FR-002); a `device:` storage only names an encrypted
 * folder of this device inside holzi and is never browsed as a source of its own. */
export function pinnableSource(source: SourceRef): boolean {
  if (source.kind === 'device') return true
  return (
    source.kind === 'storage' &&
    typeof source.storageId === 'string' &&
    source.storageId !== '' &&
    !source.storageId.startsWith('device:')
  )
}

const same = (a: FilesPin, b: FilesPin) =>
  a.path === b.path && sameSource(a.source, b.source)

/** The pins stored in `raw`; whatever is not a pin of a known shape is dropped. */
export function parsePins(raw: string | null | undefined): FilesPin[] {
  let stored: unknown
  try {
    stored = JSON.parse(raw ?? '[]')
  } catch {
    return []
  }
  if (!Array.isArray(stored)) return []
  const pins: FilesPin[] = []
  for (const item of stored) {
    const source = sourceFromToken(item?.s)
    const path = item?.p
    if (!source || !pinnableSource(source)) continue
    if (typeof path !== 'string' || !path) continue
    pins.push({ source, path })
  }
  return pins
}

export function formatPins(pins: readonly FilesPin[]): string {
  return JSON.stringify(
    pins.map((pin) => ({ s: sourceToken(pin.source), p: pin.path })),
  )
}

/** Whether `pin` is pinned already. */
export function isPinned(pins: readonly FilesPin[], pin: FilesPin): boolean {
  return pins.some((candidate) => same(candidate, pin))
}

/** `pins` with `added` at the end; a pin that is there already, or cannot be pinned, is skipped. */
export function addPins(
  pins: readonly FilesPin[],
  added: readonly FilesPin[],
): FilesPin[] {
  const next = [...pins]
  for (const pin of added)
    if (pinnableSource(pin.source) && !isPinned(next, pin)) next.push(pin)
  return next
}

export function removePin(
  pins: readonly FilesPin[],
  pin: FilesPin,
): FilesPin[] {
  return pins.filter((candidate) => !same(candidate, pin))
}

/** What the sidebar names a pin: the folder's name, or `rootName` (the storage's) for a root. */
export function pinLabel(pin: FilesPin, rootName: string): string {
  const name = breadcrumbs(pin.path).at(-1)?.name
  return !name || name === '/' ? rootName : name
}

/** What "Orte" holds beyond the known places of the device: the added folders, and the known places
 * the user removed (paths of this device). */
export type FilesPlaces = { pins: FilesPin[]; hidden: string[] }

const isKnown = (pin: FilesPin, knownPaths: readonly string[]) =>
  pin.source.kind === 'device' && knownPaths.includes(pin.path)

/** The removed known places stored in `raw`; whatever is not a path is dropped. */
export function parseHidden(raw: string | null | undefined): string[] {
  try {
    const stored: unknown = JSON.parse(raw ?? '[]')
    return Array.isArray(stored)
      ? stored.filter(
          (path): path is string => typeof path === 'string' && !!path,
        )
      : []
  } catch {
    return []
  }
}

/** What the context menu offers for `pin`: nothing for a source that cannot be added, else to add it
 * or, if "Orte" shows it (an added folder, a known place not removed), to remove it (FR-003). */
export function offerPin(
  places: FilesPlaces,
  pin: FilesPin,
  knownPaths: readonly string[],
): PinCommand | undefined {
  if (!pinnableSource(pin.source)) return undefined
  if (isKnown(pin, knownPaths))
    return places.hidden.includes(pin.path) ? 'pin' : 'unpin'
  return isPinned(places.pins, pin) ? 'unpin' : 'pin'
}

/** `places` with `added` in "Orte": a removed known place comes back, any other folder is pinned. */
export function addPlaces(
  places: FilesPlaces,
  added: readonly FilesPin[],
  knownPaths: readonly string[],
): FilesPlaces {
  const back = added.filter((pin) => isKnown(pin, knownPaths))
  return {
    pins: addPins(
      places.pins,
      added.filter((pin) => !isKnown(pin, knownPaths)),
    ),
    hidden: places.hidden.filter(
      (path) => !back.some((pin) => pin.path === path),
    ),
  }
}

/** `places` without `removed`: a known place is hidden, an added folder unpinned (FR-004). */
export function removePlace(
  places: FilesPlaces,
  removed: FilesPin,
  knownPaths: readonly string[],
): FilesPlaces {
  if (!isKnown(removed, knownPaths))
    return { ...places, pins: removePin(places.pins, removed) }
  return places.hidden.includes(removed.path)
    ? places
    : { ...places, hidden: [...places.hidden, removed.path] }
}
