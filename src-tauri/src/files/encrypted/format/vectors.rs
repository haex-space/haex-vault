//! The `HXEF` v1 test vectors (spec 048, FR-042, SC-010) and their checks, shared by the unit
//! tests and the platform probe (T013), so Windows, macOS and Android run the same checks. The
//! vectors were made independently of this code; how is in their `_comment`.

use data_encoding::HEXLOWER;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::content::{ciphertext_len, ContentOpener, ContentSealer};
use super::head::{open_head, rename_head_with, seal_head_with, HeadOutcome};
use super::keys::{kek, FolderKeys};
use super::sidecar::{open_sidecar, seal_sidecar_with, SidecarKind};
use super::{encode_id, folder_id, is_folder_prefix, FormatError, Id, CHUNK, MAGIC, VERSION};

/// `src-tauri/tests/fixtures/encrypted/v1-vectors.json`.
pub const V1: &str = include_str!("../../../../tests/fixtures/encrypted/v1-vectors.json");

/// What failed, by vector name; never secret, the vectors are public.
pub type Checked = Result<usize, String>;

/// Runs every check; returns how many passed.
pub fn check(json: &str) -> Checked {
    let vectors = parse(json)?;
    Ok(check_keys(&vectors)?
        + check_head(&vectors)?
        + check_contents(&vectors)?
        + check_sidecars(&vectors)?)
}

pub fn parse(json: &str) -> Result<Vectors, String> {
    serde_json::from_str(json).map_err(|e| format!("the vectors do not parse: {e}"))
}

#[derive(Deserialize)]
pub struct Vectors {
    constants: Constants,
    keys: Keys,
    head: Head,
    contents: Vec<Content>,
    content_cases: Vec<ContentCase>,
    sidecars: Vec<SidecarVector>,
    sidecar_cases: Vec<SidecarCase>,
    dek_cases: Vec<DekCase>,
}

#[derive(Deserialize)]
struct Constants {
    magic: String,
    version: u8,
    chunk: usize,
}

#[derive(Deserialize)]
struct Keys {
    vck: String,
    kek: String,
    fk: String,
    k_meta: String,
    k_wrap: String,
}

#[derive(Deserialize)]
struct Head {
    fid: String,
    fid_base32: String,
    object_key: String,
    key_id: String,
    wrap_nonce: String,
    name_nonce: String,
    name: String,
    bytes: String,
    renamed: Renamed,
    cases: Vec<HeadCase>,
}

#[derive(Deserialize)]
struct Renamed {
    name: String,
    name_nonce: String,
    bytes: String,
}

#[derive(Deserialize)]
struct HeadCase {
    name: String,
    bytes: String,
    fid: String,
    reader: Vec<ReaderKey>,
    expect: String,
}

#[derive(Deserialize)]
struct ReaderKey {
    key_id: String,
    vck: String,
}

#[derive(Deserialize)]
struct Content {
    name: String,
    size: u64,
    dek: String,
    cid: String,
    nonce_prefix: String,
    plaintext_sha256: String,
    object_len: u64,
    object_sha256: String,
    object: Option<String>,
}

