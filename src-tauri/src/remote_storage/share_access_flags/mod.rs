//! Access-flag bitmap constants for shareable storage backends.
//!
//! Mirrors `src/lib/storage/shareAccessFlags.ts` on the frontend so both
//! sides agree on the bit layout stored in `haex_storage_backends.share_access_flags`.
//!
//! Bit layout:
//! - bit 0: LIST
//! - bit 1: GET
//! - bit 2: PUT
//! - bit 3: DELETE

pub const LIST: i64 = 1 << 0;
pub const GET: i64 = 1 << 1;
pub const PUT: i64 = 1 << 2;
pub const DELETE: i64 = 1 << 3;

pub const READ_ONLY: i64 = LIST | GET;
pub const READ_WRITE: i64 = LIST | GET | PUT | DELETE;

#[inline]
pub fn has_flag(mask: i64, flag: i64) -> bool {
    (mask & flag) == flag
}

#[cfg(test)]
mod tests;
