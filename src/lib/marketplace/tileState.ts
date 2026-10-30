// What a catalog tile or the details of an entry offer (spec 069, FR-004, FR-018): decided from
// what the host reports about the entry, never from the marketplace's own words.
//
// The shapes of the generated bindings (`CatalogEntry`, `CatalogVersion`) as far as the rules use
// them, declared here so the check scripts load this file without the bindings.
type CatalogVersion = {
  compatible: boolean
  incompatibleReason?: string
  bound?: string
}

type CatalogEntry = {
  installed?: { version: string }
  updateTo?: string
  latestCompatible?: string
  versions: readonly CatalogVersion[]
}

export type BlockedReason =
  'needs_newer' | 'needs_older' | 'no_checksum' | 'no_versions'

export type TileAction =
  /** Not installed and a compatible version exists. */
  | { kind: 'install'; version: string }
  /** Installed and a newer compatible version is known. */
  | { kind: 'update'; from: string; to: string }
  /** Installed and nothing newer (that fits this HaexVault) is known. */
  | { kind: 'installed'; version: string }
  /** Nothing can be installed: why, and the HaexVault version the newest one needs, if any. */
  | { kind: 'blocked'; reason: BlockedReason; bound?: string }

/** The reasons the host names for an incompatible version, as far as the tile tells them. */
function blockedReason(version: CatalogVersion | undefined): {
  reason: BlockedReason
  bound?: string
} {
  switch (version?.incompatibleReason) {
    case 'needs_newer':
    case 'needs_older':
    case 'no_checksum':
      return { reason: version.incompatibleReason, bound: version.bound }
    default:
      return { reason: 'no_versions' }
  }
}

/** The one action of an entry's tile. `versions` come newest first. */
export function tileAction(entry: CatalogEntry): TileAction {
  if (entry.installed) {
    if (entry.updateTo)
      return {
        kind: 'update',
        from: entry.installed.version,
        to: entry.updateTo,
      }
    return { kind: 'installed', version: entry.installed.version }
  }
  if (entry.latestCompatible)
    return { kind: 'install', version: entry.latestCompatible }
  return { kind: 'blocked', ...blockedReason(entry.versions[0]) }
}

/** A version of the details can be chosen when it is compatible; an older one stays selectable
 * (it goes through the downgrade confirmation of the preview). */
export function selectable(version: CatalogVersion): boolean {
  return version.compatible
}
