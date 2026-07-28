#!/usr/bin/env tsx
/**
 * Cross-language column-sig verification vectors (Phase 1, Task I1).
 *
 * Generates `src-tauri/tests/fixtures/column_sig_vectors.json`, consumed by
 * the Rust vector test (`src-tauri/tests/column_sig_vectors.rs`, Task I2)
 * and the TS canonicalisation test (`src/tests/sync/column-sig-vectors.test.ts`,
 * Task I3). Both languages must agree on:
 *
 *   1. Canonical byte encoding of a SQLite storage-class value
 *      (Rust: `crdt::column_sig::value_bytes::to_canonical_bytes`;
 *       TS:   `src/utils/columnSigCanonical.ts::toCanonicalBytes`).
 *   2. Length-prefixed domain-separated preimage
 *      (Rust: `crdt::column_sig::preimage::build_preimage` = domain-tag
 *      `haex/space-col-sig/v1` + u32-BE length-prefixed fields).
 *   3. Ed25519 signature over that preimage.
 *
 * Any drift in any of these three layers breaks column-sig verification
 * across the wire (TS pull → Rust verify_column_sig_batch, ADR 0002 §4b).
 *
 * DETERMINISM: seeds, HLCs, space_ids, PK JSON — all hardcoded. Running
 * this script twice must produce byte-identical JSON.
 *
 * WARNING: seeds below are TEST_ONLY_NEVER_PROD. Never use in a live vault.
 *
 * Note (Runde 7 lesson): the fixture JSON may carry plaintext value bytes
 * because it is TEST data, not sync-wire payload. The real ColumnChange
 * wire format (see `src/stores/sync/tableScanner.ts`) does NOT ship
 * `valueBytes` — it would leak plaintext to the sync relay alongside
 * `encryptedValue`. Column-sig verify runs post-decrypt on the receiver.
 */
import {
  createHash,
  createPrivateKey,
  createPublicKey,
  sign as nodeSign,
  verify as nodeVerify,
  type KeyObject,
} from 'node:crypto'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { base58btcEncode, publicKeyToDid } from '@haex-space/ucan'

// ---------------------------------------------------------------------------
// Deterministic Ed25519 test keys (TEST_ONLY_NEVER_PROD).
//
// Two independent signing identities:
//   - primary: signs the 5 storage-class vectors + the first multi-space
//     vector + the tampered-sig / wrong-space reject vectors
//   - secondary: signs the second multi-space vector; also used to build
//     the wrong-author-did reject vector (its DID is declared in the
//     vector but the sig was actually made by `primary`).
// ---------------------------------------------------------------------------
const SEEDS_HEX: Record<string, string> = {
  primary: '0101010101010101010101010101010101010101010101010101010101010101',
  secondary: '0202020202020202020202020202020202020202020202020202020202020202',
}

// PKCS8 DER prefix for a raw Ed25519 private key seed (16 bytes header,
// followed by the 32-byte seed = 48 bytes total). Same as
// `scripts/gen-ucan-chain-vectors.ts`.
const ED25519_PKCS8_HEADER = Buffer.from('302e020100300506032b657004220420', 'hex')

interface Key {
  name: string
  priv: KeyObject
  rawPub: Uint8Array
  did: string
}

function makeKey(name: string, seedHex: string): Key {
  const seed = Buffer.from(seedHex, 'hex')
  if (seed.length !== 32) throw new Error(`seed ${name} must be 32 bytes`)
  const pkcs8 = Buffer.concat([ED25519_PKCS8_HEADER, seed])
  const priv = createPrivateKey({ key: pkcs8, format: 'der', type: 'pkcs8' })
  const pub = createPublicKey(priv)
  const spki = pub.export({ format: 'der', type: 'spki' }) as Buffer
  // SPKI wrapper for Ed25519 is 12 bytes, then the 32 raw key bytes.
  const rawPub = new Uint8Array(spki.subarray(12))
  if (rawPub.length !== 32) throw new Error(`unexpected pubkey length for ${name}`)
  const did = publicKeyToDid(rawPub)
  return { name, priv, rawPub, did }
}

const KEYS: Record<string, Key> = Object.fromEntries(
  Object.entries(SEEDS_HEX).map(([n, s]) => [n, makeKey(n, s)]),
)

