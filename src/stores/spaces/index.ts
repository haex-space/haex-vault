import type { DecryptedSpace } from '@haex-space/vault-sdk'
import { eq } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { haexSpaces } from '~/database/schemas'
import type { SelectHaexSpaces } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import { SpaceType, SpaceStatus } from '~/database/constants'
import type { SpaceType as SpaceTypeValue, SpaceStatus as SpaceStatusValue } from '~/database/constants'
import spacesDe from './spaces.de.json'
import spacesEn from './spaces.en.json'

// Module imports
import { addMemberToSpace, getSpaceMembers, updateOwnSpaceProfile, getMemberPublicKeysForSpace, removeSpaceMember, migrateExistingMembers } from './members'
import { getCapabilitiesForSpace, hasCapability } from './capabilities'
import { setupFederationForSpace } from './federation'
import { inviteMember, createInviteToken, buildInviteLink, claimInviteToken, finalizeInvite, processWelcomes, acceptLocalInvite, queueQuicInvite } from './invites'
import { createLocalSpace, createOnlineSpace, updateSpaceName, migrateSpaceServer, listSpaces, leaveSpace, deleteSpace, removeIdentityFromSpace } from './crud'

/** Extended space type including the DB type field (vault/online/local) */
export interface SpaceWithType extends DecryptedSpace {
  type: SpaceTypeValue
  status: SpaceStatusValue
}

export interface ResolvedIdentity {
  id: string
  publicKey: string
  privateKey: string
  did: string
  label: string
}

const log = createLogger('SPACES')

