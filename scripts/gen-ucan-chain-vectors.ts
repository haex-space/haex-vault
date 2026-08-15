#!/usr/bin/env tsx
/**
 * Cross-language UCAN chain verification vectors.
 *
 * Generates a fixture (`src-tauri/tests/fixtures/ucan_chain_vectors.json`)
 * consumed by both the TS UCAN library (`@haex-space/ucan`) and the Rust
 * verifier (Task 3: `walk_prf_chain`). The JSON snapshot is the source of
 * truth — the Rust test suite loads this file and asserts each vector's
 * `expected` outcome.
 *
 * DETERMINISM: all inputs (ed25519 seeds, timestamps, nonces) are fixed.
 * Running this script twice must produce byte-identical JSON. A CI
 * drift-check (future task) will diff committed JSON against a regen.
 *
 * WARNING: seeds below are TEST_ONLY_NEVER_PROD. Never use in a live vault.
 *
 * W4 PR-3 (Task 7) note: the payload wire form migrated from
 *   `cap: { "space:<id>": "space/<name>" }`         (hierarchical)
 * to
 *   `cap: { "space:<id>": [{cap, delegatable}, …] }`  (orthogonal).
 * `SpaceCap`, `CapEntry`, and `SpaceCapabilitySet` come from
 * `@haex-space/ucan` (0.2.0+). Aliased locally for shorter names in the
 * generator body.
 */
import {
  createHash,
  createPrivateKey,
  createPublicKey,
  sign as nodeSign,
  verify as nodeVerify,
  type KeyObject,
} from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  base58btcEncode,
  base64urlDecode,
  base64urlEncode,
  publicKeyToDid,
  spaceResource,
  type CapEntry,
  type EncodedUcan,
  type SpaceCap as Cap,
  type SpaceCapabilitySet as CapabilitySet,
} from '@haex-space/ucan'

// ---------------------------------------------------------------------------
// Deterministic ed25519 test keys (TEST_ONLY_NEVER_PROD).
//
// Each seed is a distinct 32-byte block. Node's crypto derives the ed25519
// public key from the seed; both are used to construct the DID (did:key:z...)
// and to sign tokens.
// ---------------------------------------------------------------------------
const SEEDS_HEX: Record<string, string> = {
  root: '0101010101010101010101010101010101010101010101010101010101010101',
  admin1: '0202020202020202020202020202020202020202020202020202020202020202',
  admin2: '0303030303030303030303030303030303030303030303030303030303030303',
  admin3: '0404040404040404040404040404040404040404040404040404040404040404',
  member: '0505050505050505050505050505050505050505050505050505050505050505',
  otherRoot: '0606060606060606060606060606060606060606060606060606060606060606',
  wrongIssuer: '0707070707070707070707070707070707070707070707070707070707070707',
}

// PKCS8 DER prefix for a raw Ed25519 private key seed (16 bytes header,
// followed by the 32-byte seed = 48 bytes total).
const ED25519_PKCS8_HEADER = Buffer.from('302e020100300506032b657004220420', 'hex')

interface Key {
  name: string
  seedHex: string
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
  // SPKI wrapper for Ed25519 is 12 bytes, then 32 raw bytes.
  const rawPub = new Uint8Array(spki.subarray(12))
  if (rawPub.length !== 32) throw new Error(`unexpected pubkey length for ${name}`)
  const did = publicKeyToDid(rawPub)
  return { name, seedHex, priv, rawPub, did }
}

const KEYS: Record<string, Key> = Object.fromEntries(
  Object.entries(SEEDS_HEX).map(([n, s]) => [n, makeKey(n, s)]),
)

// ---------------------------------------------------------------------------
// space_id derivation
//
// !!! MUST STAY BYTE-IDENTICAL WITH `src/utils/auth/spaceId.ts` and
// `src-tauri/src/ucan/space_id.rs`. The Phase-0 fixture
// `src-tauri/tests/fixtures/space_id_vectors.json` guards those two impls;
// this inline copy is duplicated on purpose so the generator can run
// standalone (outside the Nuxt TS context). If the algorithm ever changes,
// update all three call sites together.
// ---------------------------------------------------------------------------
const SPACE_ID_DOMAIN_TAG = 'haex/space-id/v1'
const NONCE_LEN = 16
const HASH_LEN = 16

function deriveSpaceId(rootDid: string, nonce: Uint8Array): string {
  if (nonce.length !== NONCE_LEN) throw new Error(`nonce must be ${NONCE_LEN} bytes`)
  const domainBytes = new TextEncoder().encode(SPACE_ID_DOMAIN_TAG)
  const didBytes = new TextEncoder().encode(rootDid)
  const preimage = Buffer.concat([
    Buffer.from(domainBytes),
    Buffer.from(nonce),
    Buffer.from(didBytes),
  ])
  const full = createHash('sha256').update(preimage).digest()
  const hashPart = new Uint8Array(full.subarray(0, HASH_LEN))
  const buf = new Uint8Array(NONCE_LEN + HASH_LEN)
  buf.set(nonce, 0)
  buf.set(hashPart, NONCE_LEN)
  return base58btcEncode(buf)
}