// ---------------------------------------------------------------------------
// Canonical byte encoding of a SQLite storage-class value.
//
// !!! MUST STAY BYTE-IDENTICAL WITH
//   - `src-tauri/src/crdt/column_sig/value_bytes.rs::to_canonical_bytes`
//   - `src/utils/columnSigCanonical.ts::toCanonicalBytes`
//
// Storage-class → bytes:
//   - NULL     → empty
//   - INTEGER  → i64 big-endian, 8 bytes
//   - REAL     → f64 big-endian IEEE-754 bits;
//                NaN → 0x7FF8_0000_0000_0000, -0.0 → +0.0
//   - TEXT     → UTF-8 bytes verbatim (no Unicode normalisation)
//   - BLOB     → bytes verbatim
// ---------------------------------------------------------------------------
const CANONICAL_QUIET_NAN = 0x7ff8_0000_0000_0000n

type StorageValue =
  | { kind: 'null' }
  | { kind: 'integer'; value: bigint }
  | { kind: 'real'; value: number }
  | { kind: 'text'; value: string }
  | { kind: 'blob'; value: Uint8Array }

function canonicalBytes(v: StorageValue): Uint8Array {
  switch (v.kind) {
    case 'null':
      return new Uint8Array(0)
    case 'integer': {
      const buf = new ArrayBuffer(8)
      new DataView(buf).setBigInt64(0, v.value, false)
      return new Uint8Array(buf)
    }
    case 'real': {
      const buf = new ArrayBuffer(8)
      const view = new DataView(buf)
      if (Number.isNaN(v.value)) view.setBigUint64(0, CANONICAL_QUIET_NAN, false)
      else if (v.value === 0) view.setBigUint64(0, 0n, false)
      else view.setFloat64(0, v.value, false)
      return new Uint8Array(buf)
    }
    case 'text':
      return new TextEncoder().encode(v.value)
    case 'blob':
      return new Uint8Array(v.value)
  }
}

/**
 * JSON-safe encoding of a StorageValue for the fixture's `value` field.
 * The Rust/TS reader dispatches on `kind` to reconstruct the native value
 * for canonicalisation, so we tag every non-null case explicitly.
 */
function encodeValueForFixture(v: StorageValue): unknown {
  switch (v.kind) {
    case 'null':
      return null
    case 'integer':
      return { integer: v.value.toString() }
    case 'real':
      // NaN cannot round-trip through JSON, tag it and let the reader
      // reconstruct `Number.NaN`. Finite reals round-trip via number.
      return Number.isNaN(v.value) ? { realNaN: true } : { real: v.value }
    case 'text':
      return { text: v.value }
    case 'blob':
      return { blob: Array.from(v.value) }
  }
}

// ---------------------------------------------------------------------------
// Length-prefixed preimage builder.
//
// !!! MUST STAY BYTE-IDENTICAL WITH
//   `src-tauri/src/crdt/column_sig/preimage.rs::build_preimage`.
//
// Format: for each field (domain_tag, space_id, table_name, row_pks,
// column_name, hlc, author_did, value_bytes) emit `u32-BE(len) || bytes`.
// Domain-tag first, so a value-bytes forgery cannot masquerade as a
// domain-tag prefix (ADR 0002 §4b).
// ---------------------------------------------------------------------------
const DOMAIN_TAG = 'haex/space-col-sig/v1'

function pushField(chunks: Uint8Array[], field: Uint8Array): void {
  if (field.length > 0xffff_ffff) throw new Error('field length exceeds u32')
  const lenBuf = new ArrayBuffer(4)
  new DataView(lenBuf).setUint32(0, field.length, false)
  chunks.push(new Uint8Array(lenBuf))
  chunks.push(field)
}

function buildPreimage(args: {
  spaceId: string
  tableName: string
  rowPks: string
  columnName: string
  hlc: string
  authorDid: string
  valueBytes: Uint8Array
}): Uint8Array {
  const enc = new TextEncoder()
  const chunks: Uint8Array[] = []
  pushField(chunks, enc.encode(DOMAIN_TAG))
  pushField(chunks, enc.encode(args.spaceId))
  pushField(chunks, enc.encode(args.tableName))
  pushField(chunks, enc.encode(args.rowPks))
  pushField(chunks, enc.encode(args.columnName))
  pushField(chunks, enc.encode(args.hlc))
  pushField(chunks, enc.encode(args.authorDid))
  pushField(chunks, args.valueBytes)
  const total = chunks.reduce((n, c) => n + c.length, 0)
  const out = new Uint8Array(total)
  let off = 0
  for (const c of chunks) {
    out.set(c, off)
    off += c.length
  }
  return out
}

// ---------------------------------------------------------------------------
// Ed25519 sign/verify helpers (node:crypto over a raw 32-byte pubkey).
// ---------------------------------------------------------------------------
function signPreimage(priv: KeyObject, preimage: Uint8Array): Uint8Array {
  return new Uint8Array(nodeSign(null, preimage, priv))
}

