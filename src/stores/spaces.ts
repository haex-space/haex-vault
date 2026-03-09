import {
  generateSpaceKey,
  encryptSpaceKeyForRecipientAsync,
  decryptSpaceKeyAsync,
  encryptSpaceNameAsync,
  decryptSpaceNameAsync,
  arrayBufferToBase64,
  base64ToArrayBuffer,
  type SharedSpace,
  type SpaceInvite,
  type DecryptedSpace,
} from '@haex-space/vault-sdk'
import { eq, and } from 'drizzle-orm'
import { haexSpaceKeys } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { getAuthTokenAsync } from '@/stores/sync/engine/supabase'

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const userKeypairStore = useUserKeypairStore()
  const { currentVault } = storeToRefs(useVaultStore())

  // In-memory read-through cache (backed by DB)
  const spaceKeyCache = new Map<string, Uint8Array>()

  const spaces = ref<SharedSpace[]>([])

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
    // Check memory cache first
    const cached = spaceKeyCache.get(`${spaceId}:${generation}`)
    if (cached) return cached

    // Load from DB
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
    // Update memory cache
    spaceKeyCache.set(`${spaceId}:${generation}`, key)

    // Persist to DB
    if (!currentVault.value?.drizzle) return
    const keyBase64 = arrayBufferToBase64(key.buffer as ArrayBuffer)
    await currentVault.value.drizzle
      .insert(haexSpaceKeys)
      .values({ spaceId, generation, key: keyBase64 })
      .onConflictDoUpdate({
        target: [haexSpaceKeys.spaceId, haexSpaceKeys.generation],
        set: { key: keyBase64 },
      })
  }

  const deleteSpaceKeysAsync = async (spaceId: string) => {
    // Clear memory cache
    for (const cacheKey of spaceKeyCache.keys()) {
      if (cacheKey.startsWith(`${spaceId}:`)) {
        spaceKeyCache.delete(cacheKey)
      }
    }

    // Delete from DB
    if (!currentVault.value?.drizzle) return
    await currentVault.value.drizzle
      .delete(haexSpaceKeys)
      .where(eq(haexSpaceKeys.spaceId, spaceId))
  }

  // =========================================================================
  // Space Name Decryption
  // =========================================================================

  /**
   * Decrypt a space name using the persisted space key.
   */
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

  /**
   * List spaces from a server with decrypted names.
   */
  const listDecryptedSpacesAsync = async (serverUrl: string): Promise<DecryptedSpace[]> => {
    const rawSpaces = await listSpacesAsync(serverUrl)
    return Promise.all(
      rawSpaces.map(async (space) => ({
        id: space.id,
        name: await resolveSpaceNameAsync(space),
        role: space.role,
        canInvite: space.canInvite,
        serverUrl,
        createdAt: space.createdAt,
      })),
    )
  }

  // =========================================================================
  // Space CRUD
  // =========================================================================

  /**
   * Create a new shared space
   */
  const createSpaceAsync = async (serverUrl: string, spaceName: string, selfLabel: string) => {
    if (!userKeypairStore.publicKeyBase64 || !userKeypairStore.privateKeyBase64) {
      throw new Error('User keypair not available')
    }

    // Generate space key (AES-256)
    const spaceKey = generateSpaceKey()

    // Encrypt space name with space key
    const { encryptedName, nameNonce } = await encryptSpaceNameAsync(spaceKey, spaceName)

    // Encrypt space key for self (ECDH)
    const keyGrant = await encryptSpaceKeyForRecipientAsync(
      spaceKey, userKeypairStore.publicKeyBase64,
    )

    // Create on server
    const response = await fetchWithAuth(`${serverUrl}/spaces`, {
      method: 'POST',
      body: JSON.stringify({
        encryptedName,
        nameNonce,
        label: selfLabel,
        keyGrant: {
          encryptedSpaceKey: keyGrant.encryptedSpaceKey,
          keyNonce: keyGrant.keyNonce,
          ephemeralPublicKey: keyGrant.ephemeralPublicKey,
        },
      }),
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to create space: ${error.error || response.statusText}`)
    }

    const data = await response.json()

    // Persist space key
    await persistSpaceKeyAsync(data.space.id, 1, spaceKey)

    log.info(`Created space ${data.space.id}`)
    await listSpacesAsync(serverUrl)
    return data.space
  }

  /**
   * List all spaces the user is a member of
   */
  const listSpacesAsync = async (serverUrl: string) => {
    const response = await fetchWithAuth(`${serverUrl}/spaces`)
    if (!response.ok) throw new Error('Failed to list spaces')
    const data = await response.json()
    spaces.value = data
    return data as SharedSpace[]
  }

  /**
   * Invite a member to a space by their public key
   */
  const inviteMemberAsync = async (
    serverUrl: string,
    spaceId: string,
    inviteePublicKey: string,
    label: string,
    role: 'member' | 'viewer',
    canInvite: boolean = false,
  ): Promise<SpaceInvite> => {
    if (!userKeypairStore.privateKeyBase64) {
      throw new Error('User keypair not available')
    }

    // Get current space key (need own key grants to decrypt it first)
    const grantsResponse = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}/key-grants`)
    if (!grantsResponse.ok) throw new Error('Failed to get key grants')
    const grants = await grantsResponse.json()

    // Find the latest generation grant
    const latestGrant = grants.sort(
      (first: { generation: number }, second: { generation: number }) => second.generation - first.generation,
    )[0]
    if (!latestGrant) throw new Error('No key grants found')

    // Get space key (from DB/cache, or decrypt from server grant)
    let spaceKey = await getSpaceKeyAsync(spaceId, latestGrant.generation)
    if (!spaceKey) {
      spaceKey = await decryptSpaceKeyAsync(
        {
          encryptedSpaceKey: latestGrant.encryptedSpaceKey,
          keyNonce: latestGrant.keyNonce,
          ephemeralPublicKey: latestGrant.ephemeralPublicKey,
        },
        userKeypairStore.privateKeyBase64,
      )
      await persistSpaceKeyAsync(spaceId, latestGrant.generation, spaceKey)
    }

    // Encrypt space key for invitee
    const keyGrant = await encryptSpaceKeyForRecipientAsync(new Uint8Array(spaceKey), inviteePublicKey)

    // Add member + key grant on server
    const memberResponse = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}/members`, {
      method: 'POST',
      body: JSON.stringify({
        publicKey: inviteePublicKey,
        label,
        role,
        canInvite,
        keyGrant: {
          encryptedSpaceKey: keyGrant.encryptedSpaceKey,
          keyNonce: keyGrant.keyNonce,
          ephemeralPublicKey: keyGrant.ephemeralPublicKey,
          generation: latestGrant.generation,
        },
      }),
    })

    if (!memberResponse.ok) {
      const error = await memberResponse.json().catch(() => ({}))
      throw new Error(`Failed to invite member: ${error.error || memberResponse.statusText}`)
    }

    // Create access token for invitee
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

    // Get space details for the invite
    const spaceResponse = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}`)
    const spaceData = await spaceResponse.json()

    // Build invite payload
    const invite: SpaceInvite = {
      spaceId,
      serverUrl,
      spaceName: spaceData.encryptedName,
      accessToken,
      encryptedSpaceKey: keyGrant.encryptedSpaceKey,
      keyNonce: keyGrant.keyNonce,
      ephemeralPublicKey: keyGrant.ephemeralPublicKey,
      generation: latestGrant.generation,
      role,
    }

    log.info(`Invited ${label} (${inviteePublicKey.slice(0, 16)}...) to space ${spaceId} as ${role}`)
    return invite
  }

  /**
   * Join a space from an invite
   */
  const joinSpaceFromInviteAsync = async (invite: SpaceInvite) => {
    if (!userKeypairStore.privateKeyBase64) {
      throw new Error('User keypair not available')
    }

    // Decrypt the space key
    const spaceKey = await decryptSpaceKeyAsync(
      {
        encryptedSpaceKey: invite.encryptedSpaceKey,
        keyNonce: invite.keyNonce,
        ephemeralPublicKey: invite.ephemeralPublicKey,
      },
      userKeypairStore.privateKeyBase64,
    )

    // Persist space key
    await persistSpaceKeyAsync(invite.spaceId, invite.generation, spaceKey)

    log.info(`Joined space ${invite.spaceId} via invite`)
    return { spaceId: invite.spaceId, spaceKey }
  }

  /**
   * Leave a space (removes membership on server, keeps local data)
   */
  const leaveSpaceAsync = async (serverUrl: string, spaceId: string) => {
    if (!userKeypairStore.publicKeyBase64) {
      throw new Error('User keypair not available')
    }

    const response = await fetchWithAuth(
      `${serverUrl}/spaces/${spaceId}/members/${encodeURIComponent(userKeypairStore.publicKeyBase64)}`,
      { method: 'DELETE' },
    )

    // 404 means already left, which is fine
    if (!response.ok && response.status !== 404) {
      throw new Error('Failed to leave space')
    }

    // Remove from local list
    spaces.value = spaces.value.filter(space => space.id !== spaceId)

    // Delete persisted keys
    await deleteSpaceKeysAsync(spaceId)

    log.info(`Left space ${spaceId}`)
  }

  /**
   * Delete a space (admin only)
   */
  const deleteSpaceAsync = async (serverUrl: string, spaceId: string) => {
    const response = await fetchWithAuth(`${serverUrl}/spaces/${spaceId}`, {
      method: 'DELETE',
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({}))
      throw new Error(`Failed to delete space: ${error.error || response.statusText}`)
    }

    // Remove from local list
    spaces.value = spaces.value.filter(space => space.id !== spaceId)

    // Delete persisted keys
    await deleteSpaceKeysAsync(spaceId)

    log.info(`Deleted space ${spaceId}`)
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
    createSpaceAsync,
    listSpacesAsync,
    listDecryptedSpacesAsync,
    resolveSpaceNameAsync,
    inviteMemberAsync,
    joinSpaceFromInviteAsync,
    leaveSpaceAsync,
    deleteSpaceAsync,
    fetchWithSpaceToken,
    clearCache,
  }
})