/**
 * Cross-check the generator's inlined `deriveSpaceId` against the
 * Phase-0 fixture (`space_id_vectors.json`), which is the authoritative
 * guard for the TS (`src/utils/auth/spaceId.ts`) and Rust
 * (`src-tauri/src/ucan/space_id.rs`) implementations. If any of the three
 * copies of the algorithm drifts, the fixture generation must fail loudly
 * — otherwise a wrong `space_id` would silently poison every chain vector
 * and `ucan_chain_vectors.rs` would still validate internally-consistent
 * garbage.
 */
function assertSpaceIdAgainstFixture(): void {
  const fixturePath = resolve(
    dirname(fileURLToPath(import.meta.url)),
    '..',
    'src-tauri',
    'tests',
    'fixtures',
    'space_id_vectors.json',
  )
  const parsed = JSON.parse(readFileSync(fixturePath, 'utf8')) as {
    domain_tag: string
    vectors: Array<{
      name: string
      root_did: string
      nonce_hex: string
      expected_space_id: string
    }>
  }
  if (parsed.domain_tag !== SPACE_ID_DOMAIN_TAG) {
    fail(
      `space_id domain_tag drift: fixture=${parsed.domain_tag} generator=${SPACE_ID_DOMAIN_TAG}`,
    )
  }
  for (const v of parsed.vectors) {
    const nonce = Buffer.from(v.nonce_hex, 'hex')
    if (nonce.length !== NONCE_LEN) {
      fail(`space_id fixture ${v.name}: nonce_hex is not ${NONCE_LEN} bytes`)
    }
    const got = deriveSpaceId(v.root_did, new Uint8Array(nonce))
    if (got !== v.expected_space_id) {
      fail(
        `space_id fixture ${v.name} drift: expected=${v.expected_space_id} got=${got}`,
      )
    }
  }
  console.error(`deriveSpaceId matches ${parsed.vectors.length} fixture vectors.`)
}

// ---------------------------------------------------------------------------
// Orthogonal capability set (W4 PR-3 wire form)
//
// Encoded on the wire as a JSON array of `{cap, delegatable}` entries, one
// per held capability, sorted by cap discriminant (read < write < invite
// < admin) with no duplicates. Mirrors
// `src-tauri/src/ucan/capability_set.rs::{Cap, CapEntry, CapabilitySet}`.
// Types come from `@haex-space/ucan` (aliased at the import above).
// ---------------------------------------------------------------------------
const CAP_ORDER: Record<Cap, number> = { read: 1, write: 2, invite: 3, admin: 4 }

/** Canonicalise a `CapEntry[]` to the wire form (sorted, no duplicates). */
function capSet(entries: CapEntry[]): CapabilitySet {
  const seen = new Set<Cap>()
  for (const e of entries) {
    if (seen.has(e.cap)) throw new Error(`duplicate cap in set: ${e.cap}`)
    seen.add(e.cap)
  }
  return [...entries].sort((a, b) => CAP_ORDER[a.cap] - CAP_ORDER[b.cap])
}

/** All four caps, delegatable=true. Handy for root / intermediate admin tokens. */
function fullDelegatableSet(): CapabilitySet {
  return capSet([
    { cap: 'read', delegatable: true },
    { cap: 'write', delegatable: true },
    { cap: 'invite', delegatable: true },
    { cap: 'admin', delegatable: true },
  ])
}

/** Single-cap set. `delegatable` defaults to `false` (typical for leaves). */
function only(cap: Cap, delegatable = false): CapabilitySet {
  return capSet([{ cap, delegatable }])
}

// ---------------------------------------------------------------------------
// UCAN construction (deterministic: caller supplies iat/exp/nonce explicitly).
//
// We build the payload manually rather than call `createUcan` because that
// function pulls iat/nnc from `Date.now()` and `crypto.getRandomValues()`.
// Everything else (header, encoding, signing input) mirrors the library.
// ---------------------------------------------------------------------------
const HEADER = { alg: 'EdDSA' as const, typ: 'JWT' as const }

interface TokenSpec {
  key: Key
  audience: string
  spaceId: string
  capSet: CapabilitySet
  exp: number
  iat: number
  nnc: string
  prf: EncodedUcan[]
}

function encodeJson(obj: unknown): string {
  return base64urlEncode(new TextEncoder().encode(JSON.stringify(obj)))
}

function makeToken(spec: TokenSpec): EncodedUcan {
  const payload = {
    ucv: '1.0',
    iss: spec.key.did,
    aud: spec.audience,
    // W4 PR-3: `cap` value is a CapabilitySet array.
    cap: { [spaceResource(spec.spaceId)]: spec.capSet },
    exp: spec.exp,
    iat: spec.iat,
    prf: spec.prf,
    nnc: spec.nnc,
  }
  const headerB64 = encodeJson(HEADER)
  const payloadB64 = encodeJson(payload)
  const signingInput = Buffer.from(`${headerB64}.${payloadB64}`, 'utf8')
  const sig = nodeSign(null, signingInput, spec.key.priv)
  const sigB64 = base64urlEncode(new Uint8Array(sig))
  return `${headerB64}.${payloadB64}.${sigB64}`
}

