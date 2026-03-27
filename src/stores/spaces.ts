import {
  generateSpaceKey,
  encryptWithPublicKeyAsync,
  decryptWithPrivateKeyAsync,
  encryptSpaceNameAsync,
  decryptSpaceNameAsync,
  arrayBufferToBase64,
  base64ToArrayBuffer,
  type SharedSpace,
  SpaceRoles,
  type SpaceRole,
  type SpaceInvite,
  type DecryptedSpace,
} from '@haex-space/vault-sdk'
import { eq, and } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexSpaceKeys, haexSpaces } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { getAuthTokenAsync } from '@/stores/sync/engine/supabase'

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  // In-memory read-through cache for space keys
  const spaceKeyCache = new Map<string, Uint8Array[]>()

  const spaces = ref<DecryptedSpace[]>([])

  // =========================================================================
  // DB Helpers
  // =========================================================================

  const getDb = () => currentVault.value?.drizzle

  /** Load all spaces from DB into memory */
  const loadSpacesFromDbAsync = async () => {
    const db = getDb()
    if (!db) return

    const rows = await db.select().from(haexSpaces)
    spaces.value = rows.map(rowToDecryptedSpace)
  }

  /** Convert a DB row to DecryptedSpace */
  const rowToDecryptedSpace = (row: SelectHaexSpaces): DecryptedSpace => ({
    id: row.id,
    name: row.name,
    role: row.role as SpaceRole,
    serverUrl: row.serverUrl ?? '',
    createdAt: row.createdAt ?? '',
  })

  /** Persist a space to DB and update in-memory list */
  const persistSpaceAsync = async (space: DecryptedSpace) => {
    const db = getDb()
    if (!db) return

    const existing = await db.select().from(haexSpaces).where(eq(haexSpaces.id, space.id)).limit(1)

    if (existing.length > 0) {
      await db.update(haexSpaces).set({
        name: space.name,
        serverUrl: space.serverUrl || null,
        role: space.role,
        modifiedAt: new Date().toISOString(),
      }).where(eq(haexSpaces.id, space.id))
    } else {
      await db.insert(haexSpaces).values({
        id: space.id,
        name: space.name,
        serverUrl: space.serverUrl || null,
        role: space.role,
      })
    }

    // Update in-memory
    const idx = spaces.value.findIndex(s => s.id === space.id)
    if (idx >= 0) {
      spaces.value[idx] = space
    } else {
      spaces.value.push(space)
    }
  }

  /** Remove a space from DB and in-memory list */
  const removeSpaceFromDbAsync = async (spaceId: string) => {
    const db = getDb()
    if (db) {
      await db.delete(haexSpaces).where(eq(haexSpaces.id, spaceId))
    }
    spaces.value = spaces.value.filter(s => s.id !== spaceId)
  }

  // =========================================================================
  // Auth Helper
  // =========================================================================

  const fetchWithAuth = async (url: string, options: RequestInit = {}) => {
    const token = await getAuthTokenAsync()
    if (!token) throw new Error('Not authenticated')
    return fetch(url, {
      ...options,
      headers: {
        ...options.headers,
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
      },
    })
  }

  const fetchWithSpaceToken = async (url: string, spaceToken: string, options: RequestInit = {}) => {
    return fetch(url, {
      ...options,
      headers: {
        ...options.headers,
        'X-Space-Token': spaceToken,
        'Content-Type': 'application/json',
      },
    })
  }

  // =========================================================================
  // Identity Helper
  // =========================================================================

  const resolveIdentityAsync = async (identityId: string) => {
    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityAsync(identityId)
    if (!identity) throw new Error(`Identity ${identityId} not found`)
    return identity
  }

  // =========================================================================
  // Space Key Management (DB-backed with in-memory cache)
  // Now supports multiple keys per (spaceId, generation)
  // =========================================================================

  const cacheKey = (spaceId: string, generation: number) => `${spaceId}:${generation}`

  const getSpaceKey = (spaceId: string, generation: number): Uint8Array | undefined => {
    const keys = spaceKeyCache.get(cacheKey(spaceId, generation))
    return keys?.[0]
  }

  const getSpaceKeysAsync = async (spaceId: string, generation: number): Promise<Uint8Array[]> => {
    const ck = cacheKey(spaceId, generation)
    const cached = spaceKeyCache.get(ck)
    if (cached) return cached

    const db = getDb()
    if (!db) return []

    const rows = await db
      .select()
      .from(haexSpaceKeys)
      .where(and(
        eq(haexSpaceKeys.spaceId, spaceId),
        eq(haexSpaceKeys.generation, generation),
      ))

    if (rows.length === 0) return []

    const keys = rows.map(r => new Uint8Array(base64ToArrayBuffer(r.key)))
    spaceKeyCache.set(ck, keys)
    return keys
  }

  const getSpaceKeyAsync = async (spaceId: string, generation: number): Promise<Uint8Array | undefined> => {
    const keys = await getSpaceKeysAsync(spaceId, generation)
    return keys[0]
  }

  const persistSpaceKeyAsync = async (spaceId: string, generation: number, key: Uint8Array) => {
    const ck = cacheKey(spaceId, generation)
    const existing = spaceKeyCache.get(ck) ?? []
    const keyBase64 = arrayBufferToBase64(key.buffer as ArrayBuffer)

    // Check if this exact key already exists
    if (!existing.some(k => arrayBufferToBase64(k.buffer as ArrayBuffer) === keyBase64)) {
      spaceKeyCache.set(ck, [...existing, key])
    }

    const db = getDb()
    if (!db) return

    // Check if this exact key already exists in DB
    const dbRows = await db
      .select()
      .from(haexSpaceKeys)
      .where(and(
        eq(haexSpaceKeys.spaceId, spaceId),
        eq(haexSpaceKeys.generation, generation),
        eq(haexSpaceKeys.key, keyBase64),
      ))
      .limit(1)

    if (dbRows.length === 0) {
      await db.insert(haexSpaceKeys).values({
        spaceId,
        generation,
        key: keyBase64,
      })
    }
  }

  const deleteSpaceKeysAsync = async (spaceId: string) => {
    for (const key of spaceKeyCache.keys()) {
      if (key.startsWith(`${spaceId}:`)) {
        spaceKeyCache.delete(key)
      }
    }

    const db = getDb()
    if (!db) return
    await db.delete(haexSpaceKeys).where(eq(haexSpaceKeys.spaceId, spaceId))
  }

  // =========================================================================
  // Space Name Decryption
  // =========================================================================

  const resolveSpaceNameAsync = async (space: SharedSpace): Promise<string> => {
    const spaceKey = await getSpaceKeyAsync(space.id, space.currentKeyGeneration)
    if (!spaceKey) {
      return `Space ${space.id.slice(0, 8)}`
    }
    try {
      return await decryptSpaceNameAsync(spaceKey, space.encryptedName, space.nameNonce)
    } catch {
      return `Space ${space.id.slice(0, 8)}`
    }
  }

  // =========================================================================
  // Space CRUD
  // =========================================================================

  const createLocalSpaceAsync = async (spaceName: string, spaceId?: string) => {
    const id = spaceId || crypto.randomUUID()
    const spaceKey = generateSpaceKey()

    const space: DecryptedSpace = {
      id,
      name: spaceName,
      role: SpaceRoles.ADMIN,
      serverUrl: '',
      createdAt: new Date().toISOString(),
    }

    await persistSpaceAsync(space)
    await persistSpaceKeyAsync(id, 1, spaceKey)

    log.info(`Created local space "${spaceName}" (${id})`)
    return { id }
  }

  const DEFAULT_SPACE_ID = 'default'

  const ensureDefaultSpaceAsync = async () => {
    const db = getDb()
    if (!db) return

    const existing = await db.select().from(haexSpaces).where(eq(haexSpaces.id, DEFAULT_SPACE_ID)).limit(1)
    if (existing.length > 0) {
      // Ensure in-memory list is populated
      if (!spaces.value.find(s => s.id === DEFAULT_SPACE_ID)) {
        spaces.value.push(rowToDecryptedSpace(existing[0]))
      }
      return
    }

    await createLocalSpaceAsync('Personal', DEFAULT_SPACE_ID)
    log.info('Default space created')
  }

  const createSpaceAsync = async (serverUrl: string, spaceName: string, selfLabel: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)

    const spaceId = crypto.randomUUID()
    const spaceKey = generateSpaceKey()
    const { encryptedName, nameNonce } = await encryptSpaceNameAsync(spaceKey, spaceName)

    const keyGrant = await encryptWithPublicKeyAsync(spaceKey, identity.publicKey)

    const response = await fetchWithAuth(`${serverUrl}/spaces`, {
      method: 'POST',
      body: JSON.stringify({
        id: spaceId,
        encryptedName,
        nameNonce,
        label: selfLabel,
        keyGrant: {
          encryptedSpaceKey: keyGrant.encryptedData,
          keyNonce: keyGrant.nonce,
          ephemeralPublicKey: keyGrant.ephemeralPublicKey,
        },
      }),
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to create space: ${error.error || JSON.stringify(error) || response.statusText}`)
    }

    await persistSpaceKeyAsync(spaceId, 1, spaceKey)

    log.info(`Created space ${spaceId}`)
    await listSpacesAsync(serverUrl)
    return { id: spaceId }
  }

  const updateSpaceNameAsync = async (spaceId: string, newName: string) => {
    const space = spaces.value.find(s => s.id === spaceId)
    if (!space) throw new Error('Space not found')

    if (space.serverUrl) {
      const spaceKey = await getSpaceKeyAsync(spaceId, 1)
      if (!spaceKey) throw new Error('Space key not found')

      const { encryptedName, nameNonce } = await encryptSpaceNameAsync(spaceKey, newName)
      const response = await fetchWithAuth(`${space.serverUrl}/spaces/${spaceId}`, {
        method: 'PATCH',
        body: JSON.stringify({ encryptedName, nameNonce }),
      })
      if (!response.ok) {
        const error = await response.json().catch(() => ({}))
        throw new Error(`Failed to update space name: ${error.error || response.statusText}`)
      }
    }

    await persistSpaceAsync({ ...space, name: newName })
    log.info(`Updated space "${spaceId}" name to "${newName}"`)
  }

  const migrateSpaceServerAsync = async (
    spaceId: string,
    oldServerUrl: string,
    newServerUrl: string,
    identityId: string,
  ) => {
    const identity = await resolveIdentityAsync(identityId)
    const spaceKey = await getSpaceKeyAsync(spaceId, 1)
    if (!spaceKey) throw new Error('Space key not found')

    const space = spaces.value.find(s => s.id === spaceId)
    if (!space) throw new Error('Space not found')

    if (oldServerUrl) {
      try {
        const response = await fetchWithAuth(`${oldServerUrl}/spaces/${spaceId}`, {
          method: 'DELETE',
        })
        if (!response.ok && response.status !== 404) {
          log.warn(`Failed to delete space from old server (${response.status}), proceeding anyway`)
        }
      } catch (e) {
        log.warn(`Old server unreachable, space may remain there: ${e}`)
      }
    }

    if (newServerUrl) {
      const { encryptedName, nameNonce } = await encryptSpaceNameAsync(spaceKey, space.name)
      const keyGrant = await encryptWithPublicKeyAsync(spaceKey as Uint8Array<ArrayBuffer>, identity.publicKey)

      const response = await fetchWithAuth(`${newServerUrl}/spaces`, {
        method: 'POST',
        body: JSON.stringify({
          id: spaceId,
          encryptedName,
          nameNonce,
          label: identity.label,
          keyGrant: {
            encryptedSpaceKey: keyGrant.encryptedData,
            keyNonce: keyGrant.nonce,
            ephemeralPublicKey: keyGrant.ephemeralPublicKey,
          },
        }),
      })

      if (!response.ok) {
        const error = await response.json().catch(() => ({}))
        throw new Error(`Failed to create space on new server: ${error.error || response.statusText}`)
      }
    }

    await persistSpaceAsync({ ...space, serverUrl: newServerUrl })
    log.info(`Migrated space "${spaceId}" from "${oldServerUrl || 'local'}" to "${newServerUrl || 'local'}"`)
  }

  const listSpacesAsync = async (serverUrl: string) => {
    const response = await fetchWithAuth(`${serverUrl}/spaces`)
    if (!response.ok) throw new Error('Failed to list spaces')
    const rawSpaces = await response.json() as SharedSpace[]

    const decrypted: DecryptedSpace[] = await Promise.all(
      rawSpaces.map(async (space) => ({
        id: space.id,
        name: await resolveSpaceNameAsync(space),
        role: space.role,
        serverUrl,
        createdAt: space.createdAt,
      })),
    )

    // Persist all remote spaces to local DB
    for (const space of decrypted) {
      await persistSpaceAsync(space)
    }

    return decrypted
  }

  const inviteMemberAsync = async (
    serverUrl: string,
    spaceId: string,
    inviteePublicKey: string,
    label: string,
    role: SpaceRole,
    identityId: string,
  ): Promise<SpaceInvite> => {
    const identity = await resolveIdentityAsync(identityId)

    const grantsResponse = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}/key-grants`)
    if (!grantsResponse.ok) throw new Error('Failed to get key grants')
    const grants = await grantsResponse.json()

    const latestGrant = grants.sort(
      (first: { generation: number }, second: { generation: number }) => second.generation - first.generation,
    )[0]
    if (!latestGrant) throw new Error('No key grants found')

    let spaceKey = await getSpaceKeyAsync(spaceId, latestGrant.generation)
    if (!spaceKey) {
      spaceKey = await decryptWithPrivateKeyAsync(
        {
          encryptedData: latestGrant.encryptedSpaceKey,
          nonce: latestGrant.keyNonce,
          ephemeralPublicKey: latestGrant.ephemeralPublicKey,
        },
        identity.privateKey,
      )
      await persistSpaceKeyAsync(spaceId, latestGrant.generation, spaceKey)
    }

    const keyGrant = await encryptWithPublicKeyAsync(new Uint8Array(spaceKey), inviteePublicKey)

    const memberResponse = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}/members`, {
      method: 'POST',
      body: JSON.stringify({
        publicKey: inviteePublicKey,
        label,
        role,
        keyGrant: {
          encryptedSpaceKey: keyGrant.encryptedData,
          keyNonce: keyGrant.nonce,
          ephemeralPublicKey: keyGrant.ephemeralPublicKey,
          generation: latestGrant.generation,
        },
      }),
    })

    if (!memberResponse.ok) {
      const error = await memberResponse.json().catch(() => ({}))
      throw new Error(`Failed to invite member: ${error.error || memberResponse.statusText}`)
    }

    const tokenResponse = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}/tokens`, {
      method: 'POST',
      body: JSON.stringify({
        publicKey: inviteePublicKey,
        role,
        label: `Token for ${label}`,
      }),
    })

    if (!tokenResponse.ok) {
      throw new Error('Failed to create access token')
    }

    const { token: accessToken } = await tokenResponse.json()
    const spaceResponse = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}`)
    const spaceData = await spaceResponse.json()

    const invite: SpaceInvite = {
      spaceId,
      serverUrl,
      spaceName: spaceData.encryptedName,
      accessToken,
      encryptedSpaceKey: keyGrant.encryptedData,
      keyNonce: keyGrant.nonce,
      ephemeralPublicKey: keyGrant.ephemeralPublicKey,
      generation: latestGrant.generation,
      role,
    }

    log.info(`Invited ${label} (${inviteePublicKey.slice(0, 16)}...) to space ${spaceId} as ${role}`)
    return invite
  }

  const joinSpaceFromInviteAsync = async (invite: SpaceInvite, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)

    const spaceKey = await decryptWithPrivateKeyAsync(
      {
        encryptedData: invite.encryptedSpaceKey,
        nonce: invite.keyNonce,
        ephemeralPublicKey: invite.ephemeralPublicKey,
      },
      identity.privateKey,
    )

    await persistSpaceKeyAsync(invite.spaceId, invite.generation, spaceKey)

    log.info(`Joined space ${invite.spaceId} via invite`)
    return { spaceId: invite.spaceId, spaceKey }
  }

  const leaveSpaceAsync = async (serverUrl: string, spaceId: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)

    const response = await fetchWithAuth(
      `${serverUrl}/spaces/${spaceId}/members/${encodeURIComponent(identity.publicKey)}`,
      { method: 'DELETE' },
    )

    if (!response.ok && response.status !== 404) {
      throw new Error('Failed to leave space')
    }

    await removeSpaceFromDbAsync(spaceId)
    await deleteSpaceKeysAsync(spaceId)
    log.info(`Left space ${spaceId}`)
  }

  const deleteSpaceAsync = async (serverUrl: string, spaceId: string) => {
    const response = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}`, {
      method: 'DELETE',
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to delete space: ${error.error || response.statusText}`)
    }

    await removeSpaceFromDbAsync(spaceId)
    await deleteSpaceKeysAsync(spaceId)
    log.info(`Deleted space ${spaceId}`)
  }

  const removeIdentityFromSpaceAsync = async (spaceId: string, identityPublicKey: string) => {
    const db = getDb()
    if (!db) throw new Error('No vault open')

    const { haexSpaceDevices } = await import('~/database/schemas')

    await db.delete(haexSpaceDevices)
      .where(and(
        eq(haexSpaceDevices.spaceId, spaceId),
        eq(haexSpaceDevices.identityId, identityPublicKey),
      ))

    const space = spaces.value.find(s => s.id === spaceId)
    if (space?.serverUrl) {
      try {
        const response = await fetchWithAuth(
          `${space.serverUrl}/spaces/${spaceId}/members/${encodeURIComponent(identityPublicKey)}`,
          { method: 'DELETE' },
        )
        if (!response.ok && response.status !== 404) {
          log.warn(`Failed to remove member from server (${response.status})`)
        }
      } catch (e) {
        log.warn(`Server unreachable, member removed locally only: ${e}`)
      }
    }

    try {
      await invoke('peer_storage_reload_shares')
    } catch {
      // P2P endpoint may not be running
    }

    log.info(`Removed identity ${identityPublicKey.slice(0, 20)}... from space ${spaceId}`)
  }

  const clearCache = () => {
    spaceKeyCache.clear()
    spaces.value = []
  }

  return {
    spaces,
    getSpaceKey,
    getSpaceKeyAsync,
    getSpaceKeysAsync,
    persistSpaceKeyAsync,
    loadSpacesFromDbAsync,
    createLocalSpaceAsync,
    ensureDefaultSpaceAsync,
    createSpaceAsync,
    updateSpaceNameAsync,
    migrateSpaceServerAsync,
    listSpacesAsync,
    resolveSpaceNameAsync,
    inviteMemberAsync,
    joinSpaceFromInviteAsync,
    leaveSpaceAsync,
    deleteSpaceAsync,
    fetchWithSpaceToken,
    removeIdentityFromSpaceAsync,
    clearCache,
  }
})