function verifyRawEd25519(rawPub: Uint8Array, preimage: Uint8Array, sig: Uint8Array): boolean {
  // Wrap raw 32-byte pubkey in SPKI so node's `verify` accepts it.
  const spkiHeader = Buffer.from('302a300506032b6570032100', 'hex')
  const spki = Buffer.concat([spkiHeader, Buffer.from(rawPub)])
  const pubKey = createPublicKey({ key: spki, format: 'der', type: 'spki' })
  return nodeVerify(null, preimage, pubKey, sig)
}

function base64Standard(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString('base64')
}

// ---------------------------------------------------------------------------
// Fixed test space_ids and HLCs.
//
// Column-sig verification treats space_id as an opaque byte string in the
// preimage — there is no space_id/DID binding check at this layer (that
// belongs to UCAN chain verification). Short deterministic strings are
// therefore fine and keep the fixture readable.
// ---------------------------------------------------------------------------
const PRIMARY_SPACE = 'spc_A_TEST_ONLY_NEVER_PROD'
const SECONDARY_SPACE = 'spc_B_TEST_ONLY_NEVER_PROD'

const TABLE_DEVICES = 'devices'
const ROW_PKS_DEV1 = JSON.stringify({ id: 'dev-1' })
const HLC_BASE = '1/aaaaaaaaaaaaaaaa'

// ---------------------------------------------------------------------------
// Vector shape (mirrors what the Rust + TS readers expect).
// ---------------------------------------------------------------------------
type Expected =
  | 'verify_ok'
  | 'verify_rejected_sig'
  | 'verify_rejected_wrong_space'
  | 'verify_rejected_wrong_did'

interface Vector {
  name: string
  spaceId: string
  tableName: string
  rowPks: string
  columnName: string
  columnType: string
  hlc: string
  authorDid: string
  value: unknown
  valueBytes: string
  sig: string
  expected: Expected
}

interface BuildValidArgs {
  name: string
  spaceId: string
  columnName: string
  columnType: string
  hlc: string
  signer: Key
  storage: StorageValue
}

function buildValid(args: BuildValidArgs): Vector {
  const valueBytes = canonicalBytes(args.storage)
  const preimage = buildPreimage({
    spaceId: args.spaceId,
    tableName: TABLE_DEVICES,
    rowPks: ROW_PKS_DEV1,
    columnName: args.columnName,
    hlc: args.hlc,
    authorDid: args.signer.did,
    valueBytes,
  })
  const sig = signPreimage(args.signer.priv, preimage)
  if (!verifyRawEd25519(args.signer.rawPub, preimage, sig)) {
    fail(`${args.name}: self-verify failed for a supposedly-valid vector`)
  }
  return {
    name: args.name,
    spaceId: args.spaceId,
    tableName: TABLE_DEVICES,
    rowPks: ROW_PKS_DEV1,
    columnName: args.columnName,
    columnType: args.columnType,
    hlc: args.hlc,
    authorDid: args.signer.did,
    value: encodeValueForFixture(args.storage),
    valueBytes: base64Standard(valueBytes),
    sig: base64Standard(sig),
    expected: 'verify_ok',
  }
}

// Reject-vector builders. Each builds a *valid* vector first and then
// perturbs one field so that Rust's `verify_column_sig` returns
// `InvalidSignature`. The `expected` tag documents which field was
// perturbed — Rust folds all three into `InvalidSignature`, but naming
// the scenario keeps the fixture readable and lets the TS test surface
// clearer failure messages.

function buildTamperedSig(base: Vector): Vector {
  // Flip byte 10 of the 64-byte signature. Signature::from_slice still
  // accepts the length; ed25519_dalek `verify` fails on the crypto check.
  const raw = Buffer.from(base.sig, 'base64')
  if (raw.length !== 64) fail(`${base.name}: expected 64-byte sig, got ${raw.length}`)
  const tampered = new Uint8Array(raw)
  tampered[10] = tampered[10]! ^ 0x01
  const preimage = rebuildPreimage(base)
  if (verifyRawEd25519(KEYS.primary!.rawPub, preimage, tampered)) {
    fail(`${base.name}: bit-flipped sig unexpectedly still verifies`)
  }
  return {
    ...base,
    name: 'reject_tampered_sig',
    sig: base64Standard(tampered),
    expected: 'verify_rejected_sig',
  }
}