/**
 * Flip one byte in the signature portion of an encoded UCAN so the ed25519
 * signature no longer verifies. Returns the tampered token and the byte
 * offset into the decoded signature that was flipped (documented in the
 * vector so downstream tests can reproduce the mutation).
 */
function flipSignatureByte(
  token: EncodedUcan,
  offset: number,
): { token: EncodedUcan; offset: number } {
  const parts = token.split('.')
  if (parts.length !== 3) throw new Error('invalid token')
  const [h, p, s] = parts
  const sigBytes = base64urlDecode(s!)
  if (offset < 0 || offset >= sigBytes.length) throw new Error('offset out of range')
  const flipped = new Uint8Array(sigBytes)
  flipped[offset] = flipped[offset]! ^ 0x01
  return { token: `${h}.${p}.${base64urlEncode(flipped)}`, offset }
}

/**
 * Verify a token's ed25519 signature against a given raw public key.
 * Used by the self-verification pass to prove that OK vectors really are
 * signature-valid and that tampered vectors really do break the signature.
 */
function verifyTokenSig(token: EncodedUcan, rawPub: Uint8Array): boolean {
  const parts = token.split('.')
  if (parts.length !== 3) return false
  const [h, p, s] = parts
  const sig = base64urlDecode(s!)
  const signingInput = Buffer.from(`${h}.${p}`, 'utf8')
  // Wrap raw 32-byte pubkey in SPKI so node's verify accepts it.
  const spkiHeader = Buffer.from('302a300506032b6570032100', 'hex')
  const spki = Buffer.concat([spkiHeader, Buffer.from(rawPub)])
  const pubKey = createPublicKey({ key: spki, format: 'der', type: 'spki' })
  return nodeVerify(null, signingInput, pubKey, sig)
}

interface DecodedPayload {
  ucv: string
  iss: string
  aud: string
  cap: Record<string, CapabilitySet>
  exp: number
  iat: number
  nnc: string
  prf: string[]
}

function decodePayload(token: EncodedUcan): DecodedPayload {
  const parts = token.split('.')
  if (parts.length !== 3) throw new Error('invalid token')
  const payloadBytes = base64urlDecode(parts[1]!)
  return JSON.parse(new TextDecoder().decode(payloadBytes)) as DecodedPayload
}

// ---------------------------------------------------------------------------
// Fixed reference clock and timestamps.
// ---------------------------------------------------------------------------
const IAT_ALL = 1_700_000_000 // 2023-11-14T22:13:20Z
const FAR_FUTURE_EXP = 4_000_000_000 // 2096; well past any realistic test run
const PAST_EXP = 1_600_000_000 // 2020; always in the past at test time
// Used ONLY by the Expired* vectors so their tokens satisfy the UCAN
// spec-standard invariant iat <= exp (Task 3's walker may enforce it and
// would otherwise trip a different error branch than the Expired one).
const PAST_IAT = 1_500_000_000 // 2017-07-14; strictly older than PAST_EXP

// Space-id nonces (16 bytes each). primary_space is used by most vectors;
// other_space is used only by the wrong_space_in_delegate vector.
const NONCE_HEX: Record<string, string> = {
  primary_space: '0102030405060708090a0b0c0d0e0f10',
  other_space: '1112131415161718191a1b1c1d1e1f20',
}

function nonceBytes(name: string): Uint8Array {
  return new Uint8Array(Buffer.from(NONCE_HEX[name]!, 'hex'))
}

/**
 * Derive a deterministic 12-byte UCAN nonce (nnc) from a label. Distinct
 * per-token so tokens differ even when other fields collide.
 */
function ucanNnc(label: string): string {
  const bytes = createHash('sha256').update(`nnc:${label}`).digest().subarray(0, 12)
  return base64urlEncode(new Uint8Array(bytes))
}

// ---------------------------------------------------------------------------
// Vector shape (matches Rust fixture expectations)
// ---------------------------------------------------------------------------
interface VectorChainNode {
  iss: string
  aud: string
  cap_set: CapabilitySet
  space_id: string
  exp: number
  proofs: EncodedUcan[]
  signed_token: EncodedUcan
}

type ExpectedError =
  | 'ChainTooDeep'
  | 'Signature'
  | 'DelegationMissing'
  | 'DelegationNotDelegatable'
  | 'RootBindingMismatch'
  | 'RootNotSelfSigned'
  | 'ChainBroken'
  | 'Expired'
  | 'WrongSpace'

interface Vector {
  name: string
  space_id: string
  nonce_hex: string
  root_did: string
  expected_audience: string
  /**
   * Bare cap name (`"read" | "write" | "invite" | "admin"`) the leaf token
   * must satisfy. Rust runners consume this via `cap_from_str`, which
   * still tolerates a `"space/"` prefix but the fixture now emits the
   * unprefixed form.
   */
  capability_needed: Cap
  chain: VectorChainNode[]
  expected:
    | { ok: true; resolved_root_did: string }
    | { ok: false; error: ExpectedError; tampered_signature_byte_offset?: number }
}

// Convenience: primary space_id derived once, reused across most vectors.
const primarySpaceId = deriveSpaceId(KEYS.root!.did, nonceBytes('primary_space'))
const otherSpaceId = deriveSpaceId(KEYS.root!.did, nonceBytes('other_space'))
const mismatchSpaceId = deriveSpaceId(KEYS.otherRoot!.did, nonceBytes('primary_space'))

