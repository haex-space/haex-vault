import { and, eq } from 'drizzle-orm'
import {
  arrayBufferToBase64,
  base64ToArrayBuffer,
  COSE_ALGORITHM,
  exportKeyPairAsync,
  generatePasskeyPairAsync,
  importPrivateKeyAsync,
  signWithPasskeyAsync,
} from '@haex-space/vault-sdk'
import { haexPasswordsPasskeys } from '~/database/schemas/passwords'
import { requireDb } from '~/stores/vault'
import { errorResponse } from './shared'
import type {
  ExternalCoreRequest,
  ExternalCoreResponse,
  PasskeyCreatePayload,
  PasskeyGetPayload,
  PasskeyListPayload,
} from './types'

// ---------------------------------------------------------------------------
// WebAuthn helpers (CBOR "none" attestation)
// ---------------------------------------------------------------------------

const buildClientDataJson = (
  type: 'webauthn.create' | 'webauthn.get',
  challenge: string,
  origin: string,
): string => JSON.stringify({ type, challenge, origin, crossOrigin: false })

const buildAuthenticatorDataAsync = async (
  relyingPartyId: string,
  signCount: number,
  attestedCredentialData: boolean,
): Promise<ArrayBuffer> => {
  const rpIdBytes = new TextEncoder().encode(relyingPartyId)
  const rpIdHash = new Uint8Array(await crypto.subtle.digest('SHA-256', rpIdBytes))

  // Flags: UP (0x01) | UV (0x04) | AT (0x40 if attested)
  let flags = 0x01 | 0x04
  if (attestedCredentialData) flags |= 0x40

  const signCountBytes = new Uint8Array(4)
  signCountBytes[0] = (signCount >> 24) & 0xff
  signCountBytes[1] = (signCount >> 16) & 0xff
  signCountBytes[2] = (signCount >> 8) & 0xff
  signCountBytes[3] = signCount & 0xff

  const authData = new Uint8Array(37)
  authData.set(rpIdHash, 0)
  authData[32] = flags
  authData.set(signCountBytes, 33)
  return authData.buffer
}

// Minimal CBOR encoder for { fmt: "none", attStmt: {}, authData: <bytes> }
const buildCborAttestationObject = (authData: Uint8Array): ArrayBuffer => {
  const parts: number[] = []
  parts.push(0xa3) // map(3)
  parts.push(0x63, 0x66, 0x6d, 0x74) // "fmt"
  parts.push(0x64, 0x6e, 0x6f, 0x6e, 0x65) // "none"
  parts.push(0x67, 0x61, 0x74, 0x74, 0x53, 0x74, 0x6d, 0x74) // "attStmt"
  parts.push(0xa0) // empty map
  parts.push(0x68, 0x61, 0x75, 0x74, 0x68, 0x44, 0x61, 0x74, 0x61) // "authData"

  if (authData.length < 24) parts.push(0x40 + authData.length)
  else if (authData.length < 256) parts.push(0x58, authData.length)
  else parts.push(0x59, (authData.length >> 8) & 0xff, authData.length & 0xff)

  for (let i = 0; i < authData.length; i++) parts.push(authData[i]!)
  return new Uint8Array(parts).buffer
}

const buildAttestationObjectAsync = async (
  relyingPartyId: string,
  credentialId: Uint8Array,
  publicKeyCoseBase64: string,
): Promise<ArrayBuffer> => {
  const publicKeyCose = base64ToArrayBuffer(publicKeyCoseBase64)
  const rpIdHash = new Uint8Array(
    await crypto.subtle.digest('SHA-256', new TextEncoder().encode(relyingPartyId)),
  )

  const flags = 0x45 // UP | UV | AT
  const aaguid = new Uint8Array(16)
  const credIdLength = new Uint8Array(2)
  credIdLength[0] = (credentialId.length >> 8) & 0xff
  credIdLength[1] = credentialId.length & 0xff

  const attested = new Uint8Array(16 + 2 + credentialId.length + publicKeyCose.byteLength)
  attested.set(aaguid, 0)
  attested.set(credIdLength, 16)
  attested.set(credentialId, 18)
  attested.set(new Uint8Array(publicKeyCose), 18 + credentialId.length)

  const authData = new Uint8Array(37 + attested.length)
  authData.set(rpIdHash, 0)
  authData[32] = flags
  // signCount stays 0 at offsets 33-36
  authData.set(attested, 37)

  return buildCborAttestationObject(authData)
}

// ---------------------------------------------------------------------------
// passkey-create
// ---------------------------------------------------------------------------

