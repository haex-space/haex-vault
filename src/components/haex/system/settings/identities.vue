<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <!-- Identities List -->
    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-semibold">{{ t('list.title') }}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
              {{ t('list.description') }}
            </p>
          </div>
          <div class="flex gap-2">
            <UButton
              color="neutral"
              variant="outline"
              icon="i-lucide-import"
              @click="showImportDialog = true"
            >
              <span class="hidden @sm:inline">{{ t('actions.import') }}</span>
            </UButton>
            <UButton
              color="primary"
              icon="i-lucide-plus"
              @click="showCreateDialog = true"
            >
              <span class="hidden @sm:inline">{{ t('actions.create') }}</span>
            </UButton>
          </div>
        </div>
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

      <!-- Identities list -->
      <div
        v-else-if="identities.length"
        class="space-y-3"
      >
        <div
          v-for="identity in identities"
          :key="identity.id"
          class="p-3 rounded-lg border border-default"
        >
          <div class="flex items-center justify-between">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <UIcon name="i-lucide-fingerprint" class="w-4 h-4 text-primary shrink-0" />
                <span class="font-medium truncate">{{ identity.label }}</span>
              </div>
              <div class="mt-1 flex items-center gap-2">
                <code class="text-xs text-muted truncate max-w-[300px]">{{ identity.did }}</code>
                <UButton
                  size="xs"
                  variant="ghost"
                  icon="i-lucide-copy"
                  :title="t('actions.copyDid')"
                  @click="copyDid(identity.did)"
                />
              </div>
              <p v-if="identity.createdAt" class="text-xs text-muted mt-1">
                {{ t('list.created') }}: {{ formatDate(identity.createdAt) }}
              </p>
            </div>

            <div class="flex items-center gap-1 shrink-0 ml-4">
              <UButton
                size="xs"
                variant="ghost"
                :icon="expandedIdentity === identity.id ? 'i-lucide-chevron-up' : 'i-lucide-chevron-down'"
                :title="t('actions.toggleClaims')"
                @click="toggleExpand(identity.id)"
              />
              <UButton
                size="xs"
                variant="ghost"
                icon="i-lucide-download"
                :title="t('actions.export')"
                @click="onExport(identity)"
              />
              <UButton
                size="xs"
                variant="ghost"
                icon="i-lucide-pencil"
                :title="t('actions.rename')"
                @click="openRenameDialog(identity)"
              />
              <UButton
                size="xs"
                variant="ghost"
                color="error"
                icon="i-lucide-trash-2"
                :title="t('actions.delete')"
                @click="prepareDelete(identity)"
              />
            </div>
          </div>

          <!-- Claims Section (expandable) -->
          <div
            v-if="expandedIdentity === identity.id"
            class="mt-3 pt-3 border-t border-default space-y-2"
          >
            <div class="flex items-center justify-between">
              <span class="text-sm font-medium">{{ t('claims.title') }}</span>
              <UButton
                size="xs"
                variant="outline"
                icon="i-lucide-plus"
                @click="openAddClaim(identity.id)"
              >
                {{ t('claims.add') }}
              </UButton>
            </div>

            <div
              v-if="identityClaims[identity.id]?.length"
              class="space-y-1"
            >
              <div
                v-for="claim in identityClaims[identity.id]"
                :key="claim.id"
                class="flex items-center justify-between p-2 rounded bg-gray-50 dark:bg-gray-800/50"
              >
                <div class="min-w-0 flex-1">
                  <span class="text-xs font-medium text-muted">{{ claim.type }}</span>
                  <p class="text-sm truncate">{{ claim.value }}</p>
                </div>
                <div class="flex gap-1 shrink-0 ml-2">
                  <UButton
                    size="xs"
                    variant="ghost"
                    icon="i-lucide-pencil"
                    @click="openEditClaim(claim)"
                  />
                  <UButton
                    size="xs"
                    variant="ghost"
                    color="error"
                    icon="i-lucide-trash-2"
                    @click="deleteClaimAsync(claim.id, identity.id)"
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
          </div>
        </div>
      </div>

      <!-- Empty state -->
      <div
        v-else
        class="text-center py-4 text-gray-500 dark:text-gray-400"
      >
        {{ t('list.empty') }}
      </div>
    </UCard>

    <!-- Create Identity Dialog -->
    <UiDrawerModal
      v-model:open="showCreateDialog"
      :title="t('create.title')"
      :description="t('create.description')"
    >
      <template #content>
        <UiInput
          v-model="createLabel"
          :label="t('create.labelField')"
          :placeholder="t('create.labelPlaceholder')"
          @keydown.enter.prevent="onCreateAsync"
        />
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showCreateDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-plus"
            :loading="isCreating"
            :disabled="!createLabel.trim()"
            @click="onCreateAsync"
          >
            {{ t('actions.create') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Import Identity Dialog -->
    <UiDrawerModal
      v-model:open="showImportDialog"
      :title="t('import.title')"
      :description="t('import.description')"
    >
      <template #content>
        <UiTextarea
          v-model="importJson"
          :label="t('import.jsonLabel')"
          :placeholder="t('import.jsonPlaceholder')"
          :rows="8"
        />
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showImportDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-import"
            :loading="isImporting"
            :disabled="!importJson.trim()"
            @click="onImportAsync"
          >
            {{ t('actions.import') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Export Identity Dialog -->
    <UiDrawerModal
      v-model:open="showExportDialog"
      :title="t('export.title')"
      :description="t('export.description')"
    >
      <template #content>
        <UiTextarea
          :model-value="exportJson"
          :label="t('export.jsonLabel')"
          :rows="8"
          readonly
        />
        <p class="text-xs text-amber-500 dark:text-amber-400 mt-2">
          {{ t('export.warning') }}
        </p>
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showExportDialog = false"
          >
            {{ t('actions.close') }}
          </UButton>
          <UiButton
            icon="i-lucide-copy"
            @click="copyExport"
          >
            {{ t('actions.copyExport') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>

    <!-- Rename Identity Dialog -->
    <UiDrawerModal
      v-model:open="showRenameDialog"
      :title="t('rename.title')"
    >
      <template #content>
        <UiInput
          v-model="renameLabel"
          :label="t('rename.labelField')"
          @keydown.enter.prevent="onRenameAsync"
        />
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showRenameDialog = false"
          >
            {{ t('actions.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-check"
            :loading="isRenaming"
            :disabled="!renameLabel.trim()"
            @click="onRenameAsync"
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
          <USelectMenu
            v-if="!editingClaim"
            v-model="claimType"
            :items="claimTypeOptions"
            value-key="value"
            :label="t('claims.type')"
            size="lg"
          />
          <UiInput
            v-if="claimType === 'custom' && !editingClaim"
            v-model="claimCustomType"
            :label="t('claims.customType')"
            placeholder="z.B. phone, company"
            size="lg"
          />
          <UiInput
            v-model="claimValue"
            :label="t('claims.value')"
            :placeholder="claimValuePlaceholder"
            size="lg"
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
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'
import type { ExportedIdentity } from '@/stores/identity'

const { t } = useI18n()
const { add } = useToast()

const identityStore = useIdentityStore()
const { identities } = storeToRefs(identityStore)

const isLoading = ref(false)
const isCreating = ref(false)
const isRenaming = ref(false)
const isImporting = ref(false)

const showCreateDialog = ref(false)
const showRenameDialog = ref(false)
const showDeleteConfirm = ref(false)
const showImportDialog = ref(false)
const showExportDialog = ref(false)

const createLabel = ref('')
const renameLabel = ref('')
const renameTarget = ref<SelectHaexIdentities | null>(null)
const deleteTarget = ref<SelectHaexIdentities | null>(null)
const importJson = ref('')
const exportJson = ref('')

onMounted(async () => {
  isLoading.value = true
  try {
    await identityStore.loadIdentitiesAsync()
  } finally {
    isLoading.value = false
  }
})

const onCreateAsync = async () => {
  if (!createLabel.value.trim()) return

  isCreating.value = true
  try {
    await identityStore.createIdentityAsync(createLabel.value.trim())
    add({ title: t('success.created'), color: 'success' })
    showCreateDialog.value = false
    createLabel.value = ''
  } catch (error) {
    console.error('Failed to create identity:', error)
    add({
      title: t('errors.createFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isCreating.value = false
  }
}

const onImportAsync = async () => {
  if (!importJson.value.trim()) return

  isImporting.value = true
  try {
    let parsed: ExportedIdentity
    try {
      parsed = JSON.parse(importJson.value)
    } catch {
      add({ title: t('errors.invalidJson'), color: 'error' })
      return
    }

    if (!parsed.did || !parsed.publicKey || !parsed.privateKey) {
      add({ title: t('errors.invalidIdentityData'), color: 'error' })
      return
    }

    await identityStore.importIdentityAsync(parsed)
    add({ title: t('success.imported'), color: 'success' })
    showImportDialog.value = false
    importJson.value = ''
  } catch (error) {
    console.error('Failed to import identity:', error)
    add({
      title: t('errors.importFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isImporting.value = false
  }
}

const onExport = (identity: SelectHaexIdentities) => {
  exportJson.value = JSON.stringify(identityStore.exportIdentity(identity), null, 2)
  showExportDialog.value = true
}

const copyExport = async () => {
  try {
    await navigator.clipboard.writeText(exportJson.value)
    add({ title: t('success.copied'), color: 'success' })
  } catch {
    add({ title: t('errors.copyFailed'), color: 'error' })
  }
}

const openRenameDialog = (identity: SelectHaexIdentities) => {
  renameTarget.value = identity
  renameLabel.value = identity.label
  showRenameDialog.value = true
}

const onRenameAsync = async () => {
  if (!renameTarget.value || !renameLabel.value.trim()) return

  isRenaming.value = true
  try {
    await identityStore.updateLabelAsync(renameTarget.value.id, renameLabel.value.trim())
    add({ title: t('success.renamed'), color: 'success' })
    showRenameDialog.value = false
  } catch (error) {
    console.error('Failed to rename identity:', error)
    add({
      title: t('errors.renameFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  } finally {
    isRenaming.value = false
  }
}

const prepareDelete = (identity: SelectHaexIdentities) => {
  deleteTarget.value = identity
  showDeleteConfirm.value = true
}

const onConfirmDeleteAsync = async () => {
  if (!deleteTarget.value) return

  try {
    await identityStore.deleteIdentityAsync(deleteTarget.value.id)
    add({ title: t('success.deleted'), color: 'success' })
    showDeleteConfirm.value = false
    deleteTarget.value = null
  } catch (error) {
    console.error('Failed to delete identity:', error)
    add({
      title: t('errors.deleteFailed'),
      description: error instanceof Error ? error.message : undefined,
      color: 'error',
    })
  }
}

const copyDid = async (did: string) => {
  try {
    await navigator.clipboard.writeText(did)
    add({ title: t('success.copied'), color: 'success' })
  } catch {
    add({ title: t('errors.copyFailed'), color: 'error' })
  }
}

const formatDate = (dateStr: string | null) => {
  if (!dateStr) return ''
  return new Date(dateStr).toLocaleDateString()
}

// Claims management
const expandedIdentity = ref<string | null>(null)
const identityClaims = ref<Record<string, { id: string; type: string; value: string }[]>>({})
const showClaimDialog = ref(false)
const claimType = ref('email')
const claimCustomType = ref('')
const claimValue = ref('')
const editingClaim = ref<{ id: string; identityId: string; type: string } | null>(null)
const claimTargetIdentityId = ref<string | null>(null)

const claimTypeOptions = computed(() => {
  const existingTypes = new Set(
    (claimTargetIdentityId.value ? identityClaims.value[claimTargetIdentityId.value] : [])
      ?.map(c => c.type) ?? [],
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
  if (!editingClaim.value && claimType.value === 'custom' && !claimCustomType.value.trim()) return false
  return true
})

const toggleExpand = async (identityId: string) => {
  if (expandedIdentity.value === identityId) {
    expandedIdentity.value = null
    return
  }
  expandedIdentity.value = identityId
  await loadClaimsAsync(identityId)
}

const loadClaimsAsync = async (identityId: string) => {
  const claims = await identityStore.getClaimsAsync(identityId)
  identityClaims.value[identityId] = claims.map(c => ({ id: c.id, type: c.type, value: c.value }))
}

const openAddClaim = (identityId: string) => {
  claimTargetIdentityId.value = identityId
  editingClaim.value = null
  // Pre-select first available (non-disabled) type
  const firstAvailable = claimTypeOptions.value.find(o => !o.disabled)
  claimType.value = firstAvailable?.value ?? 'custom'
  claimCustomType.value = ''
  claimValue.value = ''
  showClaimDialog.value = true
}

const openEditClaim = (claim: { id: string; type: string; value: string }) => {
  editingClaim.value = { id: claim.id, identityId: expandedIdentity.value!, type: claim.type }
  claimValue.value = claim.value
  showClaimDialog.value = true
}

const onSaveClaimAsync = async () => {
  if (!canSaveClaim.value) return

  try {
    if (editingClaim.value) {
      await identityStore.updateClaimAsync(editingClaim.value.id, claimValue.value.trim())
      await loadClaimsAsync(editingClaim.value.identityId)
      add({ title: t('claims.updated'), color: 'success' })
    } else {
      const type = claimType.value === 'custom' ? claimCustomType.value.trim() : claimType.value
      await identityStore.addClaimAsync(claimTargetIdentityId.value!, type, claimValue.value.trim())
      await loadClaimsAsync(claimTargetIdentityId.value!)
      add({ title: t('claims.added'), color: 'success' })
    }
    showClaimDialog.value = false
  } catch (error) {
    console.error('Failed to save claim:', error)
    add({ title: t('claims.saveFailed'), description: error instanceof Error ? error.message : undefined, color: 'error' })
  }
}

const deleteClaimAsync = async (claimId: string, identityId: string) => {
  try {
    await identityStore.deleteClaimAsync(claimId)
    await loadClaimsAsync(identityId)
    add({ title: t('claims.deleted'), color: 'success' })
  } catch (error) {
    console.error('Failed to delete claim:', error)
    add({ title: t('claims.deleteFailed'), color: 'error' })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Identitäten
  description: Verwalte deine kryptographischen Identitäten (did:key)
  list:
    title: Deine Identitäten
    description: Jede Identität ist ein einzigartiges Schlüsselpaar für die Nutzung in Spaces
    empty: Keine Identitäten vorhanden
    created: Erstellt
  create:
    title: Identität erstellen
    description: Erstelle eine neue kryptographische Identität. Jede Identität hat ihren eigenen Schlüssel und kann unabhängig in verschiedenen Spaces genutzt werden.
    labelField: Name
    labelPlaceholder: z.B. Persönlich, Arbeit, Anonym
  import:
    title: Identität importieren
    description: Importiere eine zuvor exportierte Identität. Der DID wird automatisch verifiziert.
    jsonLabel: Identitäts-JSON
    jsonPlaceholder: Exportiertes Identitäts-JSON hier einfügen
  export:
    title: Identität exportieren
    description: Kopiere diese Daten, um die Identität auf einem anderen Gerät zu importieren.
    jsonLabel: Identitäts-JSON
    warning: "Achtung: Dieses JSON enthält deinen privaten Schlüssel. Teile es nur über sichere Kanäle und lösche es nach dem Import."
  rename:
    title: Identität umbenennen
    labelField: Name
  delete:
    title: Identität löschen
    description: Möchtest du diese Identität wirklich löschen? Spaces, die diese Identität nutzen, werden den Zugriff verlieren. Diese Aktion kann nicht rückgängig gemacht werden.
  claims:
    title: Claims
    add: Hinzufügen
    addTitle: Claim hinzufügen
    editTitle: Claim bearbeiten
    type: Typ
    customType: Benutzerdefinierter Typ
    custom: Benutzerdefiniert
    value: Wert
    empty: Keine Claims vorhanden. Füge Email, Name oder andere Daten hinzu.
    added: Claim hinzugefügt
    updated: Claim aktualisiert
    deleted: Claim gelöscht
    saveFailed: Claim konnte nicht gespeichert werden
    deleteFailed: Claim konnte nicht gelöscht werden
  actions:
    create: Erstellen
    import: Importieren
    export: Exportieren
    cancel: Abbrechen
    close: Schließen
    save: Speichern
    rename: Umbenennen
    delete: Löschen
    copyDid: DID kopieren
    copyExport: JSON kopieren
    toggleClaims: Claims anzeigen/verbergen
  success:
    created: Identität erstellt
    imported: Identität importiert
    renamed: Identität umbenannt
    deleted: Identität gelöscht
    copied: Kopiert
  errors:
    createFailed: Identität konnte nicht erstellt werden
    importFailed: Import fehlgeschlagen
    invalidJson: Ungültiges JSON-Format
    invalidIdentityData: Unvollständige Identitätsdaten (did, publicKey und privateKey erforderlich)
    renameFailed: Umbenennung fehlgeschlagen
    deleteFailed: Löschen fehlgeschlagen
    copyFailed: Kopieren fehlgeschlagen
en:
  title: Identities
  description: Manage your cryptographic identities (did:key)
  list:
    title: Your Identities
    description: Each identity is a unique keypair for use in Spaces
    empty: No identities found
    created: Created
  create:
    title: Create Identity
    description: Create a new cryptographic identity. Each identity has its own key and can be used independently in different Spaces.
    labelField: Name
    labelPlaceholder: e.g. Personal, Work, Anonymous
  import:
    title: Import Identity
    description: Import a previously exported identity. The DID will be automatically verified.
    jsonLabel: Identity JSON
    jsonPlaceholder: Paste exported identity JSON here
  export:
    title: Export Identity
    description: Copy this data to import the identity on another device.
    jsonLabel: Identity JSON
    warning: "Warning: This JSON contains your private key. Only share it through secure channels and delete it after import."
  rename:
    title: Rename Identity
    labelField: Name
  delete:
    title: Delete Identity
    description: Do you really want to delete this identity? Spaces using this identity will lose access. This action cannot be undone.
  claims:
    title: Claims
    add: Add
    addTitle: Add Claim
    editTitle: Edit Claim
    type: Type
    customType: Custom Type
    custom: Custom
    value: Value
    empty: No claims yet. Add email, name or other data.
    added: Claim added
    updated: Claim updated
    deleted: Claim deleted
    saveFailed: Failed to save claim
    deleteFailed: Failed to delete claim
  actions:
    create: Create
    import: Import
    export: Export
    cancel: Cancel
    close: Close
    save: Save
    rename: Rename
    delete: Delete
    copyDid: Copy DID
    copyExport: Copy JSON
    toggleClaims: Show/hide claims
  success:
    created: Identity created
    imported: Identity imported
    renamed: Identity renamed
    deleted: Identity deleted
    copied: Copied
  errors:
    createFailed: Failed to create identity
    importFailed: Failed to import identity
    invalidJson: Invalid JSON format
    invalidIdentityData: Incomplete identity data (did, publicKey, and privateKey required)
    renameFailed: Failed to rename identity
    deleteFailed: Failed to delete identity
    copyFailed: Failed to copy
</i18n>
