import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { createOnceListener } from '@/lib/once-listener'
import { and, eq, isNotNull, like, or } from 'drizzle-orm'
import { TOTP } from 'otpauth'
import { parse as parseTld } from 'tldts'
import {
  arrayBufferToBase64,
  base64ToArrayBuffer,
  COSE_ALGORITHM,
  exportKeyPairAsync,
  generatePasskeyPairAsync,
  importPrivateKeyAsync,
  signWithPasskeyAsync,
} from '@haex-space/vault-sdk'
import {
  haexPasswordsGeneratorPresets,
  haexPasswordsGroupItems,
  haexPasswordsItemDetails,
  haexPasswordsItemKeyValues,
  haexPasswordsItemSnapshots,
  haexPasswordsPasskeys,
} from '~/database/schemas/passwords'
import { requireDb } from '~/stores/vault'
import { usePasswordsStore } from '~/stores/passwords'
import { addBinaryAsync } from '~/utils/passwords/binaries'

const CORE_REQUEST_EVENT = 'haextension:external:core-request'

export const CORE_METHODS = {
  GET_ITEMS: 'get-items',
  GET_TOTP: 'get-totp',
  CREATE_ITEM: 'create-item',
  UPDATE_ITEM: 'update-item',
  GET_PASSWORD_CONFIG: 'get-password-config',
  GET_PASSWORD_PRESETS: 'get-password-presets',
  PASSKEY_CREATE: 'passkey-create',
  PASSKEY_GET: 'passkey-get',
  PASSKEY_LIST: 'passkey-list',
} as const

interface ExternalCoreRequest {
  requestId: string
  publicKey: string
  action: string
  payload: Record<string, unknown>
  extensionPublicKey: string
  extensionName: string
}

interface ExternalCoreResponse {
  requestId: string
  success: boolean
  data?: unknown
  error?: string
}

interface GetItemsPayload {
  url?: string
  fields?: string[]
}

interface ItemEntry {
  id: string
  title: string
  url: string | null
  fields: Record<string, string>
  hasTotp: boolean
  autofillAliases?: Record<string, string[]> | null
}

interface GetTotpPayload {
  entryId?: string
}

type OtpAlgorithm = 'SHA1' | 'SHA256' | 'SHA512'

interface CreateItemPayload {
  url?: string
  title?: string
  username?: string
  password?: string
  groupId?: string | null
  otpSecret?: string | null
  otpDigits?: number | null
  otpPeriod?: number | null
  otpAlgorithm?: string | null
  iconBase64?: string | null
}

interface UpdateItemPayload {
  id: string
  url?: string
  title?: string
  username?: string
  password?: string
  otpSecret?: string | null
  otpDigits?: number | null
  otpPeriod?: number | null
  otpAlgorithm?: string | null
  iconBase64?: string | null
}

interface PasskeyCreatePayload {
  relyingPartyId: string
  relyingPartyName: string
  userHandle: string
  userName: string
  userDisplayName?: string
  challenge: string
  excludeCredentials?: string[]
  requireResidentKey?: boolean
  userVerification?: 'required' | 'preferred' | 'discouraged'
  itemId?: string
}

interface PasskeyGetPayload {
  relyingPartyId: string
  challenge: string
  allowCredentials?: Array<{
    id: string
    type: 'public-key'
    transports?: string[]
  }>
  userVerification?: 'required' | 'preferred' | 'discouraged'
}

interface PasskeyListPayload {
  relyingPartyId?: string
  itemId?: string
  discoverableOnly?: boolean
}

const respondAsync = async (response: ExternalCoreResponse): Promise<void> => {
  await invoke('external_bridge_respond', {
    requestId: response.requestId,
    success: response.success,
    data: response.data ?? null,
    error: response.error ?? null,
  })
}

const errorResponse = (requestId: string, message: string): ExternalCoreResponse => ({
  requestId,
  success: false,
  error: message,
})

const toErrorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : 'Unknown error'

/**
 * Reduce a URL (or bare host) into `{ hostname, registrableDomain }`.
 *
 * Used by the get-items URL matcher so an entry stored as `example.de`
 * matches when the browser is on `www.example.de` or `app.example.de`.
 * `registrableDomain` is the eTLD+1 from the Public Suffix List
 * (`example.de`, `example.co.uk`, …) and is `null` for IPs, `localhost`,
 * or intranet names — callers then fall back to hostname equality.
 *
 * `allowPrivateDomains: true` keeps multi-tenant private suffixes
 * (`*.github.io`, `*.herokuapp.com`, …) distinct so credentials for one
 * tenant don't cross-match another sharing the same private suffix.
 */