export const handlePasskeyCreateAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const payload = request.payload as unknown as PasskeyCreatePayload

  if (!payload.relyingPartyId || !payload.userHandle || !payload.userName || !payload.challenge) {
    return errorResponse(
      request.requestId,
      'Missing required fields: relyingPartyId, userHandle, userName, challenge',
    )
  }

  const db = requireDb()

  if (payload.excludeCredentials && payload.excludeCredentials.length > 0) {
    for (const excludedId of payload.excludeCredentials) {
      const [existing] = await db
        .select()
        .from(haexPasswordsPasskeys)
        .where(eq(haexPasswordsPasskeys.credentialId, excludedId))
        .limit(1)
      if (existing) return errorResponse(request.requestId, 'Credential already registered')
    }
  }

  const keyPair = await generatePasskeyPairAsync()
  const exportedKeys = await exportKeyPairAsync(keyPair)

  const credentialIdBytes = crypto.getRandomValues(new Uint8Array(32))
  const credentialId = arrayBufferToBase64(credentialIdBytes)

  const passkeyId = crypto.randomUUID()
  await db.insert(haexPasswordsPasskeys).values({
    id: passkeyId,
    itemId: payload.itemId || null,
    credentialId,
    relyingPartyId: payload.relyingPartyId,
    relyingPartyName: payload.relyingPartyName || null,
    userHandle: payload.userHandle,
    userName: payload.userName,
    userDisplayName: payload.userDisplayName || null,
    privateKey: exportedKeys.privateKeyBase64,
    publicKey: exportedKeys.publicKeyBase64,
    algorithm: COSE_ALGORITHM.ES256,
    signCount: 0,
    isDiscoverable: payload.requireResidentKey ?? true,
  })

  const attestationObject = await buildAttestationObjectAsync(
    payload.relyingPartyId,
    credentialIdBytes,
    exportedKeys.publicKeyCoseBase64,
  )

  const clientDataJson = buildClientDataJson(
    'webauthn.create',
    payload.challenge,
    `https://${payload.relyingPartyId}`,
  )

  return {
    requestId: request.requestId,
    success: true,
    data: {
      credentialId,
      publicKey: exportedKeys.publicKeyBase64,
      publicKeyCose: exportedKeys.publicKeyCoseBase64,
      attestationObject: arrayBufferToBase64(attestationObject),
      clientDataJson: arrayBufferToBase64(new TextEncoder().encode(clientDataJson)),
      passkeyId,
      transports: ['internal', 'hybrid'],
    },
  }
}

// ---------------------------------------------------------------------------
// passkey-get
// ---------------------------------------------------------------------------

export const handlePasskeyGetAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const payload = request.payload as unknown as PasskeyGetPayload

  if (!payload.relyingPartyId || !payload.challenge) {
    return errorResponse(
      request.requestId,
      'Missing required fields: relyingPartyId, challenge',
    )
  }

  const db = requireDb()

  let passkey: typeof haexPasswordsPasskeys.$inferSelect | undefined
  if (payload.allowCredentials && payload.allowCredentials.length > 0) {
    for (const allowed of payload.allowCredentials) {
      const [found] = await db
        .select()
        .from(haexPasswordsPasskeys)
        .where(
          and(
            eq(haexPasswordsPasskeys.credentialId, allowed.id),
            eq(haexPasswordsPasskeys.relyingPartyId, payload.relyingPartyId),
          ),
        )
        .limit(1)
      if (found) {
        passkey = found
        break
      }
    }
  } else {
    const [found] = await db
      .select()
      .from(haexPasswordsPasskeys)
      .where(
        and(
          eq(haexPasswordsPasskeys.relyingPartyId, payload.relyingPartyId),
          eq(haexPasswordsPasskeys.isDiscoverable, true),
        ),
      )
      .limit(1)
    passkey = found
  }

  if (!passkey) return errorResponse(request.requestId, 'No matching passkey found')

  const privateKey = await importPrivateKeyAsync(passkey.privateKey)
  const newSignCount = passkey.signCount + 1
  const authenticatorData = await buildAuthenticatorDataAsync(
    payload.relyingPartyId,
    newSignCount,
    false,
  )

  const clientDataJson = buildClientDataJson(
    'webauthn.get',
    payload.challenge,
    `https://${payload.relyingPartyId}`,
  )
  const clientDataJsonBytes = new TextEncoder().encode(clientDataJson)
  const clientDataHash = await crypto.subtle.digest('SHA-256', clientDataJsonBytes)

  const signatureData = new Uint8Array(authenticatorData.byteLength + clientDataHash.byteLength)
  signatureData.set(new Uint8Array(authenticatorData), 0)
  signatureData.set(new Uint8Array(clientDataHash), authenticatorData.byteLength)

  const signature = await signWithPasskeyAsync(privateKey, signatureData)

  await db
    .update(haexPasswordsPasskeys)
    .set({ signCount: newSignCount, lastUsedAt: new Date().toISOString() })
    .where(eq(haexPasswordsPasskeys.id, passkey.id))

  return {
    requestId: request.requestId,
    success: true,
    data: {
      credentialId: passkey.credentialId,
      authenticatorData: arrayBufferToBase64(authenticatorData),
      signature: arrayBufferToBase64(signature),
      clientDataJson: arrayBufferToBase64(clientDataJsonBytes),
      userHandle: passkey.isDiscoverable ? passkey.userHandle : undefined,
      passkeyId: passkey.id,
    },
  }
}

// ---------------------------------------------------------------------------
// passkey-list
// ---------------------------------------------------------------------------

export const handlePasskeyListAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const payload = request.payload as PasskeyListPayload
  const db = requireDb()

  const conditions = []
  if (payload.relyingPartyId)
    conditions.push(eq(haexPasswordsPasskeys.relyingPartyId, payload.relyingPartyId))
  if (payload.itemId) conditions.push(eq(haexPasswordsPasskeys.itemId, payload.itemId))
  if (payload.discoverableOnly) conditions.push(eq(haexPasswordsPasskeys.isDiscoverable, true))

  const passkeys = conditions.length > 0
    ? await db.select().from(haexPasswordsPasskeys).where(and(...conditions))
    : await db.select().from(haexPasswordsPasskeys)

  return {
    requestId: request.requestId,
    success: true,
    data: {
      passkeys: passkeys.map((p) => ({
        id: p.id,
        credentialId: p.credentialId,
        relyingPartyId: p.relyingPartyId,
        relyingPartyName: p.relyingPartyName,
        userName: p.userName,
        userDisplayName: p.userDisplayName,
        nickname: p.nickname,
        createdAt: p.createdAt,
        lastUsedAt: p.lastUsedAt,
        isDiscoverable: p.isDiscoverable,
        itemId: p.itemId,
      })),
    },
  }
}
