import {
  generateSpaceKey,
  encryptSpaceKeyForRecipientAsync,
  decryptSpaceKeyAsync,
  encryptString,
  type SharedSpace,
  type SpaceInvite,
} from '@haex-space/vault-sdk'
import { createLogger } from '@/stores/logging'
import { getAuthTokenAsync } from '@/stores/sync/engine/supabase'

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const userKeypairStore = useUserKeypairStore()

  // Space key cache: Map<`${spaceId}:${generation}`, Uint8Array>
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

  const getSpaceKey = (spaceId: string, generation: number): Uint8Array | undefined => {
    return spaceKeyCache.get(`${spaceId}:${generation}`)
  }

  const cacheSpaceKey = (spaceId: string, generation: number, key: Uint8Array) => {
    spaceKeyCache.set(`${spaceId}:${generation}`, key)
  }

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
    const cryptoKey = await crypto.subtle.importKey(
      'raw', spaceKey, { name: 'AES-GCM' }, false, ['encrypt'],
    )
    const { encryptedData, nonce } = await encryptString(spaceName, cryptoKey)

    // Encrypt space key for self (ECDH)
    const keyGrant = await encryptSpaceKeyForRecipientAsync(
      spaceKey, userKeypairStore.publicKeyBase64,
    )

    // Create on server
    const response = await fetchWithAuth(`${serverUrl}/spaces`, {
      method: 'POST',
      body: JSON.stringify({
        encryptedName: encryptedData,
        nameNonce: nonce,
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

    // Cache space key
    cacheSpaceKey(data.space.id, 1, spaceKey)

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
    const latestGrant = grants.sort((a: { generation: number }, b: { generation: number }) => b.generation - a.generation)[0]
    if (!latestGrant) throw new Error('No key grants found')

    // Decrypt space key if not cached
    let spaceKey = getSpaceKey(spaceId, latestGrant.generation)
    if (!spaceKey) {
      spaceKey = await decryptSpaceKeyAsync(
        {
          encryptedSpaceKey: latestGrant.encryptedSpaceKey,
          keyNonce: latestGrant.keyNonce,
          ephemeralPublicKey: latestGrant.ephemeralPublicKey,
        },
        userKeypairStore.privateKeyBase64,
      )
      cacheSpaceKey(spaceId, latestGrant.generation, spaceKey)
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

    // Cache space key
    cacheSpaceKey(invite.spaceId, invite.generation, spaceKey)

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
    spaces.value = spaces.value.filter(s => s.id !== spaceId)

    // Clear cached space keys for this space
    for (const key of spaceKeyCache.keys()) {
      if (key.startsWith(`${spaceId}:`)) {
        spaceKeyCache.delete(key)
      }
    }

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
    spaces.value = spaces.value.filter(s => s.id !== spaceId)

    // Clear cached keys
    for (const key of spaceKeyCache.keys()) {
      if (key.startsWith(`${spaceId}:`)) {
        spaceKeyCache.delete(key)
      }
    }

    log.info(`Deleted space ${spaceId}`)
  }

  const clearCache = () => {
    spaceKeyCache.clear()
    spaces.value = []
  }

  return {
    spaces,
    getSpaceKey,
    cacheSpaceKey,
    createSpaceAsync,
    listSpacesAsync,
    inviteMemberAsync,
    joinSpaceFromInviteAsync,
    leaveSpaceAsync,
    deleteSpaceAsync,
    fetchWithSpaceToken,
    clearCache,
  }
})