export const useSpacesStore = defineStore('spacesStore', () => {
  const { $i18n } = useNuxtApp()
  $i18n.mergeLocaleMessage('de', { spaces: spacesDe })
  $i18n.mergeLocaleMessage('en', { spaces: spacesEn })

  const { currentVault } = storeToRefs(useVaultStore())

  // =========================================================================
  // State
  // =========================================================================

  const spaces = ref<SpaceWithType[]>([])
  const db = computed(() => currentVault.value?.drizzle)
  const visibleSpaces = computed(() => spaces.value.filter(s => s.type !== SpaceType.VAULT))
  const activeSpaces = computed(() => visibleSpaces.value.filter(s => s.status === SpaceStatus.ACTIVE))
  const pendingSpaces = computed(() => visibleSpaces.value.filter(s => s.status === SpaceStatus.PENDING))

  // =========================================================================
  // Internal helpers
  // =========================================================================

  const rowToSpace = (row: SelectHaexSpaces): SpaceWithType => ({
    id: row.id,
    name: row.name,
    type: (row.type as SpaceTypeValue) ?? SpaceType.ONLINE,
    status: (row.status as SpaceStatusValue) ?? SpaceStatus.ACTIVE,
    serverUrl: row.originUrl ?? '',
    createdAt: row.createdAt ?? '',
  })

  const resolveIdentityAsync = async (identityId: string): Promise<ResolvedIdentity> => {
    const identityStore = useIdentityStore()
    const identity = await identityStore.getIdentityByIdAsync(identityId)
    if (!identity?.privateKey) throw new Error(`Identity ${identityId} not found or has no private key`)
    return { id: identity.id, publicKey: identity.publicKey, privateKey: identity.privateKey, did: identity.did, label: identity.label }
  }

  const requireDb = () => {
    const d = db.value
    if (!d) throw new Error('No vault open')
    return d
  }

  // =========================================================================
  // Persistence
  // =========================================================================

  const persistSpaceAsync = async (space: SpaceWithType) => {
    const d = db.value
    if (!d) return

    const existing = await d.select().from(haexSpaces).where(eq(haexSpaces.id, space.id)).limit(1)

    if (existing.length > 0) {
      await d.update(haexSpaces).set({
        name: space.name,
        originUrl: space.serverUrl || null,
        status: space.status,
        modifiedAt: new Date().toISOString(),
      }).where(eq(haexSpaces.id, space.id))
    } else {
      await d.insert(haexSpaces).values({
        id: space.id,
        type: space.type,
        name: space.name,
        originUrl: space.serverUrl || null,
        status: space.status,
      })
    }

    const idx = spaces.value.findIndex(s => s.id === space.id)
    if (idx >= 0) {
      spaces.value[idx] = space
    } else {
      spaces.value.push(space)
    }
  }

  const removeSpaceFromDbAsync = async (spaceId: string) => {
    const d = db.value
    if (d) {
      await d.delete(haexSpaces).where(eq(haexSpaces.id, spaceId))
    }
    spaces.value = spaces.value.filter(s => s.id !== spaceId)
  }

  const loadSpacesFromDbAsync = async () => {
    const d = db.value
    if (!d) return
    const rows = await d.select().from(haexSpaces)
    spaces.value = rows.map(rowToSpace)
  }

  // =========================================================================
  // Startup
  // =========================================================================

  const startLocalSpaceLeadersAsync = async () => {
    for (const space of spaces.value) {
      if (space.type === SpaceType.LOCAL && space.status === SpaceStatus.ACTIVE) {
        try {
          await invoke('local_delivery_start', { spaceId: space.id })
          log.info(`Started leader mode for local space ${space.id}`)
        } catch {
          // Already running — ignore
        }
      }
    }
  }

  const ensureVaultSpaceAsync = async (vaultId: string, vaultName: string) => {
    const d = db.value
    if (!d) {
      console.error('[SPACES] ensureVaultSpaceAsync: no DB available')
      return
    }

    const existing = await d.select().from(haexSpaces).where(eq(haexSpaces.id, vaultId)).limit(1)
    if (existing.length > 0) {
      log.info(`Vault space ${vaultId} already exists`)
      return
    }

    await d.insert(haexSpaces).values({
      id: vaultId,
      type: SpaceType.VAULT,
      name: vaultName,
      originUrl: '',
    })
    log.info(`Created vault space "${vaultName}" (${vaultId})`)
  }

  const ensureDefaultSpaceAsync = async () => {
    const d = db.value
    if (!d) return

    const localSpaces = await d.select().from(haexSpaces).where(eq(haexSpaces.type, SpaceType.LOCAL)).limit(1)

    if (localSpaces.length > 0) {
      if (!spaces.value.find(s => s.id === localSpaces[0]!.id)) {
        spaces.value.push(rowToSpace(localSpaces[0]!))
      }
      return
    }

    const name = $i18n.t('spaces.defaultSpaceName')
    await createLocalSpaceAsync(name)
    log.info(`Default space "${name}" created`)
  }

  // =========================================================================
  // Delegating wrappers — thin functions that pass state to module functions
  // =========================================================================

  const createLocalSpaceAsync = (spaceName: string, spaceId?: string) =>
    createLocalSpace(requireDb(), spaceName, persistSpaceAsync, spaceId)

  const createSpaceAsync = async (serverUrl: string, spaceName: string, selfLabel: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return createOnlineSpace(
      requireDb(), serverUrl, spaceName, selfLabel, identity, persistSpaceAsync,
      async () => { await listSpacesAsync(serverUrl, identityId) },
    )
  }

  const updateSpaceNameAsync = (spaceId: string, newName: string) =>
    updateSpaceName(spaces.value, spaceId, newName, persistSpaceAsync)

  const migrateSpaceServerAsync = async (spaceId: string, oldServerUrl: string, newServerUrl: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return migrateSpaceServer(spaces.value, spaceId, oldServerUrl, newServerUrl, identity, persistSpaceAsync)
  }

  const listSpacesAsync = async (serverUrl: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return listSpaces(identity, serverUrl, persistSpaceAsync)
  }

  const leaveSpaceAsync = async (serverUrl: string, spaceId: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return leaveSpace(identity, serverUrl, spaceId, removeSpaceFromDbAsync)
  }

  const deleteSpaceAsync = (serverUrl: string, spaceId: string) =>
    deleteSpace(spaces.value, serverUrl, spaceId, removeSpaceFromDbAsync)

  const removeIdentityFromSpaceAsync = (spaceId: string, identityPublicKey: string) =>
    removeIdentityFromSpace(requireDb(), spaces.value, spaceId, identityPublicKey)

  const removeSpaceMemberAsync = (spaceId: string, memberDid: string) =>
    removeSpaceMember(requireDb(), spaceId, memberDid)

  const inviteMemberAsync = async (serverUrl: string, spaceId: string, inviteeDid: string, capability: string, identityId: string, includeHistory = false) => {
    const identity = await resolveIdentityAsync(identityId)
    return inviteMember(spaces.value, serverUrl, spaceId, inviteeDid, capability, identity, includeHistory)
  }

  const createInviteTokenAsync = (serverUrl: string, spaceId: string, options: { capability?: string; maxUses?: number; expiresInSeconds: number; label?: string }) =>
    createInviteToken(spaces.value, serverUrl, spaceId, options)

  const claimInviteTokenAsync = async (serverUrl: string, spaceId: string, tokenId: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return claimInviteToken(requireDb(), serverUrl, spaceId, tokenId, identity, persistSpaceAsync)
  }

  const finalizeInviteAsync = async (serverUrl: string, spaceId: string, inviteeDid: string, identityId: string, inviteId?: string, capability?: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return finalizeInvite(serverUrl, spaceId, inviteeDid, identity, inviteId, capability)
  }

  const processWelcomesAsync = async (serverUrl: string, spaceId: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return processWelcomes(serverUrl, spaceId, identity)
  }

  const acceptLocalInviteAsync = (invite: Parameters<typeof acceptLocalInvite>[1]) =>
    acceptLocalInvite(requireDb(), invite, persistSpaceAsync, loadSpacesFromDbAsync)

  const queueQuicInviteAsync = (params: Parameters<typeof queueQuicInvite>[1]) =>
    queueQuicInvite(requireDb(), params)

  const setupFederationForSpaceAsync = async (relayServerUrl: string, originServerUrl: string, spaceId: string, identityId: string) => {
    const identity = await resolveIdentityAsync(identityId)
    return setupFederationForSpace(relayServerUrl, originServerUrl, spaceId, identity)
  }

  const getCapabilitiesForSpaceAsync = async (spaceId: string) => {
    const d = db.value
    if (!d) return []
    const identityStore = useIdentityStore()
    return getCapabilitiesForSpace(d, spaceId, identityStore.ownIdentities.map(i => i.did))
  }

  const hasCapabilityAsync = async (spaceId: string, capability: string) => {
    const d = db.value
    if (!d) return false
    const identityStore = useIdentityStore()
    return hasCapability(d, spaceId, capability, identityStore.ownIdentities.map(i => i.did))
  }

  const addMemberToSpaceAsync = (params: Parameters<typeof addMemberToSpace>[1]) =>
    addMemberToSpace(requireDb(), params)

  const getSpaceMembersAsync = (spaceId: string) =>
    getSpaceMembers(requireDb(), spaceId)

  const updateOwnSpaceProfileAsync = (spaceId: string, profile: { label?: string; avatar?: string | null; avatarOptions?: string | null }) => {
    const d = db.value
    if (!d) return
    const identityStore = useIdentityStore()
    return updateOwnSpaceProfile(d, identityStore.ownIdentities.map(i => i.did), spaceId, profile)
  }

  const getMemberPublicKeysForSpaceAsync = (spaceId: string) =>
    getMemberPublicKeysForSpace(requireDb(), spaceId)

  const migrateExistingMembersAsync = async () => {
    const d = db.value
    if (!d) return
    const identityStore = useIdentityStore()
    await identityStore.loadIdentitiesAsync()
    return migrateExistingMembers(d, identityStore.identities.map((i: { did: string; label: string; avatar: string | null; avatarOptions: string | null }) => ({
      did: i.did,
      label: i.label,
      avatar: i.avatar,
      avatarOptions: i.avatarOptions,
    })))
  }

  const clearCache = () => {
    spaces.value = []
  }

  return {
    spaces,
    visibleSpaces,
    activeSpaces,
    pendingSpaces,
    loadSpacesFromDbAsync,
    createLocalSpaceAsync,
    ensureVaultSpaceAsync,
    ensureDefaultSpaceAsync,
    createSpaceAsync,
    updateSpaceNameAsync,
    migrateSpaceServerAsync,
    listSpacesAsync,
    inviteMemberAsync,
    createInviteTokenAsync,
    buildInviteLink,
    claimInviteTokenAsync,
    finalizeInviteAsync,
    processWelcomesAsync,
    leaveSpaceAsync,
    deleteSpaceAsync,
    removeIdentityFromSpaceAsync,
    removeSpaceMemberAsync,
    setupFederationForSpaceAsync,
    getCapabilitiesForSpaceAsync,
    hasCapabilityAsync,
    addMemberToSpaceAsync,
    getSpaceMembersAsync,
    updateOwnSpaceProfileAsync,
    getMemberPublicKeysForSpaceAsync,
    migrateExistingMembersAsync,
    queueQuicInviteAsync,
    acceptLocalInviteAsync,
    persistSpaceAsync,
    startLocalSpaceLeadersAsync,
    removeSpaceFromDbAsync,
    clearCache,
  }
})
