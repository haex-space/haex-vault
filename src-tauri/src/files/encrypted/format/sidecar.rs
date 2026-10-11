//! Sidecars, `P + "m/" + base32(sid)`, one per version of an entry (format "Begleitdatei"):
//!
//! ```text
//! MAGIC | VERSION | 0x02 | sid(16) | nonce(24) | AEAD(K_meta, nonce, json, aad = MAGIC|VERSION|0x02|fid|sid)
//! ```
//!
//! `json` has no whitespace and its fields in a fixed order (`v, entry, parent, name, kind,
//! revision, base, written`, then for files `size, modified, type, sha256, content, cid, dek`).

use data_encoding::{BASE64, HEXLOWER};
use serde::{Deserialize, Deserializer, Serialize};
use zeroize::Zeroizing;

use super::keys::FolderKeys;
use super::{array, check_prefix, decode_id, encode_id, open, prefix, seal, FormatError, Id};
use super::{NONCE_LEN, TAG_LEN, TYPE_SIDECAR};
use crate::files::local::edit::check_name;
use crate::sync::keys::random_bytes;

const WRAPPED_DEK_LEN: usize = NONCE_LEN + 32 + TAG_LEN;
/// `MAGIC | VERSION | 0x02 | sid | nonce`.
const HEADER_LEN: usize = 6 + 16 + NONCE_LEN;

/// One version of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    /// Stays the same over the entry's versions.
    pub entry: Id,
    /// The parent entry; the folder id at the folder's root.
    pub parent: Id,
    pub name: String,
    /// 1 for the first version, then +1.
    pub revision: u64,
    /// The `sid` of the sidecar this one replaces.
    pub base: Option<Id>,
    /// When this version was written, ms since 1970.
    pub written: u64,
    pub kind: SidecarKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarKind {
    Folder,
    File(FileFacts),
}

/// What a file's sidecar says about its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    /// Plaintext size.
    pub size: u64,
    /// The file's modification time, ms.
    pub modified: u64,
    pub content_type: Option<String>,
    /// SHA-256 of the plaintext.
    pub sha256: [u8; 32],
    /// `oid`, the content object's name.
    pub content: Id,
    /// `cid` from the content object's header.
    pub cid: Id,
    pub dek: WrappedDek,
}

/// A content object's `DEK`, wrapped for one entry of one folder:
/// `nonce | AEAD(K_wrap, nonce, DEK, aad = fid | entry | cid)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedDek([u8; WRAPPED_DEK_LEN]);

impl WrappedDek {
    pub fn wrap(keys: &FolderKeys, fid: &Id, entry: &Id, cid: &Id, dek: &[u8; 32]) -> Self {
        Self::wrap_with(keys, fid, entry, cid, dek, &random_bytes::<NONCE_LEN>())
    }

    pub(super) fn wrap_with(
        keys: &FolderKeys,
        fid: &Id,
        entry: &Id,
        cid: &Id,
        dek: &[u8; 32],
        nonce: &[u8; NONCE_LEN],
    ) -> Self {
        let mut wrapped = [0u8; WRAPPED_DEK_LEN];
        wrapped[..NONCE_LEN].copy_from_slice(nonce);
        wrapped[NONCE_LEN..].copy_from_slice(&seal(
            &keys.wrap,
            nonce,
            dek,
            &dek_aad(fid, entry, cid),
        ));
        Self(wrapped)
    }

    /// The `DEK`, only for the entry and content it was wrapped for.
    pub fn unwrap(
        &self,
        keys: &FolderKeys,
        fid: &Id,
        entry: &Id,
        cid: &Id,
    ) -> Result<Zeroizing<[u8; 32]>, FormatError> {
        let dek = Zeroizing::new(open(
            &keys.wrap,
            &self.0[..NONCE_LEN],
            &self.0[NONCE_LEN..],
            &dek_aad(fid, entry, cid),
        )?);
        Ok(Zeroizing::new(array(&dek, 0)?))
    }
}

/// The sidecar `sid` of folder `fid`.
pub fn seal_sidecar(
    keys: &FolderKeys,
    fid: &Id,
    sid: &Id,
    sidecar: &Sidecar,
) -> Result<Vec<u8>, FormatError> {
    seal_sidecar_with(keys, fid, sid, sidecar, &random_bytes::<NONCE_LEN>())
}