// Encode a chain's tokens as VectorChainNode entries.
function encodeChain(nodes: Array<{ spec: TokenSpec }>): VectorChainNode[] {
  return nodes.map(({ spec }) => {
    const token = makeToken(spec)
    return {
      iss: spec.key.did,
      aud: spec.audience,
      cap_set: spec.capSet,
      space_id: spec.spaceId,
      exp: spec.exp,
      proofs: spec.prf,
      signed_token: token,
    }
  })
}

// ---------------------------------------------------------------------------
// Vector builders
// ---------------------------------------------------------------------------

function vRootOnlyValid(): Vector {
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('root_only_valid.root'),
    prf: [],
  }
  const chain = encodeChain([{ spec: root }])
  return {
    name: 'root_only_valid',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.root!.did,
    capability_needed: 'admin',
    chain,
    expected: { ok: true, resolved_root_did: KEYS.root!.did },
  }
}

function vTwoHopValid(): Vector {
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('two_hop.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const leaf: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('two_hop.leaf'),
    prf: [rootToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: leaf }])
  return {
    name: 'two_hop_valid',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: true, resolved_root_did: KEYS.root!.did },
  }
}

function vThreeHopValid(): Vector {
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('three_hop.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const mid: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('three_hop.mid'),
    prf: [rootToken],
  }
  const midToken = makeToken(mid)
  const leaf: TokenSpec = {
    key: KEYS.admin1!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('three_hop.leaf'),
    prf: [midToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: mid }, { spec: leaf }])
  return {
    name: 'three_hop_valid',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: true, resolved_root_did: KEYS.root!.did },
  }
}

function vFiveHopValidAtMax(): Vector {
  // Chain: root(self) → admin1 → admin2 → admin3 → member.
  // All intermediate hops carry the full delegatable set; the leaf claims
  // only `write` so the fixture exercises attenuation at each hop under
  // the orthogonal model.
  const specs: TokenSpec[] = [
    {
      key: KEYS.root!,
      audience: KEYS.root!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h0'),
      prf: [],
    },
    {
      key: KEYS.root!,
      audience: KEYS.admin1!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h1'),
      prf: [],
    },
    {
      key: KEYS.admin1!,
      audience: KEYS.admin2!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h2'),
      prf: [],
    },
    {
      key: KEYS.admin2!,
      audience: KEYS.admin3!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h3'),
      prf: [],
    },
    {
      key: KEYS.admin3!,
      audience: KEYS.member!.did,
      spaceId: primarySpaceId,
      capSet: only('write'),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h4'),
      prf: [],
    },
  ]
  const signed: EncodedUcan[] = []
  for (let i = 0; i < specs.length; i++) {
    if (i > 0) specs[i]!.prf = [signed[i - 1]!]
    signed.push(makeToken(specs[i]!))
  }
  const chain = encodeChain(specs.map((s) => ({ spec: s })))
  return {
    name: 'five_hop_valid_at_max',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: true, resolved_root_did: KEYS.root!.did },
  }
}

function vSixHopExceedsMax(): Vector {
  // Same shape as five_hop but with one extra admin hop, exceeding MAX=5.
  const specs: TokenSpec[] = [
    {
      key: KEYS.root!,
      audience: KEYS.root!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h0'),
      prf: [],
    },
    {
      key: KEYS.root!,
      audience: KEYS.admin1!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h1'),
      prf: [],
    },
    {
      key: KEYS.admin1!,
      audience: KEYS.admin2!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h2'),
      prf: [],
    },
    {
      key: KEYS.admin2!,
      audience: KEYS.admin3!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h3'),
      prf: [],
    },
    {
      key: KEYS.admin3!,
      audience: KEYS.wrongIssuer!.did,
      spaceId: primarySpaceId,
      capSet: fullDelegatableSet(),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h4'),
      prf: [],
    },
    {
      key: KEYS.wrongIssuer!,
      audience: KEYS.member!.did,
      spaceId: primarySpaceId,
      capSet: only('write'),
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h5'),
      prf: [],
    },
  ]
  const signed: EncodedUcan[] = []
  for (let i = 0; i < specs.length; i++) {
    if (i > 0) specs[i]!.prf = [signed[i - 1]!]
    signed.push(makeToken(specs[i]!))
  }
  const chain = encodeChain(specs.map((s) => ({ spec: s })))
  return {
    name: 'six_hop_exceeds_max',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'ChainTooDeep' },
  }
}

function vTamperedLeafSignature(): Vector {
  // Build a valid two-hop chain, then flip a byte in the leaf's signature.
  // The middle byte offset is documented in the vector so downstream tests
  // can reproduce the tampering site.
  const base = vTwoHopValid()
  const leafIdx = base.chain.length - 1
  const tampered = flipSignatureByte(base.chain[leafIdx]!.signed_token, 10)
  const chain = base.chain.map((n, i) =>
    i === leafIdx ? { ...n, signed_token: tampered.token } : n,
  )
  return {
    name: 'tampered_leaf_signature',
    space_id: base.space_id,
    nonce_hex: base.nonce_hex,
    root_did: base.root_did,
    expected_audience: base.expected_audience,
    capability_needed: base.capability_needed,
    chain,
    expected: { ok: false, error: 'Signature', tampered_signature_byte_offset: tampered.offset },
  }
}