function buildWrongSpaceId(base: Vector): Vector {
  // Take a sig valid for PRIMARY_SPACE and re-label the vector as
  // SECONDARY_SPACE. The Rust verifier rebuilds the preimage using the
  // vector's declared spaceId, so the sig no longer covers the preimage
  // → InvalidSignature.
  if (base.spaceId !== PRIMARY_SPACE) {
    fail(`${base.name}: base vector must be in PRIMARY_SPACE`)
  }
  return {
    ...base,
    name: 'reject_wrong_space_id',
    spaceId: SECONDARY_SPACE,
    expected: 'verify_rejected_wrong_space',
  }
}

function buildWrongAuthorDid(base: Vector): Vector {
  // Sig was made by KEYS.primary. Relabel the vector's authorDid to
  // KEYS.secondary. The Rust verifier decodes secondary.did → pub_2,
  // rebuilds preimage with secondary.did in the author_did field, and
  // checks the sig with pub_2 → InvalidSignature on both counts.
  if (base.authorDid !== KEYS.primary!.did) {
    fail(`${base.name}: base vector must be signed by KEYS.primary`)
  }
  return {
    ...base,
    name: 'reject_wrong_author_did',
    authorDid: KEYS.secondary!.did,
    expected: 'verify_rejected_wrong_did',
  }
}

function rebuildPreimage(v: Vector): Uint8Array {
  return buildPreimage({
    spaceId: v.spaceId,
    tableName: v.tableName,
    rowPks: v.rowPks,
    columnName: v.columnName,
    hlc: v.hlc,
    authorDid: v.authorDid,
    valueBytes: new Uint8Array(Buffer.from(v.valueBytes, 'base64')),
  })
}

function fail(msg: string): never {
  console.error(`gen-column-sig-vectors: ${msg}`)
  process.exit(1)
}

// ---------------------------------------------------------------------------
// Self-verification: prove every ok-vector's sig actually verifies and
// every reject-vector's sig actually fails against Rust's rules. This
// closes the loop before the fixture reaches disk — a subtle drift in
// canonicalisation or preimage layout that would only surface in the
// Rust test suite is caught here first, at generator time.
// ---------------------------------------------------------------------------
function selfVerify(vectors: Vector[]): void {
  for (const v of vectors) {
    const preimage = rebuildPreimage(v)
    const sig = new Uint8Array(Buffer.from(v.sig, 'base64'))

    // Which pubkey should the verifier use? The vector's declared
    // authorDid → the pubkey embedded in it. For wrong-did rejects that
    // is the secondary key — the sig was made by primary, so verify
    // must fail against secondary's pubkey.
    const declaredKey = v.authorDid === KEYS.primary!.did ? KEYS.primary! : KEYS.secondary!
    if (declaredKey.did !== v.authorDid) {
      fail(`${v.name}: authorDid ${v.authorDid} matches neither test key`)
    }
    const ok = verifyRawEd25519(declaredKey.rawPub, preimage, sig)

    if (v.expected === 'verify_ok') {
      if (!ok) fail(`${v.name}: expected verify_ok but self-verify failed`)
    } else {
      if (ok) fail(`${v.name}: expected ${v.expected} but self-verify passed`)
    }
  }
  console.error(`self-verify passed for ${vectors.length} column-sig vectors.`)
}

