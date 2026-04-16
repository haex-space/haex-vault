import { eq, ne, and, inArray, isNotNull, asc } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import {
  arrayBufferToBase64,
  publicKeyToDidKeyAsync,
  didKeyToPublicKeyAsync,
  importUserPrivateKeyAsync,
  importUserPublicKeyAsync,
  SIGNING_ALGO,
} from '@haex-space/vault-sdk'
import { buildDefaultAvatarSet } from '~/utils/identityAvatar'
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
  type SelectHaexIdentityClaims,
  type SelectHaexSpaces,
} from '~/database/schemas'
import { SpaceType } from '~/database/constants'
import { createLogger } from '@/stores/logging'
import { requireDb } from '~/stores/vault'

export interface ExportedIdentity {
  did: string
  name: string
  privateKey: string
  avatar?: string | null
  claims?: { type: string; value: string }[]
}

const log = createLogger('IDENTITY')

const generateSigningIdentityAsync = async (): Promise<{ did: string; signingPrivateKey: string }> => {
  const signing = await crypto.subtle.generateKey(
    SIGNING_ALGO,
    true,
    ['sign', 'verify'],
  ) as CryptoKeyPair

  const [publicKey, privateKey] = await Promise.all([
    crypto.subtle.exportKey('spki', signing.publicKey),
    crypto.subtle.exportKey('pkcs8', signing.privateKey),
  ])
  const signingPublicKey = arrayBufferToBase64(publicKey)

  return {
    did: await publicKeyToDidKeyAsync(signingPublicKey),
    signingPrivateKey: arrayBufferToBase64(privateKey),
  }
}