function vTamperedMiddleSignature(): Vector {
  // Build a valid three-hop chain but rebuild the leaf so its `prf` embeds
  // the tampered middle. The Rust verifier walks by parsing `leaf.prf[0]`
  // (the middle token EMBEDDED in the leaf's signed payload) — the outer
  // `chain[1].signed_token` is a display / debugging artefact, not the
  // input the walker consumes. Embedding the tampered middle in the leaf's
  // prf ensures the walker sees the bad signature and rejects.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('tampered_mid.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const mid: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('tampered_mid.mid'),
    prf: [rootToken],
  }
  const midToken = makeToken(mid)
  const tampered = flipSignatureByte(midToken, 5)
  // The leaf's `prf` claim carries the TAMPERED middle. Once the leaf is
  // signed, that tampering is baked into the leaf's payload — the walker
  // reads it back verbatim when parsing `leaf.prf[0]`.
  const leaf: TokenSpec = {
    key: KEYS.admin1!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('tampered_mid.leaf'),
    prf: [tampered.token],
  }
  const leafToken = makeToken(leaf)
  const chain: VectorChainNode[] = [
    {
      iss: root.key.did,
      aud: root.audience,
      cap_set: root.capSet,
      space_id: root.spaceId,
      exp: root.exp,
      proofs: [],
      signed_token: rootToken,
    },
    {
      iss: mid.key.did,
      aud: mid.audience,
      cap_set: mid.capSet,
      space_id: mid.spaceId,
      exp: mid.exp,
      proofs: [rootToken],
      signed_token: tampered.token,
    },
    {
      iss: leaf.key.did,
      aud: leaf.audience,
      cap_set: leaf.capSet,
      space_id: leaf.spaceId,
      exp: leaf.exp,
      proofs: [tampered.token],
      signed_token: leafToken,
    },
  ]
  return {
    name: 'tampered_middle_signature',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'Signature', tampered_signature_byte_offset: tampered.offset },
  }
}

function vDelegationMissing(): Vector {
  // Chain: root(full delegatable) → mid(write-only, delegatable=true) →
  // leaf(claims admin).
  // Semantic: mid never held `admin`, so leaf's claim to admin is
  // orthogonally missing — parent does not hold this capability at all.
  // Previously modelled as `CapabilityEscalation` under the hierarchical
  // read < write < invite < admin lattice; the new orthogonal model
  // classifies the failure as `DelegationMissing` (Missing arm).
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('delegation_missing.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const mid: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did,
    spaceId: primarySpaceId,
    capSet: only('write', true),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('delegation_missing.mid'),
    prf: [rootToken],
  }
  const midToken = makeToken(mid)
  const leaf: TokenSpec = {
    key: KEYS.admin1!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('admin'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('delegation_missing.leaf'),
    prf: [midToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: mid }, { spec: leaf }])
  return {
    name: 'delegation_missing_admin_child_from_write_parent',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'admin',
    chain,
    expected: { ok: false, error: 'DelegationMissing' },
  }
}

function vDelegationNotDelegatable(): Vector {
  // Chain: root(full delegatable) → mid(write, delegatable=false) →
  // leaf(claims write).
  // Semantic: mid may exercise `write` locally, but its non-delegatable
  // flag forbids passing it further. Leaf's identical claim triggers the
  // `DelegationNotDelegatable` arm.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('delegation_not_delegatable.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const mid: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did,
    spaceId: primarySpaceId,
    capSet: only('write', false),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('delegation_not_delegatable.mid'),
    prf: [rootToken],
  }
  const midToken = makeToken(mid)
  const leaf: TokenSpec = {
    key: KEYS.admin1!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('delegation_not_delegatable.leaf'),
    prf: [midToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: mid }, { spec: leaf }])
  return {
    name: 'delegation_not_delegatable_write_under_non_delegatable_parent',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'DelegationNotDelegatable' },
  }
}

function vOrthogonalMissingCap(): Vector {
  // Chain: root(full delegatable) → mid(write only, delegatable=true) →
  // leaf(claims read).
  // Semantic: mid holds `write` but not `read`. The old hierarchical
  // model would have accepted this because `write` implies `read`; the
  // orthogonal model rejects because `read` is not present in the
  // parent's set (`DelegationMissing`).
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('orthogonal_missing.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const mid: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did,
    spaceId: primarySpaceId,
    capSet: only('write', true),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('orthogonal_missing.mid'),
    prf: [rootToken],
  }
  const midToken = makeToken(mid)
  const leaf: TokenSpec = {
    key: KEYS.admin1!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('read'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('orthogonal_missing.leaf'),
    prf: [midToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: mid }, { spec: leaf }])
  return {
    name: 'orthogonal_missing_cap_read_child_under_write_parent',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'read',
    chain,
    expected: { ok: false, error: 'DelegationMissing' },
  }
}