const describeUrlForMatching = (
  input: string,
): { hostname: string | null; registrableDomain: string | null } => {
  const tryConstruct = (raw: string): URL | null => {
    try {
      return new URL(raw)
    } catch {
      return null
    }
  }
  // Stored entries are often just "example.de" without a scheme — URL needs one.
  const parsed = tryConstruct(input) ?? tryConstruct(`https://${input}`)
  if (!parsed) return { hostname: null, registrableDomain: null }
  const hostname = parsed.hostname.toLowerCase()
  if (!hostname) return { hostname: null, registrableDomain: null }
  const { domain } = parseTld(hostname, { allowPrivateDomains: true })
  return { hostname, registrableDomain: domain ?? null }
}

// ---------------------------------------------------------------------------
// get-items
// ---------------------------------------------------------------------------

const handleGetItemsAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const { url, fields } = request.payload as GetItemsPayload

  if (!url) return errorResponse(request.requestId, 'Missing required field: url')

  const db = requireDb()

  // Reduce the URL/host to its registrable domain (eTLD+1) via the Public
  // Suffix List. www.example.de and app.example.de both collapse to
  // example.de — so an entry stored as example.de matches when the browser
  // is on www.example.de, and sibling subdomains share the match.
  const target = describeUrlForMatching(url)
  if (!target.hostname) {
    return errorResponse(request.requestId, `Could not parse URL: ${url}`)
  }

  // SQL pre-filter casts a wide net using the registrable domain (or the
  // raw hostname when PSL can't classify it — IP addresses, localhost,
  // intranet hosts). The JS step below tightens the match.
  const filterToken = target.registrableDomain ?? target.hostname

  const candidates = await db
    .select({
      id: haexPasswordsItemDetails.id,
      title: haexPasswordsItemDetails.title,
      username: haexPasswordsItemDetails.username,
      password: haexPasswordsItemDetails.password,
      url: haexPasswordsItemDetails.url,
      otpSecret: haexPasswordsItemDetails.otpSecret,
      autofillAliases: haexPasswordsItemDetails.autofillAliases,
    })
    .from(haexPasswordsItemDetails)
    .where(
      and(
        isNotNull(haexPasswordsItemDetails.url),
        or(
          like(haexPasswordsItemDetails.url, `%${filterToken}%`),
          eq(haexPasswordsItemDetails.url, url),
        ),
      ),
    )

  // Keep entries whose URL has the same registrable domain as the target.
  // The substring filter above also hits false positives like
  // `bad-example.de` for `example.de` — this filter discards them.
  const items = candidates.filter((item) => {
    if (!item.url) return false
    const candidate = describeUrlForMatching(item.url)
    if (!candidate.hostname) return false
    if (target.registrableDomain && candidate.registrableDomain) {
      return target.registrableDomain === candidate.registrableDomain
    }
    return target.hostname === candidate.hostname
  })

  const entries: ItemEntry[] = await Promise.all(
    items.map(async (item) => {
      const keyValues = await db
        .select({
          key: haexPasswordsItemKeyValues.key,
          value: haexPasswordsItemKeyValues.value,
        })
        .from(haexPasswordsItemKeyValues)
        .where(eq(haexPasswordsItemKeyValues.itemId, item.id))

      const entryFields: Record<string, string> = {}
      if (item.username) entryFields.username = item.username
      if (item.password) entryFields.password = item.password
      if (item.otpSecret) entryFields.otp = 'TOTP'
      for (const kv of keyValues) {
        if (kv.key && kv.value) entryFields[kv.key] = kv.value
      }

      return {
        id: item.id,
        title: item.title || 'Untitled',
        url: item.url,
        fields: entryFields,
        hasTotp: !!item.otpSecret,
        autofillAliases: item.autofillAliases,
      }
    }),
  )

  const filtered = fields && fields.length > 0
    ? entries.filter((entry) => fields.some((f) => f in entry.fields))
    : entries

  return {
    requestId: request.requestId,
    success: true,
    data: { entries: filtered },
  }
}

// ---------------------------------------------------------------------------
// get-totp
// ---------------------------------------------------------------------------

const handleGetTotpAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const { entryId } = request.payload as GetTotpPayload

  if (!entryId) return errorResponse(request.requestId, 'Missing required field: entryId')

  const db = requireDb()
  const [entry] = await db
    .select({
      otpSecret: haexPasswordsItemDetails.otpSecret,
      otpDigits: haexPasswordsItemDetails.otpDigits,
      otpPeriod: haexPasswordsItemDetails.otpPeriod,
      otpAlgorithm: haexPasswordsItemDetails.otpAlgorithm,
    })
    .from(haexPasswordsItemDetails)
    .where(eq(haexPasswordsItemDetails.id, entryId))
    .limit(1)

  if (!entry || !entry.otpSecret) {
    return errorResponse(request.requestId, 'Entry not found or no TOTP configured')
  }

  const digits = entry.otpDigits ?? 6
  const period = entry.otpPeriod ?? 30
  const algorithm = (entry.otpAlgorithm ?? 'SHA1') as OtpAlgorithm

  const totp = new TOTP({
    secret: entry.otpSecret.trim(),
    digits,
    period,
    algorithm,
  })

  return {
    requestId: request.requestId,
    success: true,
    data: {
      code: totp.generate(),
      validFor: period - (Math.floor(Date.now() / 1000) % period),
    },
  }
}

