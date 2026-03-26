import { eq } from 'drizzle-orm'
import { generateIdentityAsync, publicKeyToDidKeyAsync } from '@haex-space/vault-sdk'
import { haexIdentities, haexIdentityClaims, type SelectHaexIdentities } from '~/database/schemas'
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

  // Session-only: identity passwords set during creation, consumed on first backend registration
  const _identityPasswords = new Map<string, string>()

  const setIdentityPassword = (publicKey: string, password: string) => {
    _identityPasswords.set(publicKey, password)
  }

  const consumeIdentityPassword = (publicKey: string): string | undefined => {
    const pw = _identityPasswords.get(publicKey)
    _identityPasswords.delete(publicKey)
    return pw
  }

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
    return identities.value.find(i => i.publicKey === newIdentity.publicKey)!
  }

  const deleteIdentityAsync = async (publicKey: string) => {
    if (!currentVault.value?.drizzle) return

    await currentVault.value.drizzle
      .delete(haexIdentities)
      .where(eq(haexIdentities.publicKey, publicKey))

    log.info(`Deleted identity ${publicKey.slice(0, 20)}...`)
    await loadIdentitiesAsync()
  }

  const getIdentityAsync = async (publicKey: string): Promise<SelectHaexIdentities | undefined> => {
    if (!currentVault.value?.drizzle) return undefined

    const rows = await currentVault.value.drizzle
      .select()
      .from(haexIdentities)
      .where(eq(haexIdentities.publicKey, publicKey))
      .limit(1)

    return rows[0]
  }

  const updateLabelAsync = async (publicKey: string, label: string) => {
    if (!currentVault.value?.drizzle) return

    await currentVault.value.drizzle
      .update(haexIdentities)
      .set({ label })
      .where(eq(haexIdentities.publicKey, publicKey))

    log.info(`Updated identity ${publicKey.slice(0, 20)}... label to "${label}"`)
    await loadIdentitiesAsync()
  }

  const updateAvatarAsync = async (publicKey: string, avatar: string | null) => {
    if (!currentVault.value?.drizzle) return

    await currentVault.value.drizzle
      .update(haexIdentities)
      .set({ avatar })
      .where(eq(haexIdentities.publicKey, publicKey))

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

    const newIdentity = {
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
        await addClaimAsync(exported.publicKey, claim.type, claim.value)
      }
      log.info(`Imported ${exported.claims.length} claims`)
    }

    log.info(`Imported identity "${newIdentity.label}" with DID ${exported.did.slice(0, 30)}...`)

    await loadIdentitiesAsync()
    return identities.value.find(i => i.publicKey === newIdentity.publicKey)!
  }

  const addClaimAsync = async (identityPublicKey: string, type: string, value: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    // Verify identity exists in DB before inserting claim (FK constraint)
    const identity = await db.query.haexIdentities.findFirst({
      where: eq(haexIdentities.publicKey, identityPublicKey),
    })
    if (!identity) {
      log.warn(`Cannot add claim "${type}": identity ${identityPublicKey.slice(0, 20)}... not in DB`)
      return null
    }

    const id = crypto.randomUUID()
    await db.insert(haexIdentityClaims).values({ id, identityId: identityPublicKey, type, value })
    log.info(`Added claim "${type}" for identity ${identityPublicKey.slice(0, 20)}...`)
    return { id, identityId: identityPublicKey, type, value }
  }

  const getClaimsAsync = async (identityPublicKey: string) => {
    const db = currentVault.value?.drizzle
    if (!db) return []
    return db.select().from(haexIdentityClaims).where(eq(haexIdentityClaims.identityId, identityPublicKey))
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
    updateAvatarAsync,
    exportIdentity,
    importIdentityAsync,
    addClaimAsync,
    getClaimsAsync,
    updateClaimAsync,
    deleteClaimAsync,
    markClaimVerifiedAsync,
    setIdentityPassword,
    consumeIdentityPassword,
  }
})
