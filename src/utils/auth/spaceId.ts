/**
 * Self-certifying `space_id` — binds the id to the DID of the Space-Root
 * (issuer of the `space/admin` root UCAN). See ADR 0002 §6.1.
 *
 * Layout: `nonce (16 B) ‖ sha256_16(domain_tag ‖ nonce ‖ root_did_utf8)`.
 * The 32-byte binary is base58btc-encoded (Bitcoin alphabet, ~44 chars).
 */

const DOMAIN_TAG = 'haex/space-id/v1'
const DOMAIN_TAG_BYTES = new TextEncoder().encode(DOMAIN_TAG)
const NONCE_LEN = 16
const HASH_LEN = 16
const SPACE_ID_BYTES_LEN = NONCE_LEN + HASH_LEN

const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
const BASE58_MAP = (() => {
  const m = new Int8Array(128).fill(-1)
  for (let i = 0; i < BASE58_ALPHABET.length; i++) m[BASE58_ALPHABET.charCodeAt(i)] = i
  return m
})()

// Non-null assertions in the base58 loops are load-bearing under
// `noUncheckedIndexedAccess`: every index accessed is guaranteed in-bounds by
// the surrounding loop conditions and by the fact that `Uint8Array` slots are
// zero-initialised. Runtime undefined is impossible.
function base58Encode(bytes: Uint8Array): string {
  if (bytes.length === 0) return ''
  let zeros = 0
  while (zeros < bytes.length && bytes[zeros] === 0) zeros++
  const size = Math.ceil(((bytes.length - zeros) * 138) / 100) + 1
  const b58 = new Uint8Array(size)
  let length = 0
  for (let i = zeros; i < bytes.length; i++) {
    let carry: number = bytes[i]!
    let j = 0
    for (let k = size - 1; (carry !== 0 || j < length) && k >= 0; k--, j++) {
      carry += 256 * b58[k]!
      b58[k] = carry % 58
      carry = (carry / 58) | 0
    }
    length = j
  }
  let it = size - length
  while (it < size && b58[it] === 0) it++
  let out = '1'.repeat(zeros)
  for (; it < size; it++) out += BASE58_ALPHABET[b58[it]!]
  return out
}

function base58Decode(str: string): Uint8Array | null {
  if (str.length === 0) return new Uint8Array(0)
  let zeros = 0
  while (zeros < str.length && str[zeros] === '1') zeros++
  const size = Math.ceil((str.length * 733) / 1000) + 1
  const b256 = new Uint8Array(size)
  let length = 0
  for (let i = zeros; i < str.length; i++) {
    const code = str.charCodeAt(i)
    if (code >= 128) return null
    const digit = BASE58_MAP[code]!
    if (digit < 0) return null
    let carry: number = digit
    let j = 0
    for (let k = size - 1; (carry !== 0 || j < length) && k >= 0; k--, j++) {
      carry += 58 * b256[k]!
      b256[k] = carry & 0xff
      carry >>= 8
    }
    length = j
  }
  let it = size - length
  while (it < size && b256[it] === 0) it++
  const out = new Uint8Array(zeros + (size - it))
  out.fill(0, 0, zeros)
  for (let i = zeros; it < size; i++, it++) out[i] = b256[it]!
  return out
}

async function sha256(data: Uint8Array): Promise<Uint8Array> {
  // Cast: `crypto.subtle.digest` accepts `BufferSource`; the DOM lib types
  // reject `Uint8Array<ArrayBufferLike>` because `ArrayBufferLike` widens to
  // include `SharedArrayBuffer`. Our callers only ever construct plain
  // `Uint8Array` over an `ArrayBuffer`, so the cast is safe.
  const buf = await crypto.subtle.digest('SHA-256', data as BufferSource)
  return new Uint8Array(buf)
}

function concat(...parts: Uint8Array[]): Uint8Array {
  const len = parts.reduce((s, p) => s + p.length, 0)
  const out = new Uint8Array(len)
  let off = 0
  for (const p of parts) {
    out.set(p, off)
    off += p.length
  }
  return out
}

async function computeHashPart(nonce: Uint8Array, rootDid: string): Promise<Uint8Array> {
  const didBytes = new TextEncoder().encode(rootDid)
  const preimage = concat(DOMAIN_TAG_BYTES, nonce, didBytes)
  const full = await sha256(preimage)
  return full.slice(0, HASH_LEN)
}

/**
 * Derive a self-certifying space_id from a root DID. If `nonce` is omitted a
 * fresh 16-byte random nonce is generated. The nonce is embedded in the id so
 * the binding can be verified from `(space_id, root_did)` alone.
 */
export async function deriveSpaceIdAsync(rootDid: string, nonce?: Uint8Array): Promise<string> {
  const n = nonce ?? crypto.getRandomValues(new Uint8Array(NONCE_LEN))
  if (n.length !== NONCE_LEN) throw new Error(`nonce must be ${NONCE_LEN} bytes`)
  const hash = await computeHashPart(n, rootDid)
  return base58Encode(concat(n, hash))
}

/**
 * Verify that `spaceId` is the self-certifying binding of `rootDid`. Returns
 * `false` for malformed input (no throws) so callers can treat verification as
 * a pure predicate.
 */
export async function verifySpaceIdBindingAsync(spaceId: string, rootDid: string): Promise<boolean> {
  const bytes = base58Decode(spaceId)
  if (!bytes || bytes.length !== SPACE_ID_BYTES_LEN) return false
  const nonce = bytes.slice(0, NONCE_LEN)
  const claimed = bytes.slice(NONCE_LEN)
  const expected = await computeHashPart(nonce, rootDid)
  let diff = 0
  for (let i = 0; i < HASH_LEN; i++) diff |= claimed[i]! ^ expected[i]!
  return diff === 0
}

export function encodeSpaceIdBytes(bytes: Uint8Array): string {
  if (bytes.length !== SPACE_ID_BYTES_LEN) throw new Error(`bytes must be ${SPACE_ID_BYTES_LEN}`)
  return base58Encode(bytes)
}

export function decodeSpaceIdBytes(spaceId: string): Uint8Array {
  const b = base58Decode(spaceId)
  if (!b || b.length !== SPACE_ID_BYTES_LEN) throw new Error('invalid space_id encoding')
  return b
}
