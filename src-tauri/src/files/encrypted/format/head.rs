//! A folder's head, the object `P + "h"` (format "Kopf"):
//!
//! ```text
//! MAGIC | VERSION | 0x00 | fid(16) | n(1) | recipient × n | name_nonce(24) | name_ct
//! recipient = art(1) | len(2, LE) | data(len)
//! art 0x00:   data = key_id(16) | wrap_nonce(24) | wrapped_fk(48), len = 88
//! ```
//!
//! Art `0x00` is the own vault; other arts are reserved for spaces and skipped by `len`.

use zeroize::Zeroizing;

use super::keys::{kek, FolderKeys};
use super::{array, check_prefix, open, prefix, seal, FormatError, Id, NONCE_LEN, TYPE_HEAD};
use crate::files::local::edit::check_name;
use crate::sync::keys::random_bytes;

/// The recipient art of the own vault.
pub const ART_OWN: u8 = 0x00;
const OWN_LEN: usize = 16 + NONCE_LEN + 32 + 16;
/// `MAGIC | VERSION | 0x00 | fid | n`.
const FIXED_LEN: usize = 6 + 16 + 1;

/// What a reader makes of a head.
pub enum HeadOutcome {
    Readable(OpenedHead),
    /// No recipient the reader can unwrap: another vault, or a generation this device lacks.
    OtherVault,
    NewerFormat,
    Damaged,
}

/// A head that opened.
pub struct OpenedHead {
    pub fk: Zeroizing<[u8; 32]>,
    pub name: String,
    /// The content key generation the folder key was unwrapped with.
    pub key_id: Id,
}

impl std::fmt::Debug for HeadOutcome {
    // Never the folder key, never the name (FR-027).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            HeadOutcome::Readable(_) => "Readable",
            HeadOutcome::OtherVault => "OtherVault",
            HeadOutcome::NewerFormat => "NewerFormat",
            HeadOutcome::Damaged => "Damaged",
        })
    }
}

/// A new head for folder `fid` with key `fk` and one recipient: the own vault's content key `vck`
/// of generation `key_id`.
pub fn seal_head(
    fid: &Id,
    fk: &[u8; 32],
    name: &str,
    key_id: &Id,
    vck: &[u8; 32],
) -> Result<Vec<u8>, FormatError> {
    seal_head_with(
        fid,
        fk,
        name,
        key_id,
        vck,
        &random_bytes::<NONCE_LEN>(),
        &random_bytes::<NONCE_LEN>(),
    )
}

