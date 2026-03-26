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
        color="neutral"
        variant="outline"
        icon="i-lucide-scan-line"
        @click="showScanDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.scan') }}</span>
      </UButton>
      <UButton
        color="primary"
        icon="i-lucide-plus"
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
      <div
        v-for="contact in contacts"
        :key="contact.id"
        class="p-3 rounded-lg border border-default"
      >
        <UCollapsible
          :open="expandedContact === contact.id"
          :unmount-on-hide="false"
          @update:open="(val: boolean) => onToggleContact(contact.id, val)"
        >
          <div class="flex items-center justify-between cursor-pointer">
            <div class="flex-1 min-w-0 flex items-center gap-2">
              <UIcon
                name="i-lucide-chevron-right"
                class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
                :class="{ 'rotate-90': expandedContact === contact.id }"
              />
              <UiAvatar
                :src="contact.avatar"
                :seed="contact.id"
                size="sm"
              />
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-medium truncate">{{ contact.label }}</span>
                </div>
                <div class="mt-1 flex items-center gap-2">
                  <code class="text-xs text-muted truncate max-w-[300px]">{{
                    contact.publicKey
                  }}</code>
                </div>
              </div>
            </div>

            <div
              class="shrink-0 ml-4"
              @click.stop
            >
              <!-- Large screens: inline buttons -->
              <div class="hidden @md:flex items-center gap-1">
                <UButton
                  variant="ghost"
                  icon="i-lucide-copy"
                  :title="t('actions.copyKey')"
                  @click="copyPublicKey(contact.publicKey)"
                />
                <UButton
                  variant="ghost"
                  icon="i-lucide-pencil"
                  :title="t('actions.edit')"
                  @click="openEditDialog(contact)"
                />
                <UButton
                  variant="ghost"
                  color="error"
                  icon="i-lucide-trash-2"
                  :title="t('actions.delete')"
                  @click="prepareDelete(contact)"
                />
              </div>
              <!-- Small screens: dropdown menu -->
              <UDropdownMenu
                class="@md:hidden"
                :items="[
                  [
                    {
                      label: t('actions.copyKey'),
                      icon: 'i-lucide-copy',
                      onSelect: () => copyPublicKey(contact.publicKey),
                    },
                    {
                      label: t('actions.edit'),
                      icon: 'i-lucide-pencil',
                      onSelect: () => openEditDialog(contact),
                    },
                  ],
                  [
                    {
                      label: t('actions.delete'),
                      icon: 'i-lucide-trash-2',
                      color: 'error' as const,
                      onSelect: () => prepareDelete(contact),
                    },
                  ],
                ]"
              >
                <UButton
                  variant="ghost"
                  icon="i-lucide-ellipsis-vertical"
                  color="neutral"
                />
              </UDropdownMenu>
            </div>
          </div>

          <!-- Claims Section (collapsible) -->
          <template
            v-if="expandedContact === contact.id"
            #content
          >
            <div class="mt-3 pt-3 border-t border-default space-y-2">
              <div class="flex items-center justify-between">
                <span class="text-sm font-medium">{{ t('claims.title') }}</span>
                <UButton
                  variant="outline"
                  icon="i-lucide-plus"
                  @click="openAddClaim(contact.id)"
                >
                  {{ t('claims.add') }}
                </UButton>
              </div>

              <div
                v-if="contactClaims[contact.id]?.length"
                class="space-y-1"
              >
                <div
                  v-for="claim in contactClaims[contact.id]"
                  :key="claim.id"
                  class="flex items-center justify-between p-2 rounded bg-gray-50 dark:bg-gray-800/50"
                >
                  <div class="min-w-0 flex-1">
                    <span class="text-xs font-medium text-muted">{{
                      claim.type
                    }}</span>
                    <p class="text-sm truncate">{{ claim.value }}</p>
                  </div>
                  <div class="flex gap-1 shrink-0 ml-2">
                    <UButton
                      variant="ghost"
                      icon="i-lucide-pencil"
                      @click="openEditClaim(claim)"
                    />
                    <UButton
                      variant="ghost"
                      color="error"
                      icon="i-lucide-trash-2"
                      @click="deleteClaimAsync(claim.id, contact.id)"
                    />
                  </div>
                </div>
              </div>
              <p
                v-else
                class="text-xs text-muted"
              >
                {{ t('claims.empty') }}
              </p>

              <!-- Notes -->
              <div
                v-if="contact.notes"
                class="pt-2"
              >
                <span class="text-xs font-medium text-muted">{{
                  t('fields.notes')
                }}</span>
                <p class="text-sm text-muted">{{ contact.notes }}</p>
              </div>
            </div>
          </template>
        </UCollapsible>
      </div>
    </div>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-else
      :message="t('list.empty')"
      icon="i-lucide-user"
    />
    <!-- Add Contact Dialog -->
    <UiDrawerModal
      v-model:open="showAddDialog"
      :title="t('add.title')"
      :description="t('add.description')"
    >
      <template #content>
        <UiInput
          v-model="addForm.label"
          :label="t('fields.label')"
          :placeholder="t('add.labelPlaceholder')"
        />
        <UiTextarea
          v-model="addForm.publicKey"
          :label="t('fields.publicKey')"
          :placeholder="t('add.publicKeyPlaceholder')"
          :rows="3"
        />
        <UiTextarea
          v-model="addForm.notes"
          :label="t('fields.notes')"
          :placeholder="t('add.notesPlaceholder')"
          :rows="2"
        />
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showAddDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-plus"
            :loading="isAdding"
            :disabled="!addForm.label.trim() || !addForm.publicKey.trim()"
            @click="onAddContactAsync"
          >
            {{ t('actions.add') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Edit Contact Dialog -->
    <UiDrawerModal
      v-model:open="showEditDialog"
      :title="t('edit.title')"
    >
      <template #content>
        <UiInput
          v-model="editForm.label"
          :label="t('fields.label')"
        />
        <UiTextarea
          v-model="editForm.notes"
          :label="t('fields.notes')"
          :rows="2"
        />
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showEditDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-check"
            :loading="isEditing"
            :disabled="!editForm.label.trim()"
            @click="onEditContactAsync"
          >
            {{ t('actions.save') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Add/Edit Claim Dialog -->
    <UiDrawerModal
      v-model:open="showClaimDialog"
      :title="editingClaim ? t('claims.editTitle') : t('claims.addTitle')"
    >
      <template #content>
        <div class="space-y-4">
          <USelect
            v-if="!editingClaim"
            v-model="claimType"
            class="min-w-48"
            :items="claimTypeOptions"
            value-key="value"
            :label="t('claims.type')"
          />
          <UiInput
            v-if="claimType === 'custom' && !editingClaim"
            v-model="claimCustomType"
            :label="t('claims.customType')"
            placeholder="z.B. phone, company"
          />
          <UiInput
            v-model="claimValue"
            :label="t('claims.value')"
            :placeholder="claimValuePlaceholder"
            @keydown.enter.prevent="onSaveClaimAsync"
          />
        </div>
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showClaimDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-check"
            :disabled="!canSaveClaim"
            @click="onSaveClaimAsync"
          >
            {{ t('actions.save') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Delete Confirmation -->
    <UiDialogConfirm
      v-model:open="showDeleteConfirm"
      :title="t('delete.title')"
      :description="t('delete.description')"
      @confirm="onConfirmDeleteAsync"
    />

    <!-- Share Identity QR Dialog -->
    <ShareIdentityDialog v-model:open="showShareDialog" />

    <!-- Scan Contact QR Dialog -->
    <ScanContactDialog v-model:open="showScanDialog" />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { SelectHaexContacts } from '~/database/schemas'
import ShareIdentityDialog from './contacts/ShareIdentityDialog.vue'
import ScanContactDialog from './contacts/ScanContactDialog.vue'

const { t } = useI18n()
const { add } = useToast()

const contactsStore = useContactsStore()
const { contacts } = storeToRefs(contactsStore)

const isLoading = ref(false)
const isAdding = ref(false)
const isEditing = ref(false)

const showAddDialog = ref(false)
const showEditDialog = ref(false)
const showDeleteConfirm = ref(false)
const showShareDialog = ref(false)
const showScanDialog = ref(false)

const addForm = reactive({
  label: '',
  publicKey: '',
  notes: '',
})

const editForm = reactive({
  id: '',
  label: '',
  notes: '',
})

const deleteTarget = ref<SelectHaexContacts | null>(null)

onMounted(async () => {
  isLoading.value = true
  try {
    await contactsStore.loadContactsAsync()
  } finally {
    isLoading.value = false
  }
})

const onAddContactAsync = async () => {
  if (!addForm.label.trim() || !addForm.publicKey.trim()) return

  isAdding.value = true
  try {
    await contactsStore.addContactAsync(
      addForm.label.trim(),
      addForm.publicKey.trim(),
      addForm.notes.trim() || undefined,
    )
    add({ title: t('success.added'), color: 'success' })
    showAddDialog.value = false
    addForm.label = ''
    addForm.publicKey = ''
    addForm.notes = ''
  } catch (error) {
    console.error('Failed to add contact:', error)
    add({
      title: t('errors.addFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isAdding.value = false
  }
}

const openEditDialog = (contact: SelectHaexContacts) => {
  editForm.id = contact.id
  editForm.label = contact.label
  editForm.notes = contact.notes ?? ''
  showEditDialog.value = true
}

const onEditContactAsync = async () => {
  if (!editForm.label.trim()) return

  isEditing.value = true
  try {
    await contactsStore.updateContactAsync(editForm.id, {
      label: editForm.label.trim(),
      notes: editForm.notes.trim() || undefined,
    })
    add({ title: t('success.updated'), color: 'success' })
    showEditDialog.value = false
  } catch (error) {
    console.error('Failed to update contact:', error)
    add({
      title: t('errors.updateFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isEditing.value = false
  }
}

const prepareDelete = (contact: SelectHaexContacts) => {
  deleteTarget.value = contact
  showDeleteConfirm.value = true
}

const onConfirmDeleteAsync = async () => {
  if (!deleteTarget.value) return

  try {
    await contactsStore.deleteContactAsync(deleteTarget.value.id)
    add({ title: t('success.deleted'), color: 'success' })
    showDeleteConfirm.value = false
    deleteTarget.value = null
  } catch (error) {
    console.error('Failed to delete contact:', error)
    add({
      title: t('errors.deleteFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const copyPublicKey = async (key: string) => {
  try {
    await navigator.clipboard.writeText(key)
    add({ title: t('success.copied'), color: 'success' })
  } catch {
    add({ title: t('errors.copyFailed'), color: 'error' })
  }
}

// Claims management
const expandedContact = ref<string | null>(null)
const contactClaims = ref<
  Record<string, { id: string; type: string; value: string }[]>
>({})
const showClaimDialog = ref(false)
const claimType = ref('email')
const claimCustomType = ref('')
const claimValue = ref('')
const editingClaim = ref<{
  id: string
  contactId: string
  type: string
} | null>(null)
const claimTargetContactId = ref<string | null>(null)

const claimTypeOptions = computed(() => {
  const existingTypes = new Set(
    (claimTargetContactId.value
      ? contactClaims.value[claimTargetContactId.value]
      : []
    )?.map((c) => c.type) ?? [],
  )
  return [
    { label: 'Email', value: 'email', disabled: existingTypes.has('email') },
    { label: 'Name', value: 'name', disabled: existingTypes.has('name') },
    { label: t('claims.custom'), value: 'custom' },
  ]
})

const claimValuePlaceholder = computed(() => {
  if (editingClaim.value) return ''
  if (claimType.value === 'email') return 'user@example.com'
  if (claimType.value === 'name') return 'Max Mustermann'
  return ''
})

const canSaveClaim = computed(() => {
  if (!claimValue.value.trim()) return false
  if (
    !editingClaim.value &&
    claimType.value === 'custom' &&
    !claimCustomType.value.trim()
  )
    return false
  return true
})

const onToggleContact = async (contactId: string, open: boolean) => {
  if (!open) {
    expandedContact.value = null
    return
  }
  expandedContact.value = contactId
  await loadClaimsAsync(contactId)
}

const loadClaimsAsync = async (contactId: string) => {
  const claims = await contactsStore.getClaimsAsync(contactId)
  contactClaims.value[contactId] = claims.map((c) => ({
    id: c.id,
    type: c.type,
    value: c.value,
  }))
}

const openAddClaim = (contactId: string) => {
  claimTargetContactId.value = contactId
  editingClaim.value = null
  const firstAvailable = claimTypeOptions.value.find((o) => !o.disabled)
  claimType.value = firstAvailable?.value ?? 'custom'
  claimCustomType.value = ''
  claimValue.value = ''
  showClaimDialog.value = true
}

const openEditClaim = (claim: { id: string; type: string; value: string }) => {
  editingClaim.value = {
    id: claim.id,
    contactId: expandedContact.value!,
    type: claim.type,
  }
  claimValue.value = claim.value
  showClaimDialog.value = true
}

const onSaveClaimAsync = async () => {
  if (!canSaveClaim.value) return

  try {
    if (editingClaim.value) {
      await contactsStore.updateClaimAsync(
        editingClaim.value.id,
        claimValue.value.trim(),
      )
      await loadClaimsAsync(editingClaim.value.contactId)
      add({ title: t('claims.updated'), color: 'success' })
    } else {
      const type =
        claimType.value === 'custom'
          ? claimCustomType.value.trim()
          : claimType.value
      await contactsStore.addClaimAsync(
        claimTargetContactId.value!,
        type,
        claimValue.value.trim(),
      )
      await loadClaimsAsync(claimTargetContactId.value!)
      add({ title: t('claims.added'), color: 'success' })
    }
    showClaimDialog.value = false
  } catch (error) {
    console.error('Failed to save claim:', error)
    add({
      title: t('claims.saveFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const deleteClaimAsync = async (claimId: string, contactId: string) => {
  try {
    await contactsStore.deleteClaimAsync(claimId)
    await loadClaimsAsync(contactId)
    add({ title: t('claims.deleted'), color: 'success' })
  } catch (error) {
    console.error('Failed to delete claim:', error)
    add({ title: t('claims.deleteFailed'), color: 'error' })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Kontakte
  description: Verwalte deine Kontakte und deren öffentliche Schlüssel
  list:
    title: Deine Kontakte
    description: Kontakte für die Zusammenarbeit in Spaces
    empty: Keine Kontakte vorhanden
    added: Hinzugefügt
  add:
    title: Kontakt hinzufügen
    description: Füge einen neuen Kontakt mit seinem öffentlichen Schlüssel hinzu
    labelPlaceholder: z.B. Alice, Bob, Team-Lead
    publicKeyPlaceholder: Base58-kodierten Public Key einfügen
    notesPlaceholder: Optionale Notizen
  edit:
    title: Kontakt bearbeiten
  delete:
    title: Kontakt löschen
    description: Möchtest du diesen Kontakt wirklich löschen? Dies kann nicht rückgängig gemacht werden.
  fields:
    label: Name
    publicKey: Public Key
    notes: Notizen
  claims:
    title: Claims
    add: Hinzufügen
    addTitle: Claim hinzufügen
    editTitle: Claim bearbeiten
    type: Typ
    customType: Benutzerdefinierter Typ
    custom: Benutzerdefiniert
    value: Wert
    empty: Keine Claims vorhanden.
    added: Claim hinzugefügt
    updated: Claim aktualisiert
    deleted: Claim gelöscht
    saveFailed: Claim konnte nicht gespeichert werden
    deleteFailed: Claim konnte nicht gelöscht werden
  actions:
    add: Hinzufügen
    share: Teilen
    scan: Scannen
    edit: Bearbeiten
    delete: Löschen
    cancel: Abbrechen
    save: Speichern
    copyKey: Public Key kopieren
    toggleClaims: Claims anzeigen/verbergen
  success:
    added: Kontakt hinzugefügt
    updated: Kontakt aktualisiert
    deleted: Kontakt gelöscht
    copied: Kopiert
  errors:
    addFailed: Kontakt konnte nicht hinzugefügt werden
    updateFailed: Aktualisierung fehlgeschlagen
    deleteFailed: Löschen fehlgeschlagen
    copyFailed: Kopieren fehlgeschlagen
en:
  title: Contacts
  description: Manage your contacts and their public keys
  list:
    title: Your Contacts
    description: Contacts for collaboration in Spaces
    empty: No contacts found
    added: Added
  add:
    title: Add Contact
    description: Add a new contact with their public key
    labelPlaceholder: e.g. Alice, Bob, Team Lead
    publicKeyPlaceholder: Paste Base58-encoded public key
    notesPlaceholder: Optional notes
  edit:
    title: Edit Contact
  delete:
    title: Delete Contact
    description: Do you really want to delete this contact? This action cannot be undone.
  fields:
    label: Name
    publicKey: Public Key
    notes: Notes
  claims:
    title: Claims
    add: Add
    addTitle: Add Claim
    editTitle: Edit Claim
    type: Type
    customType: Custom Type
    custom: Custom
    value: Value
    empty: No claims yet.
    added: Claim added
    updated: Claim updated
    deleted: Claim deleted
    saveFailed: Failed to save claim
    deleteFailed: Failed to delete claim
  actions:
    add: Add
    share: Share
    scan: Scan
    edit: Edit
    delete: Delete
    cancel: Cancel
    save: Save
    copyKey: Copy public key
    toggleClaims: Show/hide claims
  success:
    added: Contact added
    updated: Contact updated
    deleted: Contact deleted
    copied: Copied
  errors:
    addFailed: Failed to add contact
    updateFailed: Failed to update contact
    deleteFailed: Failed to delete contact
    copyFailed: Failed to copy
</i18n>