function vWrongRootDidBindingMismatch(): Vector {
  // space_id is derived from (otherRoot.did, primary_nonce), but the chain's
  // root is issued by KEYS.root. Verifier re-derives with root.iss and finds
  // the hash tail doesn't match → binding rejected.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: mismatchSpaceId, // NOTE: bound to otherRoot.did, not root.did
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('binding_mismatch.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const leaf: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.member!.did,
    spaceId: mismatchSpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('binding_mismatch.leaf'),
    prf: [rootToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: leaf }])
  return {
    name: 'wrong_root_did_binding_mismatch',
    space_id: mismatchSpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'RootBindingMismatch' },
  }
}

function vRootNotSelfSigned(): Vector {
  // Chain starts with a token where iss != aud (issued to someone else),
  // yet has no proofs. Verifier requires the top of the proof chain to be
  // self-signed to authenticate the space.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did, // != root.did → not self-signed
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('not_self_signed.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const leaf: TokenSpec = {
    key: KEYS.admin1!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('not_self_signed.leaf'),
    prf: [rootToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: leaf }])
  return {
    name: 'root_not_self_signed',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'RootNotSelfSigned' },
  }
}

function vChainBrokenAudMismatch(): Vector {
  // root(A→A), mid(A→B), leaf(C→D). Leaf.iss=C ≠ mid.aud=B → chain broken.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('chain_broken.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const mid: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did, // mid.aud = admin1
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('chain_broken.mid'),
    prf: [rootToken],
  }
  const midToken = makeToken(mid)
  const leaf: TokenSpec = {
    key: KEYS.wrongIssuer!, // leaf.iss = wrongIssuer, NOT admin1 → break
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('chain_broken.leaf'),
    prf: [midToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: mid }, { spec: leaf }])
  return {
    name: 'chain_broken_aud_mismatch',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'ChainBroken' },
  }
}

function vExpiredLeaf(): Vector {
  // Root valid, leaf.exp = past.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('expired_leaf.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const leaf: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: PAST_EXP,
    iat: PAST_IAT, // iat < exp so this trips the Expired branch, not iat>exp sanity
    nnc: ucanNnc('expired_leaf.leaf'),
    prf: [rootToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: leaf }])
  return {
    name: 'expired_leaf',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'Expired' },
  }
}

function vExpiredRoot(): Vector {
  // Root.exp = past, leaf.exp = future.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: PAST_EXP,
    iat: PAST_IAT, // iat < exp so this trips the Expired branch, not iat>exp sanity
    nnc: ucanNnc('expired_root.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const leaf: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('expired_root.leaf'),
    prf: [rootToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: leaf }])
  return {
    name: 'expired_root',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'Expired' },
  }
}

function vWrongSpaceInDelegate(): Vector {
  // root(space=X, full) → mid(space=Y, full) → leaf(space=X, write).
  // Mid delegates on the wrong space so the leaf's claim on X is not
  // covered by any parent proof.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('wrong_space.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const mid: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did,
    spaceId: otherSpaceId, // wrong space
    capSet: fullDelegatableSet(),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('wrong_space.mid'),
    prf: [rootToken],
  }
  const midToken = makeToken(mid)
  const leaf: TokenSpec = {
    key: KEYS.admin1!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    capSet: only('write'),
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('wrong_space.leaf'),
    prf: [midToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: mid }, { spec: leaf }])
  return {
    name: 'wrong_space_in_delegate',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'write',
    chain,
    expected: { ok: false, error: 'WrongSpace' },
  }
}

// ---------------------------------------------------------------------------
// Self-verification
// ---------------------------------------------------------------------------

// Cache raw pubkeys by DID for signature checks.
const didToPubKey = new Map<string, Uint8Array>()
for (const k of Object.values(KEYS)) didToPubKey.set(k.did, k.rawPub)

function fail(msg: string): never {
  console.error(`Self-verification FAILED: ${msg}`)
  process.exit(1)
}

/** Serialised comparison of two CapabilitySet arrays. */
function capSetEqual(a: CapabilitySet, b: CapabilitySet): boolean {
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) {
    if (a[i]!.cap !== b[i]!.cap) return false
    if (a[i]!.delegatable !== b[i]!.delegatable) return false
  }
  return true
}

/** Does `parent` hold every cap `child` holds, each with delegatable=true? */
function parentCanDelegateChild(
  parent: CapabilitySet,
  child: CapabilitySet,
): { ok: true } | { ok: false; kind: 'Missing' | 'NotDelegatable'; cap: Cap } {
  for (const c of child) {
    const p = parent.find((e) => e.cap === c.cap)
    if (!p) return { ok: false, kind: 'Missing', cap: c.cap }
    if (!p.delegatable) return { ok: false, kind: 'NotDelegatable', cap: c.cap }
  }
  return { ok: true }
}

