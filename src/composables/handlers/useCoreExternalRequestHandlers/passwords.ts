import { and, eq, isNotNull, like, or } from 'drizzle-orm'
import { TOTP } from 'otpauth'
import {
  haexPasswordsGeneratorPresets,
  haexPasswordsGroupItems,
  haexPasswordsItemDetails,
  haexPasswordsItemKeyValues,
  haexPasswordsItemSnapshots,
} from '~/database/schemas/passwords'
import { requireDb } from '~/stores/vault'
import { usePasswordsStore } from '~/stores/passwords'
import { addBinaryAsync } from '~/utils/passwords/binaries'
import { describeUrlForMatching, errorResponse } from './shared'
import type {
  CreateItemPayload,
  ExternalCoreRequest,
  ExternalCoreResponse,
  GetItemsPayload,
  GetTotpPayload,
  ItemEntry,
  OtpAlgorithm,
  UpdateItemPayload,
} from './types'

// ---------------------------------------------------------------------------
// get-items
// ---------------------------------------------------------------------------

export const handleGetItemsAsync = async (
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

export const handleGetTotpAsync = async (
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

export const handleCreateItemAsync = async (
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

export const handleUpdateItemAsync = async (
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

export const handleGetPasswordConfigAsync = async (
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

export const handleGetPasswordPresetsAsync = async (
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
