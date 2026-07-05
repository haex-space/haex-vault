// Mirrors src-tauri/src/remote_storage/share_access_flags/mod.rs.
// Bit layout for haex_s3_backends.share_access_flags — keep in sync
// with the Rust module.

export const ShareAccessFlags = {
  LIST: 1 << 0,
  GET: 1 << 1,
  PUT: 1 << 2,
  DELETE: 1 << 3,
} as const

export const SHARE_ACCESS_READ_ONLY = ShareAccessFlags.LIST | ShareAccessFlags.GET
export const SHARE_ACCESS_READ_WRITE =
  ShareAccessFlags.LIST | ShareAccessFlags.GET | ShareAccessFlags.PUT | ShareAccessFlags.DELETE

export function hasFlag(mask: number, flag: number): boolean {
  return (mask & flag) === flag
}

/** Canonical access-level classification of a share's flag bitmap.
 *  `custom` covers unknown/legacy combinations and missing values. */
export type AccessLevelKind = 'readOnly' | 'readWrite' | 'custom'

export type AccessLevelBadgeColor = 'success' | 'warning' | 'neutral'

export function accessLevelKind(
  flags: number | null | undefined,
): AccessLevelKind {
  if (flags === SHARE_ACCESS_READ_ONLY) return 'readOnly'
  if (flags === SHARE_ACCESS_READ_WRITE) return 'readWrite'
  return 'custom'
}

export function accessLevelBadgeColor(
  flags: number | null | undefined,
): AccessLevelBadgeColor {
  switch (accessLevelKind(flags)) {
    case 'readOnly':
      return 'success'
    case 'readWrite':
      return 'warning'
    default:
      return 'neutral'
  }
}