function selfVerify(vectors: Vector[]): void {
  for (const v of vectors) {
    const decoded = v.chain.map((n) => decodePayload(n.signed_token))

    // Cross-check outer JSON fields against the decoded token payload for
    // every node in every vector (both ok=true and ok=false). The Rust
    // verifier may consume EITHER the outer JSON shape OR the decoded
    // signed_token payload; both surfaces must agree byte-for-byte, or a
    // copy-paste refactor of one side would silently drift from the other.
    for (let i = 0; i < v.chain.length; i++) {
      const outer = v.chain[i]!
      const p = decoded[i]!
      if (outer.iss !== p.iss) {
        fail(`${v.name}: chain[${i}] outer.iss ${outer.iss} != decoded.iss ${p.iss}`)
      }
      if (outer.aud !== p.aud) {
        fail(`${v.name}: chain[${i}] outer.aud ${outer.aud} != decoded.aud ${p.aud}`)
      }
      if (outer.exp !== p.exp) {
        fail(`${v.name}: chain[${i}] outer.exp ${outer.exp} != decoded.exp ${p.exp}`)
      }
      const resourceKey = spaceResource(outer.space_id)
      const capKeys = Object.keys(p.cap)
      if (capKeys.length !== 1 || capKeys[0] !== resourceKey) {
        fail(
          `${v.name}: chain[${i}] decoded.cap keys ${JSON.stringify(capKeys)} do not match [${resourceKey}] derived from outer.space_id`,
        )
      }
      const decodedSet = p.cap[resourceKey]!
      if (!capSetEqual(outer.cap_set, decodedSet)) {
        fail(
          `${v.name}: chain[${i}] outer.cap_set ${JSON.stringify(outer.cap_set)} != decoded[${resourceKey}] ${JSON.stringify(decodedSet)}`,
        )
      }
      if (outer.proofs.length !== p.prf.length) {
        fail(
          `${v.name}: chain[${i}] outer.proofs.length ${outer.proofs.length} != decoded.prf.length ${p.prf.length}`,
        )
      }
      for (let j = 0; j < outer.proofs.length; j++) {
        if (outer.proofs[j] !== p.prf[j]) {
          fail(`${v.name}: chain[${i}] outer.proofs[${j}] != decoded.prf[${j}]`)
        }
      }
    }

    // Signature-of-issuer check (for tampered vectors, this should fail
    // on the tampered node; we verify per-node below).
    const sigOk = v.chain.map((n) => {
      const p = decodePayload(n.signed_token)
      const pub = didToPubKey.get(p.iss)
      if (!pub) fail(`${v.name}: unknown DID ${p.iss}`)
      return verifyTokenSig(n.signed_token, pub!)
    })

    if (v.expected.ok) {
      // All sigs must verify.
      for (let i = 0; i < sigOk.length; i++) {
        if (!sigOk[i]) fail(`${v.name}: node ${i} signature failed but vector is ok=true`)
      }
      // Chain edges: chain[i-1].aud must equal chain[i].iss.
      for (let i = 1; i < v.chain.length; i++) {
        if (decoded[i - 1]!.aud !== decoded[i]!.iss) {
          fail(`${v.name}: chain-edge broken at i=${i} but vector is ok=true`)
        }
      }
      // Root must be self-signed with Admin.
      if (decoded[0]!.iss !== decoded[0]!.aud) {
        fail(`${v.name}: root not self-signed but vector is ok=true`)
      }
      const rootResource = spaceResource(v.space_id)
      const rootSet = decoded[0]!.cap[rootResource]
      if (!rootSet || !rootSet.some((e) => e.cap === 'admin')) {
        fail(`${v.name}: root does not hold admin but vector is ok=true`)
      }
      // space_id binding on root.
      const nonce = new Uint8Array(Buffer.from(v.nonce_hex, 'hex'))
      if (deriveSpaceId(decoded[0]!.iss, nonce) !== v.space_id) {
        fail(`${v.name}: space_id binding mismatch but vector is ok=true`)
      }
      // Delegation attenuation across the chain (parent must hold every
      // child cap with delegatable=true).
      for (let i = 1; i < v.chain.length; i++) {
        const parentSet = decoded[i - 1]!.cap[rootResource]
        const childSet = decoded[i]!.cap[rootResource]
        if (!parentSet || !childSet) {
          fail(`${v.name}: missing capability entry at i=${i}`)
        }
        const check = parentCanDelegateChild(parentSet!, childSet!)
        if (!check.ok) {
          fail(
            `${v.name}: attenuation broken at i=${i} (${check.kind} ${check.cap}), but vector is ok=true`,
          )
        }
      }
      // No expiry in the past for any node.
      for (let i = 0; i < decoded.length; i++) {
        if (decoded[i]!.exp <= IAT_ALL) {
          fail(`${v.name}: node ${i} exp <= iat_ref but vector is ok=true`)
        }
      }
    } else {
      // For each bad-vector, assert that the specified tampering is real.
      const err = v.expected.error
      if (err === 'Signature') {
        if (sigOk.every((ok) => ok)) fail(`${v.name}: expected Signature error but all sigs verify`)
      } else if (err === 'ChainTooDeep') {
        if (v.chain.length <= 5) fail(`${v.name}: chain length ${v.chain.length} not > 5`)
      } else if (err === 'DelegationMissing' || err === 'DelegationNotDelegatable') {
        // Walk pairwise from root to leaf: find the first hop where the
        // parent cannot delegate the child's set. That hop's failure
        // reason must match the declared error kind.
        const resource = spaceResource(v.space_id)
        let matched = false
        for (let i = 1; i < v.chain.length; i++) {
          const parentSet = decoded[i - 1]!.cap[resource]
          const childSet = decoded[i]!.cap[resource]
          if (!parentSet || !childSet) {
            fail(`${v.name}: hop ${i} missing capability entry`)
          }
          const check = parentCanDelegateChild(parentSet!, childSet!)
          if (check.ok) continue
          const expectedKind = err === 'DelegationMissing' ? 'Missing' : 'NotDelegatable'
          if (check.kind !== expectedKind) {
            fail(
              `${v.name}: first-offender hop kind ${check.kind}, expected ${expectedKind}`,
            )
          }
          matched = true
          break
        }
        if (!matched) fail(`${v.name}: expected ${err} but no attenuation hop failed`)
      } else if (err === 'RootBindingMismatch') {
        const nonce = new Uint8Array(Buffer.from(v.nonce_hex, 'hex'))
        // The chain's root DID should NOT derive to v.space_id.
        if (deriveSpaceId(decoded[0]!.iss, nonce) === v.space_id) {
          fail(`${v.name}: expected binding mismatch but derive matches`)
        }
      } else if (err === 'RootNotSelfSigned') {
        if (decoded[0]!.iss === decoded[0]!.aud) {
          fail(`${v.name}: expected non-self-signed root but iss==aud`)
        }
      } else if (err === 'ChainBroken') {
        let broken = false
        for (let i = 1; i < v.chain.length; i++) {
          if (decoded[i - 1]!.aud !== decoded[i]!.iss) {
            broken = true
            break
          }
        }
        if (!broken) fail(`${v.name}: expected chain-broken but no edge mismatches`)
      } else if (err === 'Expired') {
        // Some node exp is <= a plausible "now" (any wall-clock will be > 2020).
        const anyPast = decoded.some((p) => p.exp <= PAST_EXP)
        if (!anyPast) fail(`${v.name}: expected expired but no exp is in the past`)
      } else if (err === 'WrongSpace') {
        const anyMismatch = decoded.some(
          (p) => Object.keys(p.cap)[0] !== spaceResource(v.space_id),
        )
        if (!anyMismatch) fail(`${v.name}: expected wrong-space but all nodes reference v.space_id`)
      } else {
        fail(`${v.name}: unknown expected error ${err}`)
      }
    }
  }
  console.error(`Self-verification passed for ${vectors.length} vectors.`)
}

