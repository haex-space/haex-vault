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
