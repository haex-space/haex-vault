import { eq, ne, and, inArray } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import { createAvatar } from '@dicebear/core'
import * as toonHead from '@dicebear/toon-head'
import { generateIdentityAsync, publicKeyToDidKeyAsync } from '@haex-space/vault-sdk'
import {
  haexIdentities,
  haexIdentityClaims,
  haexUcanTokens,
  haexSpaces,
  haexSpaceDevices,
  haexPeerShares,
  haexSharedSpaceSync,
  haexSyncBackends,
  haexInviteOutbox,
  haexInviteTokens,
  type SelectHaexIdentities,
  type SelectHaexSpaces,
} from '~/database/schemas'
import { SpaceType } from '~/database/constants'
import { createLogger } from '@/stores/logging'

export interface ExportedIdentity {
  did: string
  label: string
  publicKey: string
  privateKey: string
  avatar?: string | null
  claims?: { type: string; value: string }[]
}

const log = createLogger('IDENTITY')

export const useIdentityStore = defineStore('identityStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const identities = ref<SelectHaexIdentities[]>([])

  // Computed views: own identities (have privateKey) vs contacts (no privateKey)
  const ownIdentities = computed(() => identities.value.filter(i => i.privateKey !== null))
  const contacts = computed(() => identities.value.filter(i => i.privateKey === null))

  // Session-only: identity passwords set during creation, consumed on first backend registration
  const _identityPasswords = new Map<string, string>()

  const setIdentityPassword = (identityId: string, password: string) => {
    _identityPasswords.set(identityId, password)
  }

  const consumeIdentityPassword = (identityId: string): string | undefined => {
    const pw = _identityPasswords.get(identityId)
    _identityPasswords.delete(identityId)
    return pw
  }

  const loadIdentitiesAsync = async () => {
    if (!currentVault.value?.drizzle) return
    identities.value = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .all()
    log.info(`Loaded ${identities.value.length} identities (${ownIdentities.value.length} own, ${contacts.value.length} contacts)`)
  }

  /** Register a temporary identity in-memory (e.g. from server recovery before vault is open) */
  const registerTemporaryIdentity = (identity: { id: string; publicKey: string; privateKey: string; did: string; label: string }) => {
    if (identities.value.find(i => i.id === identity.id)) return
    identities.value.push({
      id: identity.id,
      publicKey: identity.publicKey,
      privateKey: identity.privateKey,
      did: identity.did,
      label: identity.label,
      avatar: null,
      notes: null,
      createdAt: new Date().toISOString(),
    } as SelectHaexIdentities)
  }

  const createIdentityAsync = async (label: string): Promise<SelectHaexIdentities> => {
    if (!currentVault.value?.drizzle) throw new Error('No vault open')

    const { did, signingPublicKey, signingPrivateKey } = await generateIdentityAsync()

    const id = crypto.randomUUID()
    const newIdentity = {
      id,
      label,
      did,
      publicKey: signingPublicKey,
      privateKey: signingPrivateKey,
    }

    await currentVault.value.drizzle
      .insert(haexIdentities)
      .values(newIdentity)

    log.info(`Created identity "${label}" with DID ${did.slice(0, 30)}...`)

    await loadIdentitiesAsync()
    return identities.value.find(i => i.id === id)!
  }

  // ─── Contact methods (merged from contacts store) ─────────────────────

  const addContactAsync = async (label: string, publicKey: string, notes?: string): Promise<SelectHaexIdentities> => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    const existing = await db.select()
      .from(haexIdentities)
      .where(eq(haexIdentities.publicKey, publicKey))
      .limit(1)
    if (existing.length > 0) {
      throw new Error('An identity with this public key already exists')
    }

    const did = await publicKeyToDidKeyAsync(publicKey)
    const id = crypto.randomUUID()
    await db.insert(haexIdentities).values({ id, label, publicKey, did, notes })

    log.info(`Added contact "${label}" (${publicKey.slice(0, 16)}...)`)
    await loadIdentitiesAsync()
    return identities.value.find(i => i.id === id)!
  }

  const addContactWithClaimsAsync = async (
    label: string,
    publicKey: string,
    claims: { type: string; value: string }[],
    notes?: string,
  ): Promise<SelectHaexIdentities> => {
    const contact = await addContactAsync(label, publicKey, notes)

    for (const claim of claims) {
      await addClaimAsync(contact.id, claim.type, claim.value)
    }

    log.info(`Added contact "${label}" with ${claims.length} claims`)
    return contact
  }

  const updateContactAsync = async (id: string, updates: { label?: string; notes?: string; avatar?: string | null }) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    await db.update(haexIdentities)
      .set(updates)
      .where(eq(haexIdentities.id, id))

    log.info(`Updated contact ${id}`)
    await loadIdentitiesAsync()
  }

  const getContactByPublicKeyAsync = async (publicKey: string): Promise<SelectHaexIdentities | undefined> => {
    const db = currentVault.value?.drizzle
    if (!db) return undefined

    const rows = await db.select()
      .from(haexIdentities)
      .where(eq(haexIdentities.publicKey, publicKey))
      .limit(1)
    return rows[0]
  }

  // ─── Identity lookup / update ─────────────────────────────────────────

  /**
   * Returns spaces that would be affected by deleting an identity.
   * Admin spaces (where identity issued UCANs) will be fully deleted.
   * Member spaces will have this identity's devices removed.
   */
  const getAffectedSpacesAsync = async (identityId: string): Promise<{
    adminSpaces: SelectHaexSpaces[]
    memberSpaces: SelectHaexSpaces[]
  }> => {
    const db = currentVault.value?.drizzle
    if (!db) return { adminSpaces: [], memberSpaces: [] }

    const identity = await getIdentityByIdAsync(identityId)
    if (!identity) return { adminSpaces: [], memberSpaces: [] }

    // Spaces where this identity issued UCANs → admin
    const adminUcans = await db
      .select({ spaceId: haexUcanTokens.spaceId })
      .from(haexUcanTokens)
      .where(eq(haexUcanTokens.issuerDid, identity.did))
    const adminSpaceIds = [...new Set(adminUcans.map(u => u.spaceId))]

    const adminSpaces = adminSpaceIds.length > 0
      ? await db.select().from(haexSpaces)
          .where(and(
            inArray(haexSpaces.id, adminSpaceIds),
            // Never delete the vault space
            ne(haexSpaces.type, SpaceType.VAULT),
          ))
      : []

    // Spaces where this identity has devices but is not admin
    const deviceSpaces = await db
      .select({ spaceId: haexSpaceDevices.spaceId })
      .from(haexSpaceDevices)
      .where(eq(haexSpaceDevices.identityId, identityId))
    const memberSpaceIds = deviceSpaces
      .map(d => d.spaceId)
      .filter(id => !adminSpaceIds.includes(id))

    const memberSpaces = memberSpaceIds.length > 0
      ? await db.select().from(haexSpaces).where(inArray(haexSpaces.id, memberSpaceIds))
      : []

    return { adminSpaces, memberSpaces }
  }

  const deleteIdentityAsync = async (identityId: string) => {
    const db = currentVault.value?.drizzle
    if (!db) return

    const identity = await getIdentityByIdAsync(identityId)
    if (!identity) return

    const { adminSpaces, memberSpaces } = await getAffectedSpacesAsync(identityId)
    const spacesStore = useSpacesStore()

    // 1. Delete admin spaces and all their related data
    for (const space of adminSpaces) {
      // Stop leader mode for local spaces
      try { await invoke('local_delivery_stop', { spaceId: space.id }) } catch { /* may not be running */ }

      // Delete non-cascaded relations
      await db.delete(haexSpaceDevices).where(eq(haexSpaceDevices.spaceId, space.id))
      await db.delete(haexPeerShares).where(eq(haexPeerShares.spaceId, space.id))
      await db.delete(haexSharedSpaceSync).where(eq(haexSharedSpaceSync.spaceId, space.id))
      await db.delete(haexSyncBackends).where(eq(haexSyncBackends.spaceId, space.id))
      await db.delete(haexInviteOutbox).where(eq(haexInviteOutbox.spaceId, space.id))
      await db.delete(haexInviteTokens).where(eq(haexInviteTokens.spaceId, space.id))

      // Delete space (cascades to UCAN tokens, MLS keys, enrollments, pending invites)
      await spacesStore.removeSpaceFromDbAsync(space.id)
      log.info(`Cascade-deleted admin space "${space.name}" (${space.id})`)
    }

    // 2. Remove identity's devices from member spaces
    for (const space of memberSpaces) {
      await db.delete(haexSpaceDevices).where(
        and(
          eq(haexSpaceDevices.spaceId, space.id),
          eq(haexSpaceDevices.identityId, identityId),
        ),
      )
      log.info(`Removed identity from member space "${space.name}" (${space.id})`)
    }

    // 3. Delete the identity (claims cascade via FK)
    await db.delete(haexIdentities).where(eq(haexIdentities.id, identityId))

    log.info(`Deleted identity ${identity.publicKey.slice(0, 20)}... (${adminSpaces.length} admin spaces deleted, ${memberSpaces.length} member spaces cleaned)`)
    await loadIdentitiesAsync()
  }

  const getIdentityByIdAsync = async (id: string): Promise<SelectHaexIdentities | undefined> => {
    if (!currentVault.value?.drizzle) {
      // Fallback to in-memory identities (e.g. during pre-vault-open connect flow)
      return identities.value.find(i => i.id === id)
    }

    const rows = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .where(eq(haexIdentities.id, id))
      .limit(1)

    return rows[0]
  }

  const getIdentityByPublicKeyAsync = async (publicKey: string): Promise<SelectHaexIdentities | undefined> => {
    if (!currentVault.value?.drizzle) {
      return identities.value.find(i => i.publicKey === publicKey)
    }

    const rows = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .where(eq(haexIdentities.publicKey, publicKey))
      .limit(1)

    return rows[0]
  }

  const updateLabelAsync = async (identityId: string, label: string) => {
    if (!currentVault.value?.drizzle) return

    await currentVault.value.drizzle
      .update(haexIdentities)
      .set({ label })
      .where(eq(haexIdentities.id, identityId))

    log.info(`Updated identity ${identityId.slice(0, 8)}... label to "${label}"`)
    await loadIdentitiesAsync()
  }

  const updateAvatarAsync = async (identityId: string, avatar: string | null) => {
    if (!currentVault.value?.drizzle) return

    await currentVault.value.drizzle
      .update(haexIdentities)
      .set({ avatar })
      .where(eq(haexIdentities.id, identityId))

    await loadIdentitiesAsync()
  }

  const exportIdentity = (identity: SelectHaexIdentities): ExportedIdentity => ({
    did: identity.did,
    label: identity.label,
    publicKey: identity.publicKey,
    privateKey: identity.privateKey!,
  })

  const importIdentityAsync = async (exported: ExportedIdentity): Promise<SelectHaexIdentities> => {
    if (!currentVault.value?.drizzle) throw new Error('No vault open')

    if (!exported.publicKey || !exported.privateKey || !exported.did) {
      throw new Error('Invalid identity data: missing publicKey, privateKey, or did')
    }

    // Verify DID matches the public key
    const derivedDid = await publicKeyToDidKeyAsync(exported.publicKey)
    if (derivedDid !== exported.did) {
      throw new Error('DID does not match the public key — the identity data may be corrupted')
    }

    // Check if identity already exists (same publicKey = same identity)
    const existing = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .where(eq(haexIdentities.publicKey, exported.publicKey))
      .limit(1)
    if (existing.length > 0) {
      // Identity already exists — update private key if needed, return existing
      log.info(`Identity already exists, skipping import`)
      return existing[0]!
    }

    const id = crypto.randomUUID()
    const newIdentity = {
      id,
      label: exported.label || `Imported ${exported.did.slice(0, 20)}...`,
      did: exported.did,
      publicKey: exported.publicKey,
      privateKey: exported.privateKey,
      avatar: exported.avatar || null,
    }

    await currentVault.value.drizzle
      .insert(haexIdentities)
      .values(newIdentity)

    // Import claims if present
    if (exported.claims?.length) {
      for (const claim of exported.claims) {
        await addClaimAsync(id, claim.type, claim.value)
      }
      log.info(`Imported ${exported.claims.length} claims`)
    }

    log.info(`Imported identity "${newIdentity.label}" with DID ${exported.did.slice(0, 30)}...`)

    await loadIdentitiesAsync()
    return identities.value.find(i => i.id === id)!
  }

  // ─── Claims (now always use identity UUID) ────────────────────────────

  const addClaimAsync = async (identityId: string, type: string, value: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    // Verify identity exists in DB before inserting claim (FK constraint)
    const identity = await db.query.haexIdentities.findFirst({
      where: eq(haexIdentities.id, identityId),
    })
    if (!identity) {
      log.warn(`Cannot add claim "${type}": identity ${identityId.slice(0, 8)}... not in DB`)
      return null
    }

    // Prevent exact duplicates (same type + same value)
    const existing = await db.select({ id: haexIdentityClaims.id })
      .from(haexIdentityClaims)
      .where(and(
        eq(haexIdentityClaims.identityId, identityId),
        eq(haexIdentityClaims.type, type),
        eq(haexIdentityClaims.value, value),
      ))
      .limit(1)
    if (existing.length > 0) {
      log.info(`Claim "${type}: ${value}" already exists for identity ${identityId.slice(0, 8)}...`)
      return existing[0]
    }

    const id = crypto.randomUUID()
    await db.insert(haexIdentityClaims).values({ id, identityId, type, value })
    log.info(`Added claim "${type}" for identity ${identityId.slice(0, 8)}...`)
    return { id, identityId, type, value }
  }

  const getClaimsAsync = async (identityId: string) => {
    const db = currentVault.value?.drizzle
    if (!db) return []
    return db.select().from(haexIdentityClaims).where(eq(haexIdentityClaims.identityId, identityId))
  }

  const updateClaimAsync = async (claimId: string, value: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    await db.update(haexIdentityClaims).set({ value }).where(eq(haexIdentityClaims.id, claimId))
  }

  const deleteClaimAsync = async (claimId: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    await db.delete(haexIdentityClaims).where(eq(haexIdentityClaims.id, claimId))
  }

  const markClaimVerifiedAsync = async (claimId: string, serverUrl: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    await db.update(haexIdentityClaims).set({
      verifiedAt: new Date().toISOString(),
      verifiedBy: serverUrl,
    }).where(eq(haexIdentityClaims.id, claimId))
  }

  const ensureDefaultIdentityAsync = async () => {
    if (!currentVault.value?.drizzle) return

    const existing = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .limit(1)

    if (existing.length > 0) return

    const locale = useNuxtApp().$i18n.locale.value
    const label = locale === 'de' ? 'Meine Identität' : 'My Identity'

    const identity = await createIdentityAsync(label)
    const avatar = createAvatar(toonHead, { seed: identity.publicKey }).toDataUri()
    await updateAvatarAsync(identity.id, avatar)

    log.info('Default identity created')
  }

  const reset = () => {
    identities.value = []
    _identityPasswords.clear()
  }

  return {
    identities,
    ownIdentities,
    contacts,
    loadIdentitiesAsync,
    registerTemporaryIdentity,
    createIdentityAsync,
    deleteIdentityAsync,
    getIdentityByIdAsync,
    getIdentityByPublicKeyAsync,
    updateLabelAsync,
    updateAvatarAsync,
    exportIdentity,
    importIdentityAsync,
    addContactAsync,
    addContactWithClaimsAsync,
    updateContactAsync,
    getContactByPublicKeyAsync,
    addClaimAsync,
    getClaimsAsync,
    updateClaimAsync,
    deleteClaimAsync,
    markClaimVerifiedAsync,
    getAffectedSpacesAsync,
    ensureDefaultIdentityAsync,
    setIdentityPassword,
    consumeIdentityPassword,
    reset,
  }
})