export const useIdentityStore = defineStore('identityStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const identities = ref<SelectHaexIdentities[]>([])

  // Computed views: own identities (have privateKey) vs contacts (no privateKey)
  const ownIdentities = computed(() => identities.value.filter(i => i.privateKey !== null))
  const contacts = computed(() =>
    identities.value.filter(i => i.privateKey === null && i.source === 'contact'),
  )

  // Reactive claims cache keyed by identityId. Populated on-demand by
  // `loadClaimsAsync`; invalidated (re-fetched) by the claim mutators below.
  // Consumers that want a reactive array should use `getClaimsForIdentity(id)`.
  const claimsByIdentity = ref<Record<string, SelectHaexIdentityClaims[]>>({})

  const getClaimsForIdentity = (identityId: string) =>
    computed(() => claimsByIdentity.value[identityId] ?? [])

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
    const db = requireDb()
    identities.value = await db
      .select()
      .from(haexIdentities)
      .orderBy(asc(haexIdentities.createdAt))
      .all()
    log.info(`Loaded ${identities.value.length} identities (${ownIdentities.value.length} own, ${contacts.value.length} contacts)`)

    // Backfill avatar + options for rows created before every create path
    // persisted both fields. We only touch rows where BOTH are missing so
    // user-uploaded avatars (stored in `avatar` with `avatarOptions = null`)
    // are never overwritten. The migrated avatar is a fresh random one,
    // not visually identical to the old `:seed=did` fallback — that's
    // the price of making the customizer hydrate cleanly.
    const toMigrate = identities.value.filter(
      (i) => !i.avatar && !i.avatarOptions,
    )
    if (toMigrate.length === 0) return

    for (const row of toMigrate) {
      const set = buildDefaultAvatarSet('toon-head')
      await db
        .update(haexIdentities)
        .set({ avatar: set.avatar, avatarOptions: set.avatarOptions })
        .where(eq(haexIdentities.id, row.id))
    }
    log.info(`Backfilled avatar + options for ${toMigrate.length} identity row(s)`)

    // Reload so the in-memory list reflects the backfilled fields.
    identities.value = await db.select().from(haexIdentities).all()
  }

  /** Register a temporary identity in-memory (e.g. from server recovery before vault is open) */
  const registerTemporaryIdentity = (identity: { id: string; privateKey: string; did: string; name: string }) => {
    if (identities.value.find(i => i.id === identity.id)) return
    identities.value.push({
      id: identity.id,
      privateKey: identity.privateKey,
      did: identity.did,
      name: identity.name,
      source: 'own',
      avatarOptions: null,
      avatar: null,
      notes: null,
      createdAt: new Date().toISOString(),
    } as SelectHaexIdentities)
  }

  const createIdentityAsync = async (name: string): Promise<SelectHaexIdentities> => {
    const db = requireDb()

    const { did, signingPrivateKey } = await generateSigningIdentityAsync()

    // Always persist an avatar + its options together; otherwise the
    // list view (seed-only render) and the edit dialog (options-based
    // customizer) render different avatars for the same row.
    const avatarSet = buildDefaultAvatarSet('toon-head')

    const id = crypto.randomUUID()
    const newIdentity = {
      id,
      name,
      did,
      source: 'contact',
      privateKey: signingPrivateKey,
      avatar: avatarSet.avatar,
      notes: null,
      avatarOptions: avatarSet.avatarOptions,
      createdAt: new Date().toISOString(),
    }

    await db
      .insert(haexIdentities)
      .values(newIdentity)

    log.info(`Created identity "${name}" with DID ${did.slice(0, 30)}...`)

    await loadIdentitiesAsync()
    return newIdentity
  }

  // ─── Contact methods (merged from contacts store) ─────────────────────

  const addContactAsync = async (name: string, publicKey: string, notes?: string): Promise<SelectHaexIdentities> => {
    const db = requireDb()
    const did = await publicKeyToDidKeyAsync(publicKey)
    const existing = await db.select()
      .from(haexIdentities)
      .where(eq(haexIdentities.did, did))
      .limit(1)
    if (existing.length > 0) {
      throw new Error('An identity with this public key already exists')
    }

    // Seed a random avatar so the list view and any future edit flow
    // start from a consistent, editable state.
    const avatarSet = buildDefaultAvatarSet('toon-head')

    const id = crypto.randomUUID()
    await db.insert(haexIdentities).values({
      id,
      name,
      did,
      source: 'contact',
      notes,
      avatar: avatarSet.avatar,
      avatarOptions: avatarSet.avatarOptions,
    })

    log.info(`Added contact "${name}" (${did.slice(0, 24)}...)`)
    await loadIdentitiesAsync()
    const result = identities.value.find(i => i.id === id)
    if (!result) throw new Error(`Failed to find newly created contact ${id}`)
    return result
  }

  const addContactWithClaimsAsync = async (
    name: string,
    publicKey: string,
    claims: { type: string; value: string }[],
    notes?: string,
  ): Promise<SelectHaexIdentities> => {
    const contact = await addContactAsync(name, publicKey, notes)

    for (const claim of claims) {
      await addClaimAsync(contact.id, claim.type, claim.value)
    }

    log.info(`Added contact "${name}" with ${claims.length} claims`)
    return contact
  }

  const updateContactAsync = async (id: string, updates: { name?: string; notes?: string; avatar?: string | null }) => {
    const db = requireDb()

    await db.update(haexIdentities)
      .set(updates)
      .where(eq(haexIdentities.id, id))

    log.info(`Updated contact ${id}`)
    await loadIdentitiesAsync()
  }

  const getContactByPublicKeyAsync = async (publicKey: string): Promise<SelectHaexIdentities | undefined> => {
    const did = await publicKeyToDidKeyAsync(publicKey)
    return getIdentityByDidAsync(did)
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
    const db = requireDb()

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
    const db = requireDb()

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

    log.info(`Deleted identity ${identity.did.slice(0, 20)}... (${adminSpaces.length} admin spaces deleted, ${memberSpaces.length} member spaces cleaned)`)
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

  const getIdentityByDidAsync = async (did: string): Promise<SelectHaexIdentities | undefined> => {
    if (!currentVault.value?.drizzle) {
      return identities.value.find(i => i.did === did)
    }

    const rows = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .where(eq(haexIdentities.did, did))
      .limit(1)

    return rows[0]
  }

  const getIdentityByPublicKeyAsync = async (publicKey: string): Promise<SelectHaexIdentities | undefined> => {
    const did = await publicKeyToDidKeyAsync(publicKey)
    return getIdentityByDidAsync(did)
  }

  const updateNameAsync = async (identityId: string, name: string) => {
    const db = requireDb()

    await db
      .update(haexIdentities)
      .set({ name })
      .where(eq(haexIdentities.id, identityId))

    log.info(`Updated identity ${identityId.slice(0, 8)}... name to "${name}"`)
    await loadIdentitiesAsync()
  }

  const updateAvatarAsync = async (identityId: string, avatar: string | null, avatarOptions?: string | null) => {
    const db = requireDb()

    await db
      .update(haexIdentities)
      .set({ avatar, ...(avatarOptions !== undefined ? { avatarOptions } : {}) })
      .where(eq(haexIdentities.id, identityId))

    await loadIdentitiesAsync()
  }

  const ensureIdentityForDidAsync = async (
    did: string,
    options?: { name?: string | null; avatar?: string | null; avatarOptions?: string | null; source?: 'space' | 'contact' },
  ): Promise<SelectHaexIdentities> => {
    const db = requireDb()
    const existing = await getIdentityByDidAsync(did)
    if (existing) {
      if (options?.source === 'contact' && existing.source !== 'contact') {
        await db.update(haexIdentities)
          .set({ source: 'contact' })
          .where(eq(haexIdentities.id, existing.id))
        await loadIdentitiesAsync()
        const updated = await getIdentityByIdAsync(existing.id)
        if (!updated) throw new Error(`Identity ${existing.id} disappeared after update`)
        return updated
      }
      return existing
    }

    // Backfill an avatar/options set when the caller didn't bring its own
    // so the row is immediately renderable AND editable in the customizer.
    const needsAvatar = !options?.avatar && !options?.avatarOptions
    const avatarSet = needsAvatar ? buildDefaultAvatarSet('toon-head') : null

    const id = crypto.randomUUID()
    const newIdentity = {
      id,
      did,
      name: options?.name || did.slice(0, 24),
      source: options?.source || 'space',
      privateKey: null,
      avatar: options?.avatar ?? avatarSet?.avatar ?? null,
      avatarOptions: options?.avatarOptions ?? avatarSet?.avatarOptions ?? null,
      notes: null,
      createdAt: new Date().toISOString(),
    } satisfies SelectHaexIdentities

    await db.insert(haexIdentities).values(newIdentity)
    await loadIdentitiesAsync()
    const result = identities.value.find(i => i.id === id)
    if (!result) throw new Error(`Failed to find newly created identity ${id}`)
    return result
  }

  const exportIdentity = (identity: SelectHaexIdentities): ExportedIdentity => {
    if (!identity.privateKey) throw new Error(`Cannot export identity ${identity.id}: no private key`)
    return {
      did: identity.did,
      name: identity.name,
      privateKey: identity.privateKey,
    }
  }

  const importIdentityAsync = async (exported: ExportedIdentity): Promise<SelectHaexIdentities> => {
    const db = requireDb()

    if (!exported.privateKey || !exported.did) {
      throw new Error('Invalid identity data: missing privateKey or did')
    }

    // Verify that the private key corresponds to the claimed DID
    const challenge = crypto.getRandomValues(new Uint8Array(32))
    const privateKey = await importUserPrivateKeyAsync(exported.privateKey)
    const signature = await crypto.subtle.sign('Ed25519', privateKey, challenge)
    const publicKeyBase64 = await didKeyToPublicKeyAsync(exported.did)
    const publicKey = await importUserPublicKeyAsync(publicKeyBase64)
    const isValid = await crypto.subtle.verify('Ed25519', publicKey, signature, challenge)
    if (!isValid) {
      throw new Error('Private key does not match the claimed DID')
    }

    // Check if identity already exists (same DID = same identity)
    const existing = await db
      .select()
      .from(haexIdentities)
      .where(eq(haexIdentities.did, exported.did))
      .limit(1)
    if (existing.length > 0) {
      log.info(`Identity already exists, skipping import`)
      return existing[0]!
    }

    // Imported identities without a bundled avatar get a fresh random
    // one so the customizer can hydrate it cleanly on first edit.
    const avatarSet = exported.avatar
      ? null
      : buildDefaultAvatarSet('toon-head')

    const id = crypto.randomUUID()
    const newIdentity = {
      id,
      name: exported.name || `Imported ${exported.did.slice(0, 20)}...`,
      did: exported.did,
      source: 'own',
      privateKey: exported.privateKey,
      avatar: exported.avatar || avatarSet?.avatar || null,
      avatarOptions: avatarSet?.avatarOptions ?? null,
      notes: null,
    }

    await db
      .insert(haexIdentities)
      .values(newIdentity)

    // Import claims if present
    if (exported.claims?.length) {
      for (const claim of exported.claims) {
        await addClaimAsync(id, claim.type, claim.value)
      }
      log.info(`Imported ${exported.claims.length} claims`)
    }

    log.info(`Imported identity "${newIdentity.name}" with DID ${exported.did.slice(0, 30)}...`)

    await loadIdentitiesAsync()
    const result = identities.value.find(i => i.id === id)
    if (!result) throw new Error(`Failed to find imported identity ${id}`)
    return result
  }

  // ─── Claims (now always use identity UUID) ────────────────────────────

  const addClaimAsync = async (identityId: string, type: string, value: string) => {
    const db = requireDb()

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

    // Refresh cache if the identity's claims were previously loaded.
    if (identityId in claimsByIdentity.value) {
      await loadClaimsAsync(identityId)
    }

    return { id, identityId, type, value }
  }

  const getClaimsAsync = async (identityId: string) => {
    const db = requireDb()
    return db.select().from(haexIdentityClaims).where(eq(haexIdentityClaims.identityId, identityId))
  }

  /**
   * Loads claims for the given identity from the DB and populates the
   * reactive cache. Subsequent reads via `getClaimsForIdentity(id)` see the
   * cached result without re-hitting the DB.
   */
  const loadClaimsAsync = async (identityId: string) => {
    const claims = await getClaimsAsync(identityId)
    claimsByIdentity.value[identityId] = claims
    return claims
  }

  const updateClaimAsync = async (claimId: string, value: string) => {
    const db = requireDb()
    await db.update(haexIdentityClaims).set({ value }).where(eq(haexIdentityClaims.id, claimId))

    // Invalidate cache for any identity whose cached list contains this claim.
    for (const [identityId, claims] of Object.entries(claimsByIdentity.value)) {
      if (claims.some((c) => c.id === claimId)) {
        await loadClaimsAsync(identityId)
      }
    }
  }

  const deleteClaimAsync = async (claimId: string) => {
    const db = requireDb()
    await db.delete(haexIdentityClaims).where(eq(haexIdentityClaims.id, claimId))

    for (const [identityId, claims] of Object.entries(claimsByIdentity.value)) {
      if (claims.some((c) => c.id === claimId)) {
        await loadClaimsAsync(identityId)
      }
    }
  }

  const markClaimVerifiedAsync = async (claimId: string, serverUrl: string) => {
    const db = requireDb()
    await db.update(haexIdentityClaims).set({
      verifiedAt: new Date().toISOString(),
      verifiedBy: serverUrl,
    }).where(eq(haexIdentityClaims.id, claimId))
  }

  const ensureDefaultIdentityAsync = async () => {
    const db = requireDb()

    const existing = await db
      .select()
      .from(haexIdentities)
      .where(isNotNull(haexIdentities.privateKey))
      .limit(1)

    const locale = useNuxtApp().$i18n.locale.value
    const name = locale.startsWith('de') ? 'Meine Identität' : 'My Identity'

    if (existing.length > 0) {
      const identity = existing[0]!
      // Only touch the name; avatar + options were either set at create
      // time or backfilled by loadIdentitiesAsync — re-generating here
      // would produce exactly the "avatar jumps on edit" bug we fixed.
      if (!identity.name?.trim()) {
        await db.update(haexIdentities)
          .set({ name })
          .where(eq(haexIdentities.id, identity.id))
        await loadIdentitiesAsync()
      }
      return
    }

    await createIdentityAsync(name)
    log.info('Default identity created')
  }

  const reset = () => {
    identities.value = []
    claimsByIdentity.value = {}
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
    getIdentityByDidAsync,
    getIdentityByPublicKeyAsync,
    ensureIdentityForDidAsync,
    updateNameAsync,
    updateAvatarAsync,
    exportIdentity,
    importIdentityAsync,
    addContactAsync,
    addContactWithClaimsAsync,
    updateContactAsync,
    getContactByPublicKeyAsync,
    claimsByIdentity,
    getClaimsForIdentity,
    loadClaimsAsync,
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