// ---------------------------------------------------------------------------
// Assemble and write output
// ---------------------------------------------------------------------------

function main(): void {
  // Guard against silent drift in the standalone deriveSpaceId copy.
  // Without this, a wrong space_id would poison every generated chain vector
  // and Rust's ucan_chain_vectors.rs would still see internal consistency.
  assertSpaceIdAgainstFixture()

  const vectors: Vector[] = [
    vRootOnlyValid(),
    vTwoHopValid(),
    vThreeHopValid(),
    vFiveHopValidAtMax(),
    vSixHopExceedsMax(),
    vTamperedLeafSignature(),
    vTamperedMiddleSignature(),
    vDelegationMissing(),
    vDelegationNotDelegatable(),
    vOrthogonalMissingCap(),
    vWrongRootDidBindingMismatch(),
    vRootNotSelfSigned(),
    vChainBrokenAudMismatch(),
    vExpiredLeaf(),
    vExpiredRoot(),
    vWrongSpaceInDelegate(),
  ]

  if (vectors.length !== 16) {
    fail(`expected 16 vectors, got ${vectors.length}`)
  }

  selfVerify(vectors)

  const out = {
    _readme: [
      'Cross-language UCAN chain verification fixture. Consumed by both the',
      'TS UCAN library (`@haex-space/ucan`) and the Rust verifier',
      '(`walk_prf_chain`, Task 3).',
      'W4 PR-3 wire form: payload field is `cap`, value is',
      'a CapabilitySet — a sorted array of `{cap, delegatable}` entries.',
      'chain[0] = root token (self-signed for OK vectors).',
      'chain[chain.length - 1] = leaf token.',
      'Each chain[i].signed_token is a complete encoded UCAN JWT.',
      'chain[i].proofs is the exact prf array embedded in that token.',
      'Errors correspond to Rust `UcanVerifyError` variants introduced in Task 3.',
      'Domain tag `haex/space-id/v1` matches Phase-0 space_id derivation.',
      'Regenerate via `pnpm run gen:ucan-vectors`; output is byte-deterministic.',
    ].join(' '),
    domain_tag: SPACE_ID_DOMAIN_TAG,
    reference_iat: IAT_ALL,
    far_future_exp: FAR_FUTURE_EXP,
    past_exp: PAST_EXP,
    vectors,
  }

  const __filename = fileURLToPath(import.meta.url)
  const __dirname = dirname(__filename)
  const outPath = resolve(__dirname, '..', 'src-tauri', 'tests', 'fixtures', 'ucan_chain_vectors.json')
  mkdirSync(dirname(outPath), { recursive: true })
  writeFileSync(outPath, JSON.stringify(out, null, 2) + '\n', 'utf8')
  console.error(`Wrote ${vectors.length} vectors to ${outPath}`)
}

main()
