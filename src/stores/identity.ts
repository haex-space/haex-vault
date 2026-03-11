import { eq, and } from 'drizzle-orm'
import { generateIdentityAsync, publicKeyToDidKeyAsync } from '@haex-space/vault-sdk'
import { haexIdentities, haexIdentityClaims, type SelectHaexIdentities } from '~/database/schemas'
import { createLogger } from '@/stores/logging'

export interface ExportedIdentity {
  did: string
  label: string
  publicKey: string
  privateKey: string
}

const log = createLogger('IDENTITY')

export const useIdentityStore = defineStore('identityStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const identities = ref<SelectHaexIdentities[]>([])

  const loadIdentitiesAsync = async () => {
    if (!currentVault.value?.drizzle) return
    identities.value = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .all()
    log.info(`Loaded ${identities.value.length} identities`)
  }

  const createIdentityAsync = async (label: string): Promise<SelectHaexIdentities> => {
    if (!currentVault.value?.drizzle) throw new Error('No vault open')

    const { did, publicKeyBase64, privateKeyBase64 } = await generateIdentityAsync()

    const newIdentity = {
      id: crypto.randomUUID(),
      label,
      did,
      publicKey: publicKeyBase64,
      privateKey: privateKeyBase64,
    }

    await currentVault.value.drizzle
      .insert(haexIdentities)
      .values(newIdentity)

    log.info(`Created identity "${label}" with DID ${did.slice(0, 30)}...`)

    await loadIdentitiesAsync()
    return identities.value.find(i => i.id === newIdentity.id)!
  }

  const deleteIdentityAsync = async (id: string) => {
    if (!currentVault.value?.drizzle) return

    await currentVault.value.drizzle
      .delete(haexIdentities)
      .where(eq(haexIdentities.id, id))

    log.info(`Deleted identity ${id}`)
    await loadIdentitiesAsync()
  }

  const getIdentityAsync = async (id: string): Promise<SelectHaexIdentities | undefined> => {
    if (!currentVault.value?.drizzle) return undefined

    const rows = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .where(eq(haexIdentities.id, id))
      .limit(1)

    return rows[0]
  }

  const updateLabelAsync = async (id: string, label: string) => {
    if (!currentVault.value?.drizzle) return

    await currentVault.value.drizzle
      .update(haexIdentities)
      .set({ label })
      .where(eq(haexIdentities.id, id))

    log.info(`Updated identity ${id} label to "${label}"`)
    await loadIdentitiesAsync()
  }

  const exportIdentity = (identity: SelectHaexIdentities): ExportedIdentity => ({
    did: identity.did,
    label: identity.label,
    publicKey: identity.publicKey,
    privateKey: identity.privateKey,
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

    // Check for duplicate DID
    const existing = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .where(eq(haexIdentities.did, exported.did))
      .limit(1)
    if (existing.length > 0) {
      throw new Error('An identity with this DID already exists')
    }

    const newIdentity = {
      id: crypto.randomUUID(),
      label: exported.label || `Imported ${exported.did.slice(0, 20)}...`,
      did: exported.did,
      publicKey: exported.publicKey,
      privateKey: exported.privateKey,
    }

    await currentVault.value.drizzle
      .insert(haexIdentities)
      .values(newIdentity)

    log.info(`Imported identity "${newIdentity.label}" with DID ${exported.did.slice(0, 30)}...`)

    await loadIdentitiesAsync()
    return identities.value.find(i => i.id === newIdentity.id)!
  }

  const addClaimAsync = async (identityId: string, type: string, value: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    // Each claim type can only exist once per identity
    const existing = await db.select({ id: haexIdentityClaims.id })
      .from(haexIdentityClaims)
      .where(and(eq(haexIdentityClaims.identityId, identityId), eq(haexIdentityClaims.type, type)))
      .limit(1)
    if (existing.length > 0) {
      throw new Error(`Claim type "${type}" already exists for this identity`)
    }

    const id = crypto.randomUUID()
    await db.insert(haexIdentityClaims).values({ id, identityId, type, value })
    log.info(`Added claim "${type}" for identity ${identityId}`)
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

  return {
    identities,
    loadIdentitiesAsync,
    createIdentityAsync,
    deleteIdentityAsync,
    getIdentityAsync,
    updateLabelAsync,
    exportIdentity,
    importIdentityAsync,
    addClaimAsync,
    getClaimsAsync,
    updateClaimAsync,
    deleteClaimAsync,
    markClaimVerifiedAsync,
  }
})
