import {
  generateSpaceKey,
  encryptWithPublicKeyAsync,
  decryptWithPrivateKeyAsync,
  encryptSpaceNameAsync,
  decryptSpaceNameAsync,
  arrayBufferToBase64,
  base64ToArrayBuffer,
  type SharedSpace,
  type SpaceRole,
  type SpaceInvite,
  type DecryptedSpace,
} from '@haex-space/vault-sdk'
import { eq, and } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexSpaceKeys, haexSpaceDevices } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { getAuthTokenAsync } from '@/stores/sync/engine/supabase'

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  // In-memory read-through cache (backed by DB)
  const spaceKeyCache = new Map<string, Uint8Array>()

  const spaces = ref<DecryptedSpace[]>([])

  // Helper: resolve identity keys from identityId
  const resolveIdentityAsync = async (identityId: string) => {
    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityAsync(identityId)
    if (!identity) throw new Error(`Identity ${identityId} not found`)
    return identity
  }

  // Helper: fetch with auth
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

  // Helper: fetch with space token (for federated access)
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
  // Space Key Management (DB-backed with in-memory cache)
  // =========================================================================

  const getSpaceKey = (spaceId: string, generation: number): Uint8Array | undefined => {
    return spaceKeyCache.get(`${spaceId}:${generation}`)
  }

  const getSpaceKeyAsync = async (spaceId: string, generation: number): Promise<Uint8Array | undefined> => {
    const cached = spaceKeyCache.get(`${spaceId}:${generation}`)
    if (cached) return cached

    if (!currentVault.value?.drizzle) return undefined
    const rows = await currentVault.value.drizzle
      .select()
      .from(haexSpaceKeys)
      .where(and(
        eq(haexSpaceKeys.spaceId, spaceId),
        eq(haexSpaceKeys.generation, generation),
      ))
      .limit(1)

    const row = rows[0]
    if (!row) return undefined

    const key = new Uint8Array(base64ToArrayBuffer(row.key))
    spaceKeyCache.set(`${spaceId}:${generation}`, key)
    return key
  }

  const persistSpaceKeyAsync = async (spaceId: string, generation: number, key: Uint8Array) => {
    spaceKeyCache.set(`${spaceId}:${generation}`, key)

    if (!currentVault.value?.drizzle) return
    const keyBase64 = arrayBufferToBase64(key.buffer as ArrayBuffer)

    // Drizzle's onConflictDoUpdate generates table-qualified column names in ON CONFLICT
    // which SQLite doesn't support — use check-then-insert/update instead
    const existing = await currentVault.value.drizzle
      .select()
      .from(haexSpaceKeys)
      .where(and(
        eq(haexSpaceKeys.spaceId, spaceId),
        eq(haexSpaceKeys.generation, generation),
      ))
      .limit(1)

    if (existing.length > 0) {
      await currentVault.value.drizzle
        .update(haexSpaceKeys)
        .set({ key: keyBase64 })
        .where(and(
          eq(haexSpaceKeys.spaceId, spaceId),
          eq(haexSpaceKeys.generation, generation),
        ))
    }
    else {
      await currentVault.value.drizzle
        .insert(haexSpaceKeys)
        .values({ spaceId, generation, key: keyBase64 })
    }
  }

  const deleteSpaceKeysAsync = async (spaceId: string) => {
    for (const cacheKey of spaceKeyCache.keys()) {
      if (cacheKey.startsWith(`${spaceId}:`)) {
        spaceKeyCache.delete(cacheKey)
      }
    }

    if (!currentVault.value?.drizzle) return
    await currentVault.value.drizzle
      .delete(haexSpaceKeys)
      .where(eq(haexSpaceKeys.spaceId, spaceId))
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

  /**
   * Create a local space (no server required).
   * Used for P2P sharing, local-only grouping, and the default space.
   */
  const createLocalSpaceAsync = async (spaceName: string, spaceId?: string) => {
    const id = spaceId || crypto.randomUUID()
    const spaceKey = generateSpaceKey()

    await persistSpaceKeyAsync(id, 1, spaceKey)

    // Add to local spaces list
    const existing = spaces.value.find(s => s.id === id)
    if (!existing) {
      spaces.value.push({
        id,
        name: spaceName,
        role: 'admin' as SpaceRole,
        serverUrl: '',
        createdAt: new Date().toISOString(),
      })
    }

    log.info(`Created local space "${spaceName}" (${id})`)
    return { id }
  }

  /**
   * Ensures the default local space exists. Called on vault open.
   * Uses a deterministic ID so it's the same across devices.
   */
  const DEFAULT_SPACE_ID = 'default'

  const ensureDefaultSpaceAsync = async () => {
    const existingKey = await getSpaceKeyAsync(DEFAULT_SPACE_ID, 1)
    if (existingKey) {
      // Default space already exists — ensure it's in the list
      if (!spaces.value.find(s => s.id === DEFAULT_SPACE_ID)) {
        spaces.value.push({
          id: DEFAULT_SPACE_ID,
          name: 'Personal',
          role: 'admin' as SpaceRole,
          serverUrl: '',
          createdAt: '',
        })
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

    // Encrypt space key for self (ECDH with own public key)
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

  /**
   * Update a space's name. If the space has a server, update there too.
   */
  const updateSpaceNameAsync = async (spaceId: string, newName: string) => {
    const space = spaces.value.find(s => s.id === spaceId)
    if (!space) throw new Error('Space not found')

    // Update on server if remote
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

    // Update locally
    space.name = newName
    log.info(`Updated space "${spaceId}" name to "${newName}"`)
  }

  /**
   * Migrate a space from one server to another (or to/from local).
   */
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

    // 1. Delete from old server (if it had one)
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
        // TODO: Queue for later deletion
      }
    }

    // 2. Create on new server (if it has one)
    if (newServerUrl) {
      const { encryptedName, nameNonce } = await encryptSpaceNameAsync(spaceKey, space.name)
      const keyGrant = await encryptWithPublicKeyAsync(spaceKey, identity.publicKey)

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

    // 3. Update local state
    space.serverUrl = newServerUrl
    log.info(`Migrated space "${spaceId}" from "${oldServerUrl || 'local'}" to "${newServerUrl || 'local'}"`)
  }

  const listSpacesAsync = async (serverUrl: string) => {
    const response = await fetchWithAuth(`${serverUrl}/spaces`)
    if (!response.ok) throw new Error('Failed to list spaces')
    const rawSpaces = await response.json() as SharedSpace[]

    const decrypted = await Promise.all(
      rawSpaces.map(async (space) => ({
        id: space.id,
        name: await resolveSpaceNameAsync(space),
        role: space.role,
        serverUrl,
        createdAt: space.createdAt,
      })),
    )

    // Merge with existing spaces from other servers
    const otherSpaces = spaces.value.filter(s => s.serverUrl !== serverUrl)
    spaces.value = [...otherSpaces, ...decrypted]
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

    // Get key grants to decrypt current space key
    const grantsResponse = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}/key-grants`)
    if (!grantsResponse.ok) throw new Error('Failed to get key grants')
    const grants = await grantsResponse.json()

    const latestGrant = grants.sort(
      (first: { generation: number }, second: { generation: number }) => second.generation - first.generation,
    )[0]
    if (!latestGrant) throw new Error('No key grants found')

    // Get space key (from cache or decrypt from grant)
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

    // Encrypt space key for invitee
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

    spaces.value = spaces.value.filter(space => space.id !== spaceId)
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

    spaces.value = spaces.value.filter(space => space.id !== spaceId)
    await deleteSpaceKeysAsync(spaceId)
    log.info(`Deleted space ${spaceId}`)
  }

  /**
   * Remove an identity from a space. Deletes all devices of this identity
   * from haex_space_devices, which revokes P2P access via CRDT sync.
   * Also removes from server if remote space.
   */
  const removeIdentityFromSpaceAsync = async (spaceId: string, identityPublicKey: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    // Delete all devices of this identity from the space
    await db.delete(haexSpaceDevices)
      .where(and(
        eq(haexSpaceDevices.spaceId, spaceId),
        eq(haexSpaceDevices.identityId, identityPublicKey),
      ))

    // Remove from server if remote
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

    // Immediately reload P2P allowed peers so the removed identity is blocked
    try {
      await invoke('peer_storage_reload_shares')
    } catch {
      // P2P endpoint may not be running — that's fine
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
    persistSpaceKeyAsync,
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