// ---------------------------------------------------------------------------
// create-item
// ---------------------------------------------------------------------------

const handleCreateItemAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const payload = request.payload as CreateItemPayload
  const { url, title, username, password, groupId, otpSecret, otpDigits, otpPeriod, otpAlgorithm, iconBase64 } = payload

  if (!url && !title) {
    return errorResponse(request.requestId, 'Missing required field: url or title')
  }

  const db = requireDb()

  let entryTitle = title
  if (!entryTitle && url) {
    try {
      entryTitle = new URL(url).hostname
    } catch {
      entryTitle = url
    }
  }

  let iconRef: string | null = null
  if (iconBase64) {
    try {
      const size = atob(iconBase64).length
      const hash = await addBinaryAsync(iconBase64, size, 'icon')
      iconRef = `binary:${hash}`
    } catch (error) {
      console.error('[core] create-item icon failed:', error)
    }
  }

  const itemId = crypto.randomUUID()

  await db.insert(haexPasswordsItemDetails).values({
    id: itemId,
    title: entryTitle || null,
    username: username || null,
    password: password || null,
    url: url || null,
    note: null,
    otpSecret: otpSecret || null,
    otpDigits: otpDigits ?? null,
    otpPeriod: otpPeriod ?? null,
    otpAlgorithm: otpAlgorithm || null,
    icon: iconRef,
    color: null,
  })

  await db.insert(haexPasswordsGroupItems).values({
    itemId,
    groupId: groupId || null,
  })

  const snapshotData = {
    title: entryTitle,
    username: username || null,
    password: password || null,
    url: url || null,
    note: null,
    tags: null,
    otpSecret: otpSecret || null,
    otpDigits: otpDigits ?? null,
    otpPeriod: otpPeriod ?? null,
    otpAlgorithm: otpAlgorithm || null,
    icon: iconRef,
    keyValues: [],
    attachments: [],
  }

  await db.insert(haexPasswordsItemSnapshots).values({
    id: crypto.randomUUID(),
    itemId,
    snapshotData: JSON.stringify(snapshotData),
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
  })

  await usePasswordsStore().loadItemsAsync()

  return {
    requestId: request.requestId,
    success: true,
    data: { entryId: itemId, title: entryTitle || '' },
  }
}

// ---------------------------------------------------------------------------
// update-item
// ---------------------------------------------------------------------------

const handleUpdateItemAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const payload = request.payload as unknown as UpdateItemPayload
  const { id, url, title, username, password, otpSecret, otpDigits, otpPeriod, otpAlgorithm, iconBase64 } = payload

  if (!id) return errorResponse(request.requestId, 'Missing required field: id')

  const db = requireDb()
  const [existing] = await db
    .select()
    .from(haexPasswordsItemDetails)
    .where(eq(haexPasswordsItemDetails.id, id))
    .limit(1)

  if (!existing) return errorResponse(request.requestId, 'Entry not found')

  const updateFields: Record<string, unknown> = {}
  if (title !== undefined) updateFields.title = title || null
  if (username !== undefined) updateFields.username = username || null
  if (password !== undefined) updateFields.password = password || null
  if (url !== undefined) updateFields.url = url || null
  if (otpSecret !== undefined) updateFields.otpSecret = otpSecret || null
  if (otpDigits !== undefined) updateFields.otpDigits = otpDigits ?? null
  if (otpPeriod !== undefined) updateFields.otpPeriod = otpPeriod ?? null
  if (otpAlgorithm !== undefined) updateFields.otpAlgorithm = otpAlgorithm || null

  if (iconBase64 !== undefined) {
    if (iconBase64) {
      try {
        const size = atob(iconBase64).length
        const hash = await addBinaryAsync(iconBase64, size, 'icon')
        updateFields.icon = `binary:${hash}`
      } catch (error) {
        console.error('[core] update-item icon failed:', error)
      }
    } else {
      updateFields.icon = null
    }
  }

  await db
    .update(haexPasswordsItemDetails)
    .set(updateFields)
    .where(eq(haexPasswordsItemDetails.id, id))

  const snapshotData = {
    title: title ?? existing.title,
    username: username ?? existing.username,
    password: password ?? existing.password,
    url: url ?? existing.url,
    note: existing.note,
    tags: null,
    otpSecret: otpSecret ?? existing.otpSecret,
    otpDigits: otpDigits ?? existing.otpDigits,
    otpPeriod: otpPeriod ?? existing.otpPeriod,
    otpAlgorithm: otpAlgorithm ?? existing.otpAlgorithm,
    icon: updateFields.icon !== undefined ? updateFields.icon : existing.icon,
    keyValues: [],
    attachments: [],
  }

  await db.insert(haexPasswordsItemSnapshots).values({
    id: crypto.randomUUID(),
    itemId: id,
    snapshotData: JSON.stringify(snapshotData),
    createdAt: new Date().toISOString(),
    modifiedAt: new Date().toISOString(),
  })

  await usePasswordsStore().loadItemsAsync()

  return {
    requestId: request.requestId,
    success: true,
    data: { entryId: id },
  }
}

