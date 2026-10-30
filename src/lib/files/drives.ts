// The drives of the file browser (spec 064): which drive a path lies on, how a path moves to a new
// mount point, and the nearest folder that still exists. Pure, so `node --test` checks it without
// Vue.
import type { FilesPlace } from './registry.ts'
import { parentPath, separatorOf } from './state.ts'

/** What these functions read of a drive (the `Drive` binding fits); kept structural so the checks
 * load no binding with imports of its own. */
export type DriveLike = {
  id: string
  label: string | null
  path: string | null
  state: 'mounted' | 'unmounted' | 'locked'
  size: number | null
}

/** Windows paths compare without case; everything else as it is. */
function fold(path: string): string {
  return separatorOf(path) === '\\' ? path.toLowerCase() : path
}

/** Whether `path` is `root` or lies below it, on a separator boundary. */
export function isUnder(path: string, root: string): boolean {
  const p = fold(path)
  const r = fold(root)
  if (p === r) return true
  const sep = separatorOf(root)
  return p.startsWith(r.endsWith(sep) ? r : `${r}${sep}`)
}

/** The mounted drive `path` lies on: the one with the longest mount path above it. */
export function driveOf<D extends DriveLike>(
  path: string,
  drives: readonly D[],
): D | null {
  let best: D | null = null
  for (const drive of drives) {
    if (drive.state !== 'mounted' || drive.path === null) continue
    if (!isUnder(path, drive.path)) continue
    if (best?.path && best.path.length >= drive.path.length) continue
    best = drive
  }
  return best
}

/** `p`, which lay below `droot`, below the new mount path `root`; `null` when it did not. */
export function remap(p: string, droot: string, root: string): string | null {
  if (!isUnder(p, droot)) return null
  const rest = p.slice(droot.length).replace(/^[/\\]+/, '')
  if (rest === '') return root
  const sep = separatorOf(root)
  return root.endsWith(sep) ? `${root}${rest}` : `${root}${sep}${rest}`
}

/**
 * `place` with the drive its path lies on (spec 064 FR-025, research R10): the deepest mounted drive
 * with a `key` that `driveOf` finds, never the system drive (it is always there at a restore). A
 * drive the place already names keeps it while no deeper one is listed: a drive just mounted may not
 * be in the list yet. Otherwise the place carries no drive.
 */
export function placeOnDrive(
  place: FilesPlace,
  drives: readonly (DriveLike & { key: string | null; kind?: string })[],
): FilesPlace {
  const { drive, droot, ...rest } = place
  if (place.source.kind !== 'device' || place.path === null) return rest
  const on = driveOf(
    place.path,
    drives.filter((candidate) => candidate.kind !== 'system'),
  )
  const found = on?.key && on.path ? { drive: on.key, droot: on.path } : null
  const kept =
    drive && droot && isUnder(place.path, droot) ? { drive, droot } : null
  const best =
    kept && (!found || kept.droot.length > found.droot.length) ? kept : found
  return best ? { ...rest, ...best } : rest
}

/** The name the sidebar shows: the label, else a name from the size. */
export function driveLabel(
  drive: DriveLike,
  unnamed: (size: number | null) => string,
): string {
  return drive.label || unnamed(drive.size)
}

/** Whether `drive` holds the open folder `current` (the deepest drive above it). */
export function isCurrentDrive(
  drive: DriveLike,
  current: string | null,
  drives: readonly DriveLike[],
): boolean {
  if (current === null) return false
  return driveOf(current, drives)?.id === drive.id
}

/**
 * The nearest folder at or above `path` that `exists` says is there (spec 044 edge case "Der
 * offene Ordner verschwindet", spec 064 research R10); `null` when none is, up to the root.
 */
export async function nearestExisting(
  path: string,
  exists: (path: string) => Promise<boolean>,
): Promise<string | null> {
  let at: string | null = parentPath(path)
  while (at !== null) {
    if (await exists(at)) return at
    at = parentPath(at)
  }
  return null
}
