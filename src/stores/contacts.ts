import { eq, and } from 'drizzle-orm'
import { haexContacts, haexContactClaims, type SelectHaexContacts, type SelectHaexContactClaims } from '~/database/schemas'
import { createLogger } from '@/stores/logging'

export interface ContactWithClaims extends SelectHaexContacts {
  claims: SelectHaexContactClaims[]
}

const log = createLogger('CONTACTS')

export const useContactsStore = defineStore('contactsStore', () => {
  const { currentVault } = storeToRefs(useVaultStore())

  const contacts = ref<SelectHaexContacts[]>([])

  const loadContactsAsync = async () => {
    if (!currentVault.value?.drizzle) return
    contacts.value = await currentVault.value.drizzle
      .select()
      .from(haexContacts)
      .all()
    log.info(`Loaded ${contacts.value.length} contacts`)
  }

  const addContactAsync = async (label: string, publicKey: string, notes?: string): Promise<SelectHaexContacts> => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    const existing = await db.select()
      .from(haexContacts)
      .where(eq(haexContacts.publicKey, publicKey))
      .limit(1)
    if (existing.length > 0) {
      throw new Error('A contact with this public key already exists')
    }

    const id = crypto.randomUUID()
    await db.insert(haexContacts).values({ id, label, publicKey, notes })

    log.info(`Added contact "${label}" (${publicKey.slice(0, 16)}...)`)
    await loadContactsAsync()
    return contacts.value.find(c => c.id === id)!
  }

  const addContactWithClaimsAsync = async (
    label: string,
    publicKey: string,
    claims: { type: string; value: string }[],
    notes?: string,
  ): Promise<SelectHaexContacts> => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    const existing = await db.select()
      .from(haexContacts)
      .where(eq(haexContacts.publicKey, publicKey))
      .limit(1)
    if (existing.length > 0) {
      throw new Error('A contact with this public key already exists')
    }

    const contactId = crypto.randomUUID()
    await db.insert(haexContacts).values({ id: contactId, label, publicKey, notes })

    for (const claim of claims) {
      await db.insert(haexContactClaims).values({
        id: crypto.randomUUID(),
        contactId,
        type: claim.type,
        value: claim.value,
      })
    }

    log.info(`Added contact "${label}" with ${claims.length} claims`)
    await loadContactsAsync()
    return contacts.value.find(c => c.id === contactId)!
  }

  const updateContactAsync = async (id: string, updates: { label?: string; notes?: string; avatar?: string | null }) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    await db.update(haexContacts)
      .set(updates)
      .where(eq(haexContacts.id, id))

    log.info(`Updated contact ${id}`)
    await loadContactsAsync()
  }

  const deleteContactAsync = async (id: string) => {
    const db = currentVault.value?.drizzle
    if (!db) return

    await db.delete(haexContacts).where(eq(haexContacts.id, id))

    log.info(`Deleted contact ${id}`)
    await loadContactsAsync()
  }

  const getContactByPublicKeyAsync = async (publicKey: string): Promise<SelectHaexContacts | undefined> => {
    const db = currentVault.value?.drizzle
    if (!db) return undefined

    const rows = await db.select()
      .from(haexContacts)
      .where(eq(haexContacts.publicKey, publicKey))
      .limit(1)
    return rows[0]
  }

  // Claims management
  const getClaimsAsync = async (contactId: string): Promise<SelectHaexContactClaims[]> => {
    const db = currentVault.value?.drizzle
    if (!db) return []
    return db.select().from(haexContactClaims).where(eq(haexContactClaims.contactId, contactId))
  }

  const addClaimAsync = async (contactId: string, type: string, value: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')

    const existing = await db.select({ id: haexContactClaims.id })
      .from(haexContactClaims)
      .where(and(eq(haexContactClaims.contactId, contactId), eq(haexContactClaims.type, type)))
      .limit(1)
    if (existing.length > 0) {
      throw new Error(`Claim type "${type}" already exists for this contact`)
    }

    const id = crypto.randomUUID()
    await db.insert(haexContactClaims).values({ id, contactId, type, value })
    log.info(`Added claim "${type}" for contact ${contactId}`)
    return { id, contactId, type, value }
  }

  const updateClaimAsync = async (claimId: string, value: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    await db.update(haexContactClaims).set({ value }).where(eq(haexContactClaims.id, claimId))
  }

  const deleteClaimAsync = async (claimId: string) => {
    const db = currentVault.value?.drizzle
    if (!db) throw new Error('No vault open')
    await db.delete(haexContactClaims).where(eq(haexContactClaims.id, claimId))
  }

  const getContactWithClaimsAsync = async (id: string): Promise<ContactWithClaims | undefined> => {
    const db = currentVault.value?.drizzle
    if (!db) return undefined

    const rows = await db.select().from(haexContacts).where(eq(haexContacts.id, id)).limit(1)
    const contact = rows[0]
    if (!contact) return undefined

    const claims = await getClaimsAsync(id)
    return { ...contact, claims }
  }

  const reset = () => {
    contacts.value = []
  }

  return {
    contacts,
    loadContactsAsync,
    addContactAsync,
    addContactWithClaimsAsync,
    updateContactAsync,
    deleteContactAsync,
    getContactByPublicKeyAsync,
    getClaimsAsync,
    addClaimAsync,
    updateClaimAsync,
    deleteClaimAsync,
    getContactWithClaimsAsync,
    reset,
  }
})
