<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
  >
    <template #actions>
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-share-2"
        @click="showShareDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.share') }}</span>
      </UButton>
      <UButton
        color="primary"
        icon="i-lucide-plus"
        data-testid="contacts-add-trigger"
        @click="showAddDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.add') }}</span>
      </UButton>
    </template>

    <!-- Loading -->
    <div
      v-if="isLoading"
      class="flex items-center justify-center py-8"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-5 h-5 animate-spin text-primary"
      />
    </div>

    <!-- Contacts list -->
    <div
      v-else-if="contacts.length"
      class="space-y-3"
    >
      <ContactListItem
        v-for="contact in contacts"
        :key="contact.id"
        :contact="contact"
        :expanded="expandedContact === contact.id"
        :claims="contactClaims[contact.id] ?? []"
        @toggle="onToggleContact"
        @edit="openEditDialog"
        @delete="prepareDelete"
        @add-claim="openAddClaim"
        @edit-claim="openEditClaim"
        @delete-claim="deleteClaimAsync"
      />
    </div>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-else
      :message="t('list.empty')"
      icon="i-lucide-user"
    />

    <!-- Dialogs -->
    <AddContactDialog
      v-model:open="showAddDialog"
      @added="onContactChanged"
    />

    <EditContactDialog
      v-model:open="showEditDialog"
      :contact="editTarget"
      @saved="onContactChanged"
    />

    <ClaimDialog
      ref="claimDialogRef"
      v-model:open="showClaimDialog"
      :contact-id="claimTargetContactId"
      :editing-claim="editingClaim"
      @saved="loadClaimsAsync"
    />

    <UiDialogConfirm
      v-model:open="showDeleteConfirm"
      :title="t('delete.title')"
      :description="t('delete.description')"
      @confirm="onConfirmDeleteAsync"
    />

    <ShareIdentityDialog v-model:open="showShareDialog" />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'
import { createLogger } from '@/stores/logging'
import AddContactDialog from './contacts/AddContactDialog.vue'
import ClaimDialog from './contacts/ClaimDialog.vue'
import ContactListItem from './contacts/ContactListItem.vue'
import EditContactDialog from './contacts/EditContactDialog.vue'
import ShareIdentityDialog from './contacts/ShareIdentityDialog.vue'

const log = createLogger('CONTACTS')

const { t } = useI18n()
const { add: addToast } = useToast()

const identityStore = useIdentityStore()
const { contacts } = storeToRefs(identityStore)

const isLoading = ref(false)

// Dialog visibility
const showAddDialog = ref(false)
const showEditDialog = ref(false)
const showDeleteConfirm = ref(false)
const showShareDialog = ref(false)
const showClaimDialog = ref(false)

// Edit state
const editTarget = ref<SelectHaexIdentities | null>(null)
const deleteTarget = ref<SelectHaexIdentities | null>(null)

// Claims state
const expandedContact = ref<string | null>(null)
const contactClaims = ref<Record<string, { id: string; type: string; value: string }[]>>({})
const claimTargetContactId = ref<string | null>(null)
const editingClaim = ref<{ id: string; contactId: string; type: string } | null>(null)
const claimDialogRef = ref<InstanceType<typeof ClaimDialog> | null>(null)

onMounted(async () => {
  isLoading.value = true
  try {
    await identityStore.loadIdentitiesAsync()
  } finally {
    isLoading.value = false
  }
})

const onContactChanged = () => {
  // Contacts are reactive via store, nothing extra needed
}

// --- Edit ---
const openEditDialog = (contact: SelectHaexIdentities) => {
  editTarget.value = contact
  showEditDialog.value = true
}

// --- Delete ---
const prepareDelete = (contact: SelectHaexIdentities) => {
  deleteTarget.value = contact
  showDeleteConfirm.value = true
}

const onConfirmDeleteAsync = async () => {
  if (!deleteTarget.value) return

  log.info(`Deleting contact: "${deleteTarget.value.label}" (${deleteTarget.value.id})`)
  try {
    await identityStore.deleteIdentityAsync(deleteTarget.value.id)
    log.info('Contact deleted successfully')
    addToast({ title: t('success.deleted'), color: 'success' })
    showDeleteConfirm.value = false
    deleteTarget.value = null
  } catch (error) {
    log.error('Failed to delete contact', error)
    addToast({
      title: t('errors.deleteFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

// --- Claims ---
const onToggleContact = async (contactId: string, open: boolean) => {
  if (!open) {
    expandedContact.value = null
    return
  }
  expandedContact.value = contactId
  await loadClaimsAsync(contactId)
}

const loadClaimsAsync = async (contactId: string) => {
  const claims = await identityStore.getClaimsAsync(contactId)
  contactClaims.value[contactId] = claims.map(c => ({
    id: c.id,
    type: c.type,
    value: c.value,
  }))
}

const openAddClaim = (contactId: string) => {
  claimTargetContactId.value = contactId
  editingClaim.value = null
  showClaimDialog.value = true
}

const openEditClaim = (claim: { id: string; type: string; value: string }) => {
  editingClaim.value = {
    id: claim.id,
    contactId: expandedContact.value!,
    type: claim.type,
  }
  claimTargetContactId.value = expandedContact.value
  showClaimDialog.value = true
  nextTick(() => {
    claimDialogRef.value?.initEdit(claim.value)
  })
}

const deleteClaimAsync = async (claimId: string, contactId: string) => {
  log.info(`Deleting claim ${claimId} from contact ${contactId}`)
  try {
    await identityStore.deleteClaimAsync(claimId)
    await loadClaimsAsync(contactId)
    log.info('Claim deleted successfully')
    addToast({ title: t('claims.deleted'), color: 'success' })
  } catch (error) {
    log.error('Failed to delete claim', error)
    addToast({ title: t('claims.deleteFailed'), color: 'error' })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Kontakte
  description: Verwalte deine Kontakte und deren öffentliche Schlüssel
  list:
    empty: Keine Kontakte vorhanden
  delete:
    title: Kontakt löschen
    description: Möchtest du diesen Kontakt wirklich löschen? Dies kann nicht rückgängig gemacht werden.
  claims:
    deleted: Claim gelöscht
    deleteFailed: Claim konnte nicht gelöscht werden
  actions:
    add: Hinzufügen
    share: Teilen
  success:
    deleted: Kontakt gelöscht
  errors:
    deleteFailed: Löschen fehlgeschlagen
en:
  title: Contacts
  description: Manage your contacts and their public keys
  list:
    empty: No contacts found
  delete:
    title: Delete Contact
    description: Do you really want to delete this contact? This action cannot be undone.
  claims:
    deleted: Claim deleted
    deleteFailed: Failed to delete claim
  actions:
    add: Add
    share: Share
  success:
    deleted: Contact deleted
  errors:
    deleteFailed: Failed to delete contact
</i18n>
