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

import {
  base58btcEncode,
  base64urlDecode,
  base64urlEncode,
  publicKeyToDid,
  spaceResource,
  type Capabilities,
  type EncodedUcan,
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

// ---------------------------------------------------------------------------
// UCAN construction (deterministic: caller supplies iat/exp/nonce explicitly).
//
// We build the payload manually rather than call `createUcan` because that
// function pulls iat/nnc from `Date.now()` and `crypto.getRandomValues()`.
// Everything else (header, encoding, signing input) mirrors the library.
// ---------------------------------------------------------------------------
const HEADER = { alg: 'EdDSA' as const, typ: 'JWT' as const }

type SpaceCap = 'space/admin' | 'space/invite' | 'space/write' | 'space/read'

interface TokenSpec {
  key: Key
  audience: string
  spaceId: string
  cap: SpaceCap
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
    cap: { [spaceResource(spec.spaceId)]: spec.cap } satisfies Capabilities,
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
  cap: Record<string, string>
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
  cap: string
  space_id: string
  exp: number
  proofs: EncodedUcan[]
  signed_token: EncodedUcan
}

type ExpectedError =
  | 'ChainTooDeep'
  | 'Signature'
  | 'CapabilityEscalation'
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
  capability_needed: string
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
      cap: spec.cap,
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
    cap: 'space/admin',
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
    capability_needed: 'space/admin',
    chain,
    expected: { ok: true, resolved_root_did: KEYS.root!.did },
  }
}

function vTwoHopValid(): Vector {
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    cap: 'space/admin',
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
    cap: 'space/write',
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
    capability_needed: 'space/write',
    chain,
    expected: { ok: true, resolved_root_did: KEYS.root!.did },
  }
}

function vThreeHopValid(): Vector {
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    cap: 'space/admin',
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
    cap: 'space/admin',
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
    cap: 'space/write',
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
    capability_needed: 'space/write',
    chain,
    expected: { ok: true, resolved_root_did: KEYS.root!.did },
  }
}

function vFiveHopValidAtMax(): Vector {
  // Chain shape: root(root→root, admin) → (root→admin1, admin) → (admin1→admin2, admin) →
  //              (admin2→admin3, admin) → (admin3→member, write leaf).
  // Manually construct so audiences line up correctly (root=self-signed, rest chain forward).
  const specs: TokenSpec[] = [
    {
      key: KEYS.root!,
      audience: KEYS.root!.did,
      spaceId: primarySpaceId,
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h0'),
      prf: [],
    },
    {
      key: KEYS.root!,
      audience: KEYS.admin1!.did,
      spaceId: primarySpaceId,
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h1'),
      prf: [],
    },
    {
      key: KEYS.admin1!,
      audience: KEYS.admin2!.did,
      spaceId: primarySpaceId,
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h2'),
      prf: [],
    },
    {
      key: KEYS.admin2!,
      audience: KEYS.admin3!.did,
      spaceId: primarySpaceId,
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('five_hop.h3'),
      prf: [],
    },
    {
      key: KEYS.admin3!,
      audience: KEYS.member!.did,
      spaceId: primarySpaceId,
      cap: 'space/write',
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
    capability_needed: 'space/write',
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
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h0'),
      prf: [],
    },
    {
      key: KEYS.root!,
      audience: KEYS.admin1!.did,
      spaceId: primarySpaceId,
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h1'),
      prf: [],
    },
    {
      key: KEYS.admin1!,
      audience: KEYS.admin2!.did,
      spaceId: primarySpaceId,
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h2'),
      prf: [],
    },
    {
      key: KEYS.admin2!,
      audience: KEYS.admin3!.did,
      spaceId: primarySpaceId,
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h3'),
      prf: [],
    },
    {
      key: KEYS.admin3!,
      audience: KEYS.wrongIssuer!.did,
      spaceId: primarySpaceId,
      cap: 'space/admin',
      exp: FAR_FUTURE_EXP,
      iat: IAT_ALL,
      nnc: ucanNnc('six_hop.h4'),
      prf: [],
    },
    {
      key: KEYS.wrongIssuer!,
      audience: KEYS.member!.did,
      spaceId: primarySpaceId,
      cap: 'space/write',
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
    capability_needed: 'space/write',
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
  // Also patch the parent's outgoing proofs list to reference the tampered leaf?
  // No — proofs point PARENT→CHILD only via child.prf = [parent], so the leaf's
  // prf still references the intact parent. That's what we want: only the
  // leaf sig is broken.
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
  // Build a valid three-hop chain, flip a byte in the middle node's sig.
  const base = vThreeHopValid()
  const midIdx = 1
  const tampered = flipSignatureByte(base.chain[midIdx]!.signed_token, 5)
  const chain = base.chain.map((n, i) => {
    if (i === midIdx) return { ...n, signed_token: tampered.token }
    // Leaf's `proofs` array still points at the ORIGINAL middle token,
    // because leaf.prf is baked into its signed payload. The Rust
    // verifier reads chain[i].signed_token; the leaf's own prf field
    // (embedded in its payload) contains the original middle. That's OK —
    // for a signature-tamper test, the walker never gets past the middle.
    return n
  })
  return {
    name: 'tampered_middle_signature',
    space_id: base.space_id,
    nonce_hex: base.nonce_hex,
    root_did: base.root_did,
    expected_audience: base.expected_audience,
    capability_needed: base.capability_needed,
    chain,
    expected: { ok: false, error: 'Signature', tampered_signature_byte_offset: tampered.offset },
  }
}

function vCapabilityEscalation(): Vector {
  // Chain: root(admin, self-signed) → mid(root→admin1, READ) → leaf(admin1→member, ADMIN).
  // The leaf claims admin but its parent only grants read. The verifier
  // must reject because attenuation is broken (not the signature).
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    cap: 'space/admin',
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('escalation.root'),
    prf: [],
  }
  const rootToken = makeToken(root)
  const mid: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.admin1!.did,
    spaceId: primarySpaceId,
    cap: 'space/read',
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('escalation.mid'),
    prf: [rootToken],
  }
  const midToken = makeToken(mid)
  const leaf: TokenSpec = {
    key: KEYS.admin1!,
    audience: KEYS.member!.did,
    spaceId: primarySpaceId,
    cap: 'space/admin',
    exp: FAR_FUTURE_EXP,
    iat: IAT_ALL,
    nnc: ucanNnc('escalation.leaf'),
    prf: [midToken],
  }
  const chain = encodeChain([{ spec: root }, { spec: mid }, { spec: leaf }])
  return {
    name: 'capability_escalation_read_to_admin',
    space_id: primarySpaceId,
    nonce_hex: NONCE_HEX.primary_space!,
    root_did: KEYS.root!.did,
    expected_audience: KEYS.member!.did,
    capability_needed: 'space/admin',
    chain,
    expected: { ok: false, error: 'CapabilityEscalation' },
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
    cap: 'space/admin',
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
    cap: 'space/write',
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
    capability_needed: 'space/write',
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
    cap: 'space/admin',
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
    cap: 'space/write',
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
    capability_needed: 'space/write',
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
    cap: 'space/admin',
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
    cap: 'space/admin',
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
    cap: 'space/write',
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
    capability_needed: 'space/write',
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
    cap: 'space/admin',
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
    cap: 'space/write',
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
    capability_needed: 'space/write',
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
    cap: 'space/admin',
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
    cap: 'space/write',
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
    capability_needed: 'space/write',
    chain,
    expected: { ok: false, error: 'Expired' },
  }
}