pub(super) fn seal_sidecar_with(
    keys: &FolderKeys,
    fid: &Id,
    sid: &Id,
    sidecar: &Sidecar,
    nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, FormatError> {
    check_name(&sidecar.name)?;
    let json = Zeroizing::new(sidecar.to_json());
    let mut object = Vec::with_capacity(HEADER_LEN + json.len() + TAG_LEN);
    object.extend_from_slice(&prefix(TYPE_SIDECAR));
    object.extend_from_slice(sid);
    object.extend_from_slice(nonce);
    object.extend_from_slice(&seal(&keys.meta, nonce, &json, &sidecar_aad(fid, sid)));
    Ok(object)
}

/// Opens the object named `P + "m/" + base32(sid)` in folder `fid`.
pub fn open_sidecar(
    keys: &FolderKeys,
    fid: &Id,
    sid: &Id,
    object: &[u8],
) -> Result<Sidecar, FormatError> {
    check_prefix(object, TYPE_SIDECAR)?;
    let inside: Id = array(object, 6)?;
    if inside != *sid {
        return Err(FormatError::Mismatch);
    }
    let json = Zeroizing::new(open(
        &keys.meta,
        object.get(22..HEADER_LEN).ok_or(FormatError::Malformed)?,
        &object[HEADER_LEN..],
        &sidecar_aad(fid, sid),
    )?);
    Sidecar::from_json(&json)
}

impl Sidecar {
    pub(super) fn to_json(&self) -> Vec<u8> {
        let file = match &self.kind {
            SidecarKind::File(facts) => Some(facts),
            SidecarKind::Folder => None,
        };
        let wire = Wire {
            v: 1,
            entry: encode_id(&self.entry),
            parent: encode_id(&self.parent),
            name: self.name.clone(),
            kind: match self.kind {
                SidecarKind::File(_) => WireKind::File,
                SidecarKind::Folder => WireKind::Folder,
            },
            revision: self.revision,
            base: self.base.as_ref().map(encode_id),
            written: self.written,
            size: file.map(|f| f.size),
            modified: file.map(|f| f.modified),
            content_type: file.map(|f| f.content_type.clone()),
            sha256: file.map(|f| HEXLOWER.encode(&f.sha256)),
            content: file.map(|f| encode_id(&f.content)),
            cid: file.map(|f| encode_id(&f.cid)),
            dek: file.map(|f| BASE64.encode(&f.dek.0)),
        };
        serde_json::to_vec(&wire).expect("a sidecar's fields serialize")
    }

    fn from_json(json: &[u8]) -> Result<Self, FormatError> {
        let wire: Wire =
            serde_json::from_slice(json).map_err(|e| FormatError::Json(e.classify()))?;
        if wire.v > 1 {
            return Err(FormatError::NewerFormat);
        }
        if wire.v != 1 {
            return Err(FormatError::Malformed);
        }
        check_name(&wire.name)?;
        let id = |text: &str| decode_id(text).ok_or(FormatError::Malformed);
        let file_fields = (
            wire.size,
            wire.modified,
            wire.content_type,
            wire.sha256,
            wire.content,
            wire.cid,
            wire.dek,
        );
        let kind = match (wire.kind, file_fields) {
            (WireKind::Folder, (None, None, None, None, None, None, None)) => SidecarKind::Folder,
            (
                WireKind::File,
                (
                    Some(size),
                    Some(modified),
                    Some(content_type),
                    Some(sha256),
                    Some(content),
                    Some(cid),
                    Some(dek),
                ),
            ) => SidecarKind::File(FileFacts {
                size,
                modified,
                content_type,
                sha256: decoded(&HEXLOWER, &sha256)?,
                content: id(&content)?,
                cid: id(&cid)?,
                dek: WrappedDek(decoded(&BASE64, &dek)?),
            }),
            _ => return Err(FormatError::Malformed),
        };
        Ok(Self {
            entry: id(&wire.entry)?,
            parent: id(&wire.parent)?,
            name: wire.name,
            revision: wire.revision,
            base: wire.base.as_deref().map(id).transpose()?,
            written: wire.written,
            kind,
        })
    }
}

/// The JSON of a sidecar; the field order here is the order on the wire. A new field needs a new
/// format version, so an unknown one makes the sidecar invalid.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    v: u64,
    entry: String,
    parent: String,
    name: String,
    kind: WireKind,
    revision: u64,
    base: Option<String>,
    written: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modified: Option<u64>,
    /// Absent for folders, `null` or a text for files.
    #[serde(
        rename = "type",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    content_type: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dek: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum WireKind {
    File,
    Folder,
}

/// A field that is there, even as `null`.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

fn decoded<const N: usize>(
    encoding: &data_encoding::Encoding,
    text: &str,
) -> Result<[u8; N], FormatError> {
    let bytes = encoding
        .decode(text.as_bytes())
        .map_err(|_| FormatError::Malformed)?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| FormatError::Malformed)
}

/// `MAGIC | VERSION | 0x02 | fid | sid`.
fn sidecar_aad(fid: &Id, sid: &Id) -> Vec<u8> {
    let mut aad = prefix(TYPE_SIDECAR).to_vec();
    aad.extend_from_slice(fid);
    aad.extend_from_slice(sid);
    aad
}

/// `fid | entry | cid`.
fn dek_aad(fid: &Id, entry: &Id, cid: &Id) -> [u8; 48] {
    let mut aad = [0u8; 48];
    aad[..16].copy_from_slice(fid);
    aad[16..32].copy_from_slice(entry);
    aad[32..].copy_from_slice(cid);
    aad
}

#[cfg(test)]
#[path = "sidecar_tests.rs"]
mod tests;