// ---------------------------------------------------------------------------
// get-password-config + get-password-presets
// ---------------------------------------------------------------------------

const handleGetPasswordConfigAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const db = requireDb()
  const [defaultPreset] = await db
    .select()
    .from(haexPasswordsGeneratorPresets)
    .where(eq(haexPasswordsGeneratorPresets.isDefault, true))
    .limit(1)

  if (!defaultPreset) {
    return {
      requestId: request.requestId,
      success: true,
      data: { config: null, presetName: null },
    }
  }

  return {
    requestId: request.requestId,
    success: true,
    data: {
      config: {
        length: defaultPreset.length,
        uppercase: defaultPreset.uppercase,
        lowercase: defaultPreset.lowercase,
        numbers: defaultPreset.numbers,
        symbols: defaultPreset.symbols,
        excludeChars: defaultPreset.excludeChars || null,
        usePattern: defaultPreset.usePattern,
        pattern: defaultPreset.pattern || null,
      },
      presetName: defaultPreset.name,
    },
  }
}

const handleGetPasswordPresetsAsync = async (
  request: ExternalCoreRequest,
): Promise<ExternalCoreResponse> => {
  const db = requireDb()
  const presets = await db.select().from(haexPasswordsGeneratorPresets)

  return {
    requestId: request.requestId,
    success: true,
    data: {
      presets: presets.map((preset) => ({
        id: preset.id,
        name: preset.name,
        isDefault: preset.isDefault,
        config: {
          length: preset.length,
          uppercase: preset.uppercase,
          lowercase: preset.lowercase,
          numbers: preset.numbers,
          symbols: preset.symbols,
          excludeChars: preset.excludeChars || null,
          usePattern: preset.usePattern,
          pattern: preset.pattern || null,
        },
      })),
    },
  }
}

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

const handlePasskeyCreateAsync = async (
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

const handlePasskeyGetAsync = async (
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

const handlePasskeyListAsync = async (
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

// ---------------------------------------------------------------------------
// Dispatch + composable
// ---------------------------------------------------------------------------

const dispatchAsync = async (request: ExternalCoreRequest): Promise<ExternalCoreResponse> => {
  try {
    switch (request.action) {
      case CORE_METHODS.GET_ITEMS:
        return await handleGetItemsAsync(request)
      case CORE_METHODS.GET_TOTP:
        return await handleGetTotpAsync(request)
      case CORE_METHODS.CREATE_ITEM:
        return await handleCreateItemAsync(request)
      case CORE_METHODS.UPDATE_ITEM:
        return await handleUpdateItemAsync(request)
      case CORE_METHODS.GET_PASSWORD_CONFIG:
        return await handleGetPasswordConfigAsync(request)
      case CORE_METHODS.GET_PASSWORD_PRESETS:
        return await handleGetPasswordPresetsAsync(request)
      case CORE_METHODS.PASSKEY_CREATE:
        return await handlePasskeyCreateAsync(request)
      case CORE_METHODS.PASSKEY_GET:
        return await handlePasskeyGetAsync(request)
      case CORE_METHODS.PASSKEY_LIST:
        return await handlePasskeyListAsync(request)
      default:
        return errorResponse(request.requestId, `Unknown core action: ${request.action}`)
    }
  } catch (error) {
    console.error(`[core] handler failed for ${request.action}:`, error)
    return errorResponse(request.requestId, toErrorMessage(error))
  }
}

export const useCoreExternalRequestHandlers = () => {
  const listener = createOnceListener(() =>
    listen<ExternalCoreRequest>(CORE_REQUEST_EVENT, async (event) => {
      const response = await dispatchAsync(event.payload)
      await respondAsync(response).catch((err) => {
        console.error('[core] failed to send response:', err)
      })
    }),
  )

  return { initAsync: listener.initAsync, dispose: listener.dispose }
}