function vWrongSpaceInDelegate(): Vector {
  // root(space=X, admin) → mid(space=Y, admin) → leaf(space=X, write).
  // Mid delegates on the wrong space so the leaf's claim on X is not
  // covered by any parent proof.
  const root: TokenSpec = {
    key: KEYS.root!,
    audience: KEYS.root!.did,
    spaceId: primarySpaceId,
    cap: 'space/admin',
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
    cap: 'space/admin',
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
    cap: 'space/write',
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
    capability_needed: 'space/write',
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

const CAP_LEVEL: Record<SpaceCap, number> = {
  'space/read': 1,
  'space/write': 2,
  'space/invite': 3,
  'space/admin': 4,
}

function fail(msg: string): never {
  console.error(`Self-verification FAILED: ${msg}`)
  process.exit(1)
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
      const decodedCap = p.cap[resourceKey]
      if (outer.cap !== decodedCap) {
        fail(`${v.name}: chain[${i}] outer.cap ${outer.cap} != decoded.cap[${resourceKey}] ${decodedCap}`)
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
      // Root must be self-signed.
      if (decoded[0]!.iss !== decoded[0]!.aud) {
        fail(`${v.name}: root not self-signed but vector is ok=true`)
      }
      // space_id binding on root.
      const nonce = new Uint8Array(Buffer.from(v.nonce_hex, 'hex'))
      if (deriveSpaceId(decoded[0]!.iss, nonce) !== v.space_id) {
        fail(`${v.name}: space_id binding mismatch but vector is ok=true`)
      }
      // Capability attenuation across the chain.
      for (let i = 1; i < v.chain.length; i++) {
        const parentCapStr = decoded[i - 1]!.cap[spaceResource(v.space_id)]
        const childCapStr = decoded[i]!.cap[spaceResource(v.space_id)]
        if (!parentCapStr || !childCapStr) {
          fail(`${v.name}: missing capability entry at i=${i}`)
        }
        const parentLvl = CAP_LEVEL[parentCapStr as SpaceCap]
        const childLvl = CAP_LEVEL[childCapStr as SpaceCap]
        if (parentLvl < childLvl) {
          fail(`${v.name}: parent cap ${parentCapStr} < child cap ${childCapStr}, but vector is ok=true`)
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
      } else if (err === 'CapabilityEscalation') {
        // Some child cap > parent cap.
        let found = false
        for (let i = 1; i < v.chain.length; i++) {
          const parent = decoded[i - 1]!.cap[spaceResource(v.space_id)]
          const child = decoded[i]!.cap[spaceResource(v.space_id)]
          if (parent && child && CAP_LEVEL[parent as SpaceCap] < CAP_LEVEL[child as SpaceCap]) {
            found = true
            break
          }
        }
        if (!found) fail(`${v.name}: expected escalation but no child cap exceeds parent`)
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
  const vectors: Vector[] = [
    vRootOnlyValid(),
    vTwoHopValid(),
    vThreeHopValid(),
    vFiveHopValidAtMax(),
    vSixHopExceedsMax(),
    vTamperedLeafSignature(),
    vTamperedMiddleSignature(),
    vCapabilityEscalation(),
    vWrongRootDidBindingMismatch(),
    vRootNotSelfSigned(),
    vChainBrokenAudMismatch(),
    vExpiredLeaf(),
    vExpiredRoot(),
    vWrongSpaceInDelegate(),
  ]

  if (vectors.length !== 14) {
    fail(`expected 14 vectors, got ${vectors.length}`)
  }

  selfVerify(vectors)

  const out = {
    _readme: [
      'Cross-language UCAN chain verification fixture. Consumed by both the',
      'TS UCAN library (`@haex-space/ucan`) and the Rust verifier',
      '(`walk_prf_chain`, Task 3).',
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