pub(super) fn seal_head_with(
    fid: &Id,
    fk: &[u8; 32],
    name: &str,
    key_id: &Id,
    vck: &[u8; 32],
    wrap_nonce: &[u8; NONCE_LEN],
    name_nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, FormatError> {
    check_name(name)?;
    let mut head = Vec::with_capacity(FIXED_LEN + 3 + OWN_LEN + NONCE_LEN + name.len() + 16);
    head.extend_from_slice(&prefix(TYPE_HEAD));
    head.extend_from_slice(fid);
    head.push(1);
    head.push(ART_OWN);
    head.extend_from_slice(&(OWN_LEN as u16).to_le_bytes());
    head.extend_from_slice(key_id);
    head.extend_from_slice(wrap_nonce);
    head.extend_from_slice(&seal(
        &kek(vck),
        wrap_nonce,
        fk,
        &wrap_aad(fid, ART_OWN, key_id),
    ));
    push_name(&mut head, fid, fk, name, name_nonce);
    Ok(head)
}

/// The head with a new name under a new `name_nonce`; the recipients stay as they are.
pub fn rename_head(head: &[u8], fk: &[u8; 32], name: &str) -> Result<Vec<u8>, FormatError> {
    rename_head_with(head, fk, name, &random_bytes::<NONCE_LEN>())
}

pub(super) fn rename_head_with(
    head: &[u8],
    fk: &[u8; 32],
    name: &str,
    name_nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, FormatError> {
    check_name(name)?;
    let parsed = parse(head)?;
    let mut renamed = head[..parsed.recipients_end].to_vec();
    push_name(&mut renamed, &parsed.fid, fk, name, name_nonce);
    Ok(renamed)
}

/// Opens the head of the folder whose prefix names `fid`. `vck_for` gives the content key of a
/// generation by its key id, if this device holds it.
pub fn open_head(
    head: &[u8],
    fid: &Id,
    mut vck_for: impl FnMut(&Id) -> Option<Zeroizing<[u8; 32]>>,
) -> HeadOutcome {
    let parsed = match parse(head) {
        Ok(parsed) => parsed,
        Err(FormatError::NewerFormat) => return HeadOutcome::NewerFormat,
        Err(_) => return HeadOutcome::Damaged,
    };
    if parsed.fid != *fid {
        return HeadOutcome::Damaged;
    }
    let unwrapped = parsed.own.iter().find_map(|data| {
        let key_id: Id = array(data, 0).ok()?;
        let vck = vck_for(&key_id)?;
        let fk = Zeroizing::new(
            open(
                &kek(&vck),
                &data[16..16 + NONCE_LEN],
                &data[16 + NONCE_LEN..],
                &wrap_aad(fid, ART_OWN, &key_id),
            )
            .ok()?,
        );
        let fk: [u8; 32] = fk.as_slice().try_into().ok()?;
        Some((Zeroizing::new(fk), key_id))
    });
    let Some((fk, key_id)) = unwrapped else {
        return HeadOutcome::OtherVault;
    };
    let keys = FolderKeys::derive(&fk);
    let name = open(
        &keys.meta,
        parsed.name_nonce,
        parsed.name_ct,
        &name_aad(fid),
    )
    .ok()
    .and_then(|bytes| String::from_utf8(bytes).ok())
    .filter(|name| check_name(name).is_ok());
    match name {
        Some(name) => HeadOutcome::Readable(OpenedHead { fk, name, key_id }),
        None => HeadOutcome::Damaged,
    }
}

struct Parsed<'a> {
    fid: Id,
    /// The data of every recipient of art [`ART_OWN`].
    own: Vec<&'a [u8]>,
    recipients_end: usize,
    name_nonce: &'a [u8],
    name_ct: &'a [u8],
}

fn parse(head: &[u8]) -> Result<Parsed<'_>, FormatError> {
    check_prefix(head, TYPE_HEAD)?;
    let fid = array(head, 6)?;
    let count = *head.get(22).ok_or(FormatError::Malformed)?;
    let mut at = FIXED_LEN;
    let mut own = Vec::new();
    for _ in 0..count {
        let [art, len_lo, len_hi] = array(head, at)?;
        let len = usize::from(u16::from_le_bytes([len_lo, len_hi]));
        let data = head
            .get(at + 3..at + 3 + len)
            .ok_or(FormatError::Malformed)?;
        if art == ART_OWN {
            if len != OWN_LEN {
                return Err(FormatError::Malformed);
            }
            own.push(data);
        }
        at += 3 + len;
    }
    let name_nonce = head.get(at..at + NONCE_LEN).ok_or(FormatError::Malformed)?;
    let name_ct = &head[at + NONCE_LEN..];
    if name_ct.len() < 16 {
        return Err(FormatError::Malformed);
    }
    Ok(Parsed {
        fid,
        own,
        recipients_end: at,
        name_nonce,
        name_ct,
    })
}

fn push_name(head: &mut Vec<u8>, fid: &Id, fk: &[u8; 32], name: &str, nonce: &[u8; NONCE_LEN]) {
    let keys = FolderKeys::derive(fk);
    head.extend_from_slice(nonce);
    head.extend_from_slice(&seal(&keys.meta, nonce, name.as_bytes(), &name_aad(fid)));
}

/// `MAGIC | VERSION | 0x00 | fid | art | key_id`.
fn wrap_aad(fid: &Id, art: u8, key_id: &Id) -> Vec<u8> {
    let mut aad = prefix(TYPE_HEAD).to_vec();
    aad.extend_from_slice(fid);
    aad.push(art);
    aad.extend_from_slice(key_id);
    aad
}

/// `MAGIC | VERSION | 0x00 | fid | "name"`.
fn name_aad(fid: &Id) -> Vec<u8> {
    let mut aad = prefix(TYPE_HEAD).to_vec();
    aad.extend_from_slice(fid);
    aad.extend_from_slice(b"name");
    aad
}

#[cfg(test)]
#[path = "head_tests.rs"]
mod tests;