// ---------------------------------------------------------------------------
// Vector list
// ---------------------------------------------------------------------------
function buildAllVectors(): Vector[] {
  // Storage-class valid vectors. One per SQLite storage class the
  // canonicaliser handles.
  const nullVec = buildValid({
    name: 'null_value_valid',
    spaceId: PRIMARY_SPACE,
    columnName: 'avatar',
    columnType: 'TEXT',
    hlc: HLC_BASE,
    signer: KEYS.primary!,
    storage: { kind: 'null' },
  })
  const integerVec = buildValid({
    name: 'integer_negative_valid',
    spaceId: PRIMARY_SPACE,
    columnName: 'signed_counter',
    columnType: 'INTEGER',
    hlc: '2/aaaaaaaaaaaaaaaa',
    signer: KEYS.primary!,
    storage: { kind: 'integer', value: -1n },
  })
  const realVec = buildValid({
    name: 'real_nan_valid',
    spaceId: PRIMARY_SPACE,
    columnName: 'weight_kg',
    columnType: 'REAL',
    hlc: '3/aaaaaaaaaaaaaaaa',
    signer: KEYS.primary!,
    storage: { kind: 'real', value: Number.NaN },
  })
  const textVec = buildValid({
    name: 'text_utf8_umlaut_valid',
    spaceId: PRIMARY_SPACE,
    columnName: 'display_name',
    columnType: 'TEXT',
    hlc: '4/aaaaaaaaaaaaaaaa',
    signer: KEYS.primary!,
    storage: { kind: 'text', value: 'ä' },
  })
  // Deterministic 32-byte blob (0x10, 0x11, ..., 0x2f). Fully self-contained,
  // matches what Rust `Value::Blob(vec![0x10..0x30])` would produce.
  const blobBytes = new Uint8Array(32)
  for (let i = 0; i < 32; i++) blobBytes[i] = 0x10 + i
  const blobVec = buildValid({
    name: 'blob_random32_valid',
    spaceId: PRIMARY_SPACE,
    columnName: 'raw_hash',
    columnType: 'BLOB',
    hlc: '5/aaaaaaaaaaaaaaaa',
    signer: KEYS.primary!,
    storage: { kind: 'blob', value: blobBytes },
  })

  // Reject vectors: perturb the text-vector one field at a time.
  const rejectTamperedSig = buildTamperedSig(textVec)
  const rejectWrongSpace = buildWrongSpaceId(textVec)
  const rejectWrongDid = buildWrongAuthorDid(textVec)

  // Multi-space vectors: same (table, row, column, value, hlc), different
  // (spaceId, signer.did). Proves that the space_id and author_did fields
  // are load-bearing in the preimage — a leader who resigned an existing
  // row for a different space would need the target space's key.
  const sharedText: StorageValue = { kind: 'text', value: 'shared-value' }
  const multiSpaceA = buildValid({
    name: 'multi_space_primary_valid',
    spaceId: PRIMARY_SPACE,
    columnName: 'shared_col',
    columnType: 'TEXT',
    hlc: '6/aaaaaaaaaaaaaaaa',
    signer: KEYS.primary!,
    storage: sharedText,
  })
  const multiSpaceB = buildValid({
    name: 'multi_space_secondary_valid',
    spaceId: SECONDARY_SPACE,
    columnName: 'shared_col',
    columnType: 'TEXT',
    hlc: '6/aaaaaaaaaaaaaaaa',
    signer: KEYS.secondary!,
    storage: sharedText,
  })

  // Sanity: multi-space sigs must differ even when everything else matches,
  // otherwise the preimage does not actually incorporate spaceId + authorDid.
  if (multiSpaceA.sig === multiSpaceB.sig) {
    fail('multi-space sigs collided — spaceId/authorDid missing from preimage?')
  }

  return [
    nullVec,
    integerVec,
    realVec,
    textVec,
    blobVec,
    rejectTamperedSig,
    rejectWrongSpace,
    rejectWrongDid,
    multiSpaceA,
    multiSpaceB,
  ]
}

// ---------------------------------------------------------------------------
// Assemble output
// ---------------------------------------------------------------------------
function main(): void {
  const vectors = buildAllVectors()
  if (vectors.length !== 10) {
    fail(`expected 10 vectors, got ${vectors.length}`)
  }
  selfVerify(vectors)

  const out = {
    _readme: [
      'Cross-language column-sig verification fixture (Phase 1, Task I1).',
      'Consumed by Rust `src-tauri/tests/column_sig_vectors.rs` (I2) and',
      'TS `src/tests/sync/column-sig-vectors.test.ts` (I3).',
      'Every valid vector rebuilds preimage from (spaceId, tableName, rowPks,',
      'columnName, hlc, authorDid, valueBytes) via the domain-separated',
      'length-prefixed layout of `preimage.rs::build_preimage`.',
      'Reject vectors are ok-vectors with one field or the sig bit-flipped;',
      'all three converge to `VerifyColumnSigError::InvalidSignature` in Rust.',
      'Regenerate via `pnpm run gen:column-sig-vectors`; output is deterministic.',
    ].join(' '),
    domain_tag: DOMAIN_TAG,
    primary_space: PRIMARY_SPACE,
    secondary_space: SECONDARY_SPACE,
    primary_author_did: KEYS.primary!.did,
    secondary_author_did: KEYS.secondary!.did,
    vectors,
  }

  const __filename = fileURLToPath(import.meta.url)
  const __dirname = dirname(__filename)
  const outPath = resolve(
    __dirname,
    '..',
    'src-tauri',
    'tests',
    'fixtures',
    'column_sig_vectors.json',
  )
  mkdirSync(dirname(outPath), { recursive: true })
  writeFileSync(outPath, JSON.stringify(out, null, 2) + '\n', 'utf8')
  console.error(`wrote ${vectors.length} column-sig vectors to ${outPath}`)
}

// Suppress "declared but never used" warnings while keeping the tsx script
// self-contained. `base58btcEncode` and `createHash` are imported so future
// vectors that derive space_ids the same way ucan chain vectors do can use
// them without another import cycle.
void base58btcEncode
void createHash

main()