#[derive(Deserialize)]
struct ContentCase {
    name: String,
    object: String,
    ops: Vec<Op>,
    reader: ContentReader,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Op {
    Flip(usize),
    Truncate(usize),
    Append(String),
    Swap(usize, usize, usize),
    Replace(usize, String),
}

#[derive(Deserialize)]
struct ContentReader {
    cid: String,
    dek: String,
    size: u64,
}

#[derive(Deserialize)]
struct SidecarVector {
    name: String,
    sid: String,
    object_key: String,
    nonce: String,
    json: String,
    bytes: String,
    entry: String,
    dek: Option<String>,
}

#[derive(Deserialize)]
struct SidecarCase {
    name: String,
    bytes: String,
    fid: String,
    sid: String,
}

#[derive(Deserialize)]
struct DekCase {
    name: String,
    sidecar: String,
    entry: String,
    expect: String,
}

/// Vector 1: constants, `KEK_g`, `K_meta`, `K_wrap`.
pub fn check_keys(v: &Vectors) -> Checked {
    expect(hex(&v.constants.magic)? == MAGIC, "magic")?;
    expect(v.constants.version == VERSION, "version")?;
    expect(v.constants.chunk == CHUNK, "chunk")?;
    let keys = FolderKeys::derive(&fixed(&v.keys.fk)?);
    expect(*kek(&fixed(&v.keys.vck)?) == fixed(&v.keys.kek)?, "KEK")?;
    expect(*keys.meta == fixed(&v.keys.k_meta)?, "K_meta")?;
    expect(*keys.wrap == fixed(&v.keys.k_wrap)?, "K_wrap")?;
    Ok(6)
}

/// Vector 2 and the head cases of 5.
pub fn check_head(v: &Vectors) -> Checked {
    let h = &v.head;
    let fid: Id = fixed(&h.fid)?;
    let fk: [u8; 32] = fixed(&v.keys.fk)?;
    let prefix = format!("{}.hxef", h.fid_base32);
    expect(encode_id(&fid) == h.fid_base32, "head: fid in Base32")?;
    expect(format!("{prefix}/h") == h.object_key, "head: object key")?;
    expect(is_folder_prefix(&prefix), "head: prefix form")?;
    expect(folder_id(&prefix) == Some(fid), "head: fid of the prefix")?;
    let sealed = seal_head_with(
        &fid,
        &fk,
        &h.name,
        &fixed(&h.key_id)?,
        &fixed(&v.keys.vck)?,
        &fixed(&h.wrap_nonce)?,
        &fixed(&h.name_nonce)?,
    )
    .map_err(|e| format!("head: seal: {e}"))?;
    expect(sealed == hex(&h.bytes)?, "head: sealed bytes")?;
    let renamed = rename_head_with(
        &sealed,
        &fk,
        &h.renamed.name,
        &fixed(&h.renamed.name_nonce)?,
    )
    .map_err(|e| format!("head: rename: {e}"))?;
    expect(renamed == hex(&h.renamed.bytes)?, "head: renamed bytes")?;
    let mut count = 6;
    for case in &h.cases {
        let readers = case
            .reader
            .iter()
            .map(|r| Ok((fixed::<16>(&r.key_id)?, fixed::<32>(&r.vck)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let outcome = open_head(&hex(&case.bytes)?, &fixed(&case.fid)?, |key_id| {
            readers
                .iter()
                .find(|(id, _)| id == key_id)
                .map(|(_, vck)| Zeroizing::new(*vck))
        });
        let ok = match (&outcome, case.expect.as_str()) {
            (HeadOutcome::Readable(head), "readable") => {
                *head.fk == fk && head.name == h.name && head.key_id == fixed::<16>(&h.key_id)?
            }
            (HeadOutcome::OtherVault, "otherVault")
            | (HeadOutcome::NewerFormat, "newerFormat")
            | (HeadOutcome::Damaged, "damaged") => true,
            _ => false,
        };
        expect(ok, &format!("head case {:?}: got {outcome:?}", case.name))?;
        count += 1;
    }
    Ok(count)
}

/// Vector 3 and the content cases of 5.
pub fn check_contents(v: &Vectors) -> Checked {
    let mut count = 0;
    for c in &v.contents {
        let object = sealed_content(c)?;
        let what = |check: &str| format!("content {:?}: {check}", c.name);
        expect(
            ciphertext_len(c.size) == c.object_len,
            &what("ciphertext_len"),
        )?;
        expect(object.len() as u64 == c.object_len, &what("length"))?;
        expect(
            sha256(&object) == fixed(&c.object_sha256)?,
            &what("object hash"),
        )?;
        if let Some(bytes) = &c.object {
            expect(object == hex(bytes)?, &what("object bytes"))?;
        }
        let plain = read_all(&object, &fixed(&c.cid)?, &fixed(&c.dek)?, c.size)
            .map_err(|e| what(&format!("open: {e}")))?;
        expect(
            sha256(&plain) == fixed(&c.plaintext_sha256)?,
            &what("plaintext"),
        )?;
        let opener = ContentOpener::new(&object, &fixed(&c.cid)?, &fixed(&c.dek)?, c.size)
            .map_err(|e| what(&e.to_string()))?;
        let chunk = CHUNK as u64;
        for (start, end) in [(0, c.size), (chunk - 3, chunk + 1)] {
            if start >= end || end > c.size {
                continue;
            }
            let range = opener
                .ciphertext_range(start, end)
                .map_err(|e| what(&e.to_string()))?;
            let part = opener
                .open_range(
                    start,
                    end,
                    &object[range.start as usize..range.end as usize],
                )
                .map_err(|e| what(&format!("range: {e}")))?;
            expect(
                part == plaintext(c.size)[start as usize..end as usize],
                &what("range"),
            )?;
            count += 1;
        }
        count += 5;
    }
    for case in &v.content_cases {
        let base = v
            .contents
            .iter()
            .find(|c| c.name == case.object)
            .ok_or_else(|| format!("content case {:?}: no object", case.name))?;
        let mut object = sealed_content(base)?;
        for op in &case.ops {
            apply(&mut object, op).map_err(|e| format!("content case {:?}: {e}", case.name))?;
        }
        let r = &case.reader;
        let outcome = read_all(&object, &fixed(&r.cid)?, &fixed(&r.dek)?, r.size);
        expect(
            outcome.is_err(),
            &format!("content case {:?} was not refused", case.name),
        )?;
        count += 1;
    }
    Ok(count)
}

/// Vector 4, the sidecar cases of 5 and the rewrapped keys.
pub fn check_sidecars(v: &Vectors) -> Checked {
    let keys = FolderKeys::derive(&fixed(&v.keys.fk)?);
    let fid: Id = fixed(&v.head.fid)?;
    let mut count = 0;
    for s in &v.sidecars {
        let what = |check: &str| format!("sidecar {:?}: {check}", s.name);
        let sid: Id = fixed(&s.sid)?;
        let sidecar = open_sidecar(&keys, &fid, &sid, &hex(&s.bytes)?)
            .map_err(|e| what(&format!("open: {e}")))?;
        let key = format!("{}.hxef/m/{}", encode_id(&fid), encode_id(&sid));
        expect(key == s.object_key, &what("object key"))?;
        expect(sidecar.to_json() == s.json.as_bytes(), &what("json"))?;
        expect(sidecar.entry == fixed(&s.entry)?, &what("entry"))?;
        let resealed = seal_sidecar_with(&keys, &fid, &sid, &sidecar, &fixed(&s.nonce)?)
            .map_err(|e| what(&e.to_string()))?;
        expect(resealed == hex(&s.bytes)?, &what("sealed bytes"))?;
        count += 4;
        match (&sidecar.kind, &s.dek) {
            (SidecarKind::File(facts), Some(dek)) => {
                let unwrapped = facts
                    .dek
                    .unwrap(&keys, &fid, &sidecar.entry, &facts.cid)
                    .map_err(|e| what(&format!("dek: {e}")))?;
                expect(*unwrapped == fixed::<32>(dek)?, &what("dek"))?;
                count += 1;
            }
            (SidecarKind::Folder, None) => {}
            _ => return Err(what("kind")),
        }
    }
    for case in &v.sidecar_cases {
        let outcome = open_sidecar(
            &keys,
            &fixed(&case.fid)?,
            &fixed(&case.sid)?,
            &hex(&case.bytes)?,
        );
        expect(
            outcome.is_err(),
            &format!("sidecar case {:?} was not refused", case.name),
        )?;
        count += 1;
    }
    for case in &v.dek_cases {
        let what = |check: &str| format!("dek case {:?}: {check}", case.name);
        let s = v
            .sidecars
            .iter()
            .find(|s| s.name == case.sidecar)
            .ok_or_else(|| what("no sidecar"))?;
        let sidecar = open_sidecar(&keys, &fid, &fixed(&s.sid)?, &hex(&s.bytes)?)
            .map_err(|e| what(&e.to_string()))?;
        let SidecarKind::File(facts) = &sidecar.kind else {
            return Err(what("not a file"));
        };
        let unwrapped = facts
            .dek
            .unwrap(&keys, &fid, &fixed(&case.entry)?, &facts.cid);
        let ok = match (case.expect.as_str(), &unwrapped, &s.dek) {
            ("refused", Err(_), _) => true,
            ("dek", Ok(dek), Some(expected)) => **dek == fixed::<32>(expected)?,
            _ => false,
        };
        expect(ok, &what("unexpected outcome"))?;
        count += 1;
    }
    Ok(count)
}

/// A whole content object read the way a download reads it: header, length, every block.
fn read_all(object: &[u8], cid: &Id, dek: &[u8; 32], size: u64) -> Result<Vec<u8>, FormatError> {
    let opener = ContentOpener::new(object, cid, dek, size)?;
    opener.check_len(object.len() as u64)?;
    let mut plain = Vec::new();
    for index in 0..opener.block_count() {
        let span = opener.block_span(index)?;
        plain.extend(opener.open_block(index, &object[span.start as usize..span.end as usize])?);
    }
    Ok(plain)
}

/// The content object sealed from the vector's inputs, in uneven pieces.
fn sealed_content(c: &Content) -> Result<Vec<u8>, String> {
    let mut sealer =
        ContentSealer::with_parts(&fixed(&c.dek)?, &fixed(&c.cid)?, &fixed(&c.nonce_prefix)?);
    let mut object = Vec::new();
    for piece in plaintext(c.size).chunks(4093) {
        sealer.push(piece, &mut object);
    }
    sealer.finish(&mut object);
    Ok(object)
}

fn apply(object: &mut Vec<u8>, op: &Op) -> Result<(), String> {
    let outside = || "an op reaches outside the object".to_owned();
    match op {
        Op::Flip(at) => *object.get_mut(*at).ok_or_else(outside)? ^= 0x01,
        Op::Truncate(len) => {
            if *len > object.len() {
                return Err(outside());
            }
            object.truncate(*len);
        }
        Op::Append(bytes) => object.extend(hex(bytes)?),
        Op::Swap(a, b, len) => {
            let (a, b) = ((*a).min(*b), (*a).max(*b));
            if a + len > b || b + len > object.len() {
                return Err(outside());
            }
            let (head, tail) = object.split_at_mut(b);
            head[a..a + len].swap_with_slice(&mut tail[..*len]);
        }
        Op::Replace(at, bytes) => {
            let bytes = hex(bytes)?;
            object
                .get_mut(*at..*at + bytes.len())
                .ok_or_else(outside)?
                .copy_from_slice(&bytes);
        }
    }
    Ok(())
}

/// The vectors' plaintext of `size` bytes: byte `i` is `i mod 251`.
fn plaintext(size: u64) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn hex(text: &str) -> Result<Vec<u8>, String> {
    HEXLOWER
        .decode(text.as_bytes())
        .map_err(|e| format!("bad hex in the vectors: {e}"))
}

fn fixed<const N: usize>(text: &str) -> Result<[u8; N], String> {
    hex(text)?
        .try_into()
        .map_err(|_| format!("expected {N} bytes in the vectors"))
}

fn expect(ok: bool, what: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("vector check failed: {what}"))
    }
}

#[cfg(test)]
#[path = "vectors_tests.rs"]
mod tests;
