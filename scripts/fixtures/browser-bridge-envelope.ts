// `node scripts/fixtures/browser-bridge-envelope.ts` (spec 049, research R18): writes the WebCrypto
// fixture of the bridge envelopes. It seals with the calls of haex-pass-browser's
// `background/crypto.ts` (X25519, the raw shared secret as the AES-256-GCM key, standard Base64), but
// with fixed keys and IVs, so every run writes the same file. The keys are test keys and protect
// nothing.
import { webcrypto } from 'node:crypto'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import { pathToFileURL } from 'node:url'

const { subtle } = webcrypto

export const FIXTURE = 'src-tauri/tests/fixtures/browser_bridge/webcrypto.json'

/** The PKCS#8 header of an X25519 private key (RFC 8410); the 32 key bytes follow. */
const PKCS8_X25519 = [
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x6e, 0x04,
  0x22, 0x04, 0x20,
]

/** Bytes WebCrypto takes: never backed by a `SharedArrayBuffer`. */
type Bytes = Uint8Array<ArrayBuffer>

export interface Sealed {
  publicKey: string
  iv: string
  message: string
}

export interface Fixture {
  note: string
  client: { secret: string; publicKey: string; clientId: string }
  server: { secret: string; publicKey: string }
  request: Sealed & { action: string; plaintext: string }
  response: Sealed & { ephemeralSecret: string; plaintext: string }
}

const b64 = (bytes: ArrayBuffer | Uint8Array) =>
  Buffer.from(
    bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes),
  ).toString('base64')
const unb64 = (text: string): Bytes =>
  new Uint8Array(Buffer.from(text, 'base64'))

/** `length` bytes counting up from `start`: fixed, and different for every key. */
const counting = (start: number, length: number): Bytes =>
  Uint8Array.from({ length }, (_, i) => (start + i) & 0xff)

function privateKey(secret: Bytes) {
  return subtle.importKey(
    'pkcs8',
    Uint8Array.from([...PKCS8_X25519, ...secret]),
    { name: 'X25519' },
    true,
    ['deriveBits'],
  )
}

/** The raw public key of a secret, read from the private key's JWK. */
async function publicOf(secret: Bytes): Promise<Bytes> {
  const jwk = await subtle.exportKey('jwk', await privateKey(secret))
  return new Uint8Array(Buffer.from(jwk.x ?? '', 'base64url'))
}

/** `deriveSharedKey` of `background/crypto.ts`. */
async function sharedKey(secret: Bytes, peer: Bytes) {
  const bits = await subtle.deriveBits(
    {
      name: 'X25519',
      public: await subtle.importKey('raw', peer, { name: 'X25519' }, true, []),
    },
    await privateKey(secret),
    256,
  )
  return subtle.importKey(
    'raw',
    bits,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt'],
  )
}

async function seal(
  ephemeral: Bytes,
  iv: Bytes,
  recipient: Bytes,
  plaintext: string,
): Promise<Sealed> {
  const key = await sharedKey(ephemeral, recipient)
  const message = await subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    new TextEncoder().encode(plaintext),
  )
  return {
    publicKey: b64(await publicOf(ephemeral)),
    iv: b64(iv),
    message: b64(message),
  }
}

/** Opens an envelope with the recipient's secret, as `decryptMessageEnvelope` does. */
export async function open(secret: string, sealed: Sealed): Promise<string> {
  const key = await sharedKey(unb64(secret), unb64(sealed.publicKey))
  const plain = await subtle.decrypt(
    { name: 'AES-GCM', iv: unb64(sealed.iv) },
    key,
    unb64(sealed.message),
  )
  return new TextDecoder().decode(plain)
}

/** `generateClientId` of `background/crypto.ts`. */
async function clientIdOf(publicKey: Bytes): Promise<string> {
  const hash = new Uint8Array(await subtle.digest('SHA-256', publicKey))
  return Buffer.from(hash.slice(0, 16)).toString('hex')
}

export async function build(): Promise<Fixture> {
  const clientSecret = counting(0x01, 32)
  const serverSecret = counting(0x41, 32)
  const responseEphemeral = counting(0xc1, 32)
  const clientPublic = await publicOf(clientSecret)
  const serverPublic = await publicOf(serverSecret)
  const requestPlain = JSON.stringify({
    requestId: 'r-fixture',
    url: 'https://login.example.de',
  })
  const responsePlain = JSON.stringify({
    requestId: 'r-fixture',
    success: true,
    data: { entries: [] },
  })
  return {
    note: 'Test keys only (spec 049, research R18); written by scripts/fixtures/browser-bridge-envelope.ts.',
    client: {
      secret: b64(clientSecret),
      publicKey: b64(clientPublic),
      clientId: await clientIdOf(clientPublic),
    },
    server: { secret: b64(serverSecret), publicKey: b64(serverPublic) },
    request: {
      action: 'get-items',
      plaintext: requestPlain,
      ...(await seal(
        counting(0x81, 32),
        counting(0xa1, 12),
        serverPublic,
        requestPlain,
      )),
    },
    response: {
      ephemeralSecret: b64(responseEphemeral),
      plaintext: responsePlain,
      ...(await seal(
        responseEphemeral,
        counting(0xe1, 12),
        clientPublic,
        responsePlain,
      )),
    },
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  mkdirSync(dirname(FIXTURE), { recursive: true })
  writeFileSync(FIXTURE, `${JSON.stringify(await build(), null, 2)}\n`)
  console.log(`wrote ${FIXTURE}`)
}
