<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
  >
    <template #actions>
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
        data-tour="settings-identities-create"
        @click="showCreateDialog = true"
      >
        <span class="hidden @sm:inline">{{ t('actions.create') }}</span>
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

    <!-- Identities list -->
    <div
      v-else-if="identities.length"
      class="space-y-3"
    >
      <IdentityListItem
        v-for="identity in identities"
        :key="identity.id"
        :identity="identity"
        :expanded="expandedIdentity === identity.id"
        :claims="identityStore.getClaimsForIdentity(identity.id).value"
        @toggle="(open) => onToggleIdentity(identity.id, open)"
        @share-qr="onShareQr(identity)"
        @copy-did="copyText(identity.did)"
        @export="onExport(identity)"
        @edit="openEditDialog(identity)"
        @delete="prepareDelete(identity)"
        @add-claim="openAddClaim(identity.id)"
        @copy-claim="copyText"
        @edit-claim="openEditClaim(identity.id, $event)"
        @delete-claim="(claimId) => onDeleteClaim(claimId, identity.id)"
      />
    </div>

    <!-- Empty state -->
    <HaexSystemSettingsLayoutEmpty
      v-else
      :message="t('list.empty')"
      icon="i-lucide-fingerprint"
    />

    <IdentityCreateDialog
      v-model:open="showCreateDialog"
      :submitting="isCreating"
      :vault-password-available="!!currentVaultPassword"
      @submit="onCreateAsync"
    />

    <IdentityImportDialog
      v-model:open="showImportDialog"
      v-model:parsed="importParsed"
      v-model:json="importJson"
      :submitting="isImporting"
      @parse="onParseImport"
      @select-file="onSelectImportFileAsync"
      @submit="onImportAsync"
    />

    <IdentityExportDialog
      v-model:open="showExportDialog"
      :target="exportTarget"
      :claims="exportClaims"
      :submitting="isExporting"
      @submit="onExportSubmit"
    />

    <UiDialogConfirm
      v-model:open="showPrivateKeyConfirm"
      :title="t('export.confirmPrivateKey.title')"
      :description="t('export.confirmPrivateKey.description')"
      @confirm="onConfirmExportWithPrivateKeyAsync"
    />

    <IdentityEditDialog
      v-model:open="showEditDialog"
      :target="editTarget"
      :submitting="isRenaming"
      @submit="onRenameAsync"
      @avatar-update="onEditAvatarUpdateAsync"
    />

    <IdentityClaimDialog
      v-model:open="showClaimDialog"
      :editing-claim="editingClaim"
      @submit="onClaimSubmitAsync"
    />

    <UiDialogConfirm
      v-model:open="showDeleteConfirm"
      :title="t('delete.title')"
      :description="t('delete.description')"
      :confirm-label="t('delete.confirmLabel')"
      confirm-icon="i-lucide-trash-2"
      @confirm="onConfirmDeleteAsync"
    >
      <div
        v-if="affectedAdminSpaces.length > 0"
        class="mt-4 space-y-2"
      >
        <p class="text-sm font-medium text-highlighted">
          {{
            t('delete.adminSpacesWarning', {
              count: affectedAdminSpaces.length,
            })
          }}
        </p>
        <ul class="list-disc list-inside text-sm text-muted">
          <li
            v-for="space in affectedAdminSpaces"
            :key="space.id"
            class="font-medium"
          >
            {{ space.name }}
          </li>
        </ul>
      </div>
      <div
        v-if="affectedMemberSpaces.length > 0"
        class="mt-3 space-y-2"
      >
        <p class="text-sm text-muted">
          {{
            t('delete.memberSpacesInfo', { count: affectedMemberSpaces.length })
          }}
        </p>
      </div>
    </UiDialogConfirm>

    <ShareIdentityDialog
      v-model:open="showShareQrDialog"
      :pre-selected-identity-id="shareQrIdentityId"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type {
  SelectHaexIdentities,
  SelectHaexSpaces,
} from '~/database/schemas'
import ShareIdentityDialog from './contacts/ShareIdentityDialog.vue'
import IdentityListItem, {
  type ListItemClaim,
} from './identities/IdentityListItem.vue'
import IdentityCreateDialog, {
  type CreateSubmitPayload,
} from './identities/IdentityCreateDialog.vue'
import IdentityEditDialog, {
  type EditSubmitPayload,
  type AvatarUpdatePayload,
} from './identities/IdentityEditDialog.vue'
import IdentityImportDialog, {
  type ImportSubmitPayload,
} from './identities/IdentityImportDialog.vue'
import IdentityExportDialog, {
  type ExportSubmitPayload,
} from './identities/IdentityExportDialog.vue'
import IdentityClaimDialog, {
  type ClaimDialogEditTarget,
  type ClaimSubmitPayload,
} from './identities/IdentityClaimDialog.vue'
import { useIdentityCreation } from '@/composables/useIdentityCreation'
import {
  useIdentityImport,
  InvalidImportJsonError,
  InvalidImportDataError,
  type ParsedIdentityImport,
} from '@/composables/useIdentityImport'
import {
  useIdentityExport,
  type ExportClaim,
} from '@/composables/useIdentityExport'
import { useUpdateIdentityPassword } from '@/composables/useUpdateIdentityPassword'
import { useOperationErrorToast } from '@/composables/useOperationErrorToast'

const { t } = useI18n()
const { add } = useToast()

const identityStore = useIdentityStore()
const { ownIdentities: identities } = storeToRefs(identityStore)
const { currentVaultPassword } = storeToRefs(useVaultStore())

const { createIdentityAsync: runCreateIdentityAsync } = useIdentityCreation()
const { parseImport, importAsync } = useIdentityImport()
const { exportToFileAsync } = useIdentityExport()
const { updatePasswordAsync } = useUpdateIdentityPassword()
const { showOperationError } = useOperationErrorToast()

// =========================================================================
// Loading & dialog visibility
// =========================================================================

const isLoading = ref(false)
const isCreating = ref(false)
const isRenaming = ref(false)
const isImporting = ref(false)
const isExporting = ref(false)

const showCreateDialog = ref(false)
const showEditDialog = ref(false)
const showDeleteConfirm = ref(false)
const showImportDialog = ref(false)
const showExportDialog = ref(false)
const showShareQrDialog = ref(false)
const showPrivateKeyConfirm = ref(false)
const showClaimDialog = ref(false)

// =========================================================================
// Per-dialog target state
// =========================================================================

const shareQrIdentityId = ref('')

const editTarget = ref<SelectHaexIdentities | null>(null)

const deleteTarget = ref<SelectHaexIdentities | null>(null)
const affectedAdminSpaces = ref<SelectHaexSpaces[]>([])
const affectedMemberSpaces = ref<SelectHaexSpaces[]>([])

// Import
const importJson = ref('')
const importParsed = ref<ParsedIdentityImport | null>(null)

// Export
const exportTarget = ref<SelectHaexIdentities | null>(null)
const exportClaims = ref<ExportClaim[]>([])
/** Retained across the private-key confirm round-trip. */
const pendingExportOptions = ref<ExportSubmitPayload | null>(null)

// Claim dialog
const editingClaim = ref<ClaimDialogEditTarget | null>(null)
const claimTargetIdentityId = ref<string | null>(null)

// List item expansion
const expandedIdentity = ref<string | null>(null)

// =========================================================================
// Lifecycle
// =========================================================================

onMounted(async () => {
  isLoading.value = true
  try {
    await identityStore.loadIdentitiesAsync()
  } finally {
    isLoading.value = false
  }
})

// =========================================================================
// List item
// =========================================================================

const onToggleIdentity = async (identityId: string, open: boolean) => {
  if (!open) {
    expandedIdentity.value = null
    return
  }
  expandedIdentity.value = identityId
  await identityStore.loadClaimsAsync(identityId)
}

const copyText = async (value: string) => {
  try {
    await navigator.clipboard.writeText(value)
    add({ title: t('success.copied'), color: 'success' })
  } catch {
    add({ title: t('errors.copyFailed'), color: 'error' })
  }
}

// =========================================================================
// Create
// =========================================================================

const onCreateAsync = async (payload: CreateSubmitPayload) => {
  isCreating.value = true
  try {
    const effectivePassword = payload.useVaultPassword
      ? (currentVaultPassword.value ?? '')
      : payload.identityPassword

    await runCreateIdentityAsync({
      label: payload.label,
      avatar: payload.avatar,
      avatarOptions: payload.avatarOptions,
      password: effectivePassword,
      claims: [
        { type: 'email', value: payload.claims.email },
        { type: 'name', value: payload.claims.name },
        { type: 'phone', value: payload.claims.phone },
        { type: 'address', value: payload.claims.address },
      ],
    })

    add({ title: t('success.created'), color: 'success' })
    showCreateDialog.value = false
  } catch (error) {
    console.error('Failed to create identity:', error)
    showOperationError(error, 'errors.createFailed')
  } finally {
    isCreating.value = false
  }
}

// =========================================================================
// Import
// =========================================================================

const onSelectImportFileAsync = async () => {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const { readFile } = await import('@tauri-apps/plugin-fs')

    const filePath = await open({
      title: t('import.title'),
      filters: [{ name: 'JSON', extensions: ['json'] }],
      multiple: false,
    })
    if (!filePath) return

    const data = await readFile(filePath as string)
    importJson.value = new TextDecoder().decode(data)
  } catch (error) {
    console.error('Failed to read file:', error)
    showOperationError(error, 'errors.importFailed')
  }
}

const onParseImport = (rawJson: string) => {
  try {
    importParsed.value = parseImport(rawJson)
  } catch (error) {
    if (error instanceof InvalidImportJsonError) {
      add({ title: t('errors.invalidJson'), color: 'error' })
      return
    }
    if (error instanceof InvalidImportDataError) {
      add({ title: t('errors.invalidIdentityData'), color: 'error' })
      return
    }
    showOperationError(error, 'errors.importFailed')
  }
}

const onImportAsync = async (payload: ImportSubmitPayload) => {
  isImporting.value = true
  try {
    const result = await importAsync(payload.parsed, {
      selectedClaimIndices: payload.selectedClaimIndices,
      includeAvatar: payload.includeAvatar,
    })

    add({
      title: t(
        result.kind === 'identity' ? 'success.imported' : 'success.importedAsContact',
      ),
      color: 'success',
    })
    showImportDialog.value = false
  } catch (error) {
    console.error('Failed to import:', error)
    showOperationError(error, 'errors.importFailed')
  } finally {
    isImporting.value = false
  }
}

// =========================================================================
// Export
// =========================================================================

const onShareQr = (identity: SelectHaexIdentities) => {
  shareQrIdentityId.value = identity.id
  showShareQrDialog.value = true
}

const onExport = async (identity: SelectHaexIdentities) => {
  exportTarget.value = identity
  const claims = await identityStore.getClaimsAsync(identity.id)
  exportClaims.value = claims.map((c) => ({
    id: c.id,
    type: c.type,
    value: c.value,
  }))
  showExportDialog.value = true
}

const onExportSubmit = async (payload: ExportSubmitPayload) => {
  if (!exportTarget.value) return

  // Intercept private-key exports for an extra confirmation step.
  if (payload.includePrivateKey) {
    pendingExportOptions.value = payload
    showPrivateKeyConfirm.value = true
    return
  }

  await runExportAsync(payload)
}

const onConfirmExportWithPrivateKeyAsync = async () => {
  showPrivateKeyConfirm.value = false
  if (!pendingExportOptions.value) return
  const options = pendingExportOptions.value
  pendingExportOptions.value = null
  await runExportAsync(options)
}

const runExportAsync = async (options: ExportSubmitPayload) => {
  if (!exportTarget.value) return

  isExporting.value = true
  try {
    const outcome = await exportToFileAsync(
      exportTarget.value,
      exportClaims.value,
      {
        selectedClaimIds: options.selectedClaimIds,
        includeAvatar: options.includeAvatar,
        includePrivateKey: options.includePrivateKey,
      },
      t('export.title'),
    )
    if (outcome.saved) {
      add({ title: t('success.exported'), color: 'success' })
      showExportDialog.value = false
    }
  } catch (error) {
    console.error('Failed to export identity:', error)
    showOperationError(error, 'errors.exportFailed')
  } finally {
    isExporting.value = false
  }
}

// =========================================================================
// Edit (rename + password + avatar)
// =========================================================================

const openEditDialog = (identity: SelectHaexIdentities) => {
  editTarget.value = identity
  showEditDialog.value = true
}

const onEditAvatarUpdateAsync = async (payload: AvatarUpdatePayload) => {
  if (!editTarget.value) return

  const optionsJson =
    payload.options !== undefined
      ? payload.options
        ? JSON.stringify(payload.options)
        : null
      : undefined

  await identityStore.updateAvatarAsync(
    editTarget.value.id,
    payload.avatar,
    optionsJson,
  )

  // Refresh local snapshot so the dialog immediately shows the new avatar.
  editTarget.value = {
    ...editTarget.value,
    avatar: payload.avatar,
    avatarOptions: optionsJson ?? editTarget.value.avatarOptions,
  }
}

const onRenameAsync = async (payload: EditSubmitPayload) => {
  if (!editTarget.value) return

  isRenaming.value = true
  try {
    await identityStore.updateLabelAsync(editTarget.value.id, payload.label)

    if (payload.newPassword) {
      const ok = await updatePasswordAsync(
        editTarget.value.id,
        payload.newPassword,
      )
      if (!ok) {
        add({ title: t('errors.passwordUpdateFailed'), color: 'error' })
        return
      }
    }

    add({ title: t('success.saved'), color: 'success' })
    showEditDialog.value = false
  } catch (error) {
    console.error('Failed to edit identity:', error)
    showOperationError(error, 'errors.editFailed')
  } finally {
    isRenaming.value = false
  }
}

// =========================================================================
// Delete
// =========================================================================

const prepareDelete = async (identity: SelectHaexIdentities) => {
  deleteTarget.value = identity
  const affected = await identityStore.getAffectedSpacesAsync(identity.id)
  affectedAdminSpaces.value = affected.adminSpaces
  affectedMemberSpaces.value = affected.memberSpaces
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
    showOperationError(error, 'errors.deleteFailed')
  }
}

// =========================================================================
// Claims
// =========================================================================

const openAddClaim = (identityId: string) => {
  claimTargetIdentityId.value = identityId
  editingClaim.value = null
  showClaimDialog.value = true
}

const openEditClaim = (identityId: string, claim: ListItemClaim) => {
  claimTargetIdentityId.value = identityId
  editingClaim.value = { id: claim.id, type: claim.type, value: claim.value }
  showClaimDialog.value = true
}

const onClaimSubmitAsync = async (payload: ClaimSubmitPayload) => {
  try {
    if (payload.mode === 'edit') {
      await identityStore.updateClaimAsync(payload.claimId, payload.value)
      add({ title: t('claims.updated'), color: 'success' })
    } else {
      if (!claimTargetIdentityId.value) return
      await identityStore.addClaimAsync(
        claimTargetIdentityId.value,
        payload.type,
        payload.value,
      )
      add({ title: t('claims.added'), color: 'success' })
    }
    showClaimDialog.value = false
  } catch (error) {
    console.error('Failed to save claim:', error)
    showOperationError(error, 'claims.saveFailed')
  }
}

const onDeleteClaim = async (claimId: string, identityId: string) => {
  try {
    await identityStore.deleteClaimAsync(claimId)
    add({ title: t('claims.deleted'), color: 'success' })
  } catch (error) {
    console.error('Failed to delete claim:', error)
    add({ title: t('claims.deleteFailed'), color: 'error' })
  }
  // Consumed for future use; keep identityId in the signature for clarity.
  void identityId
}
</script>

<i18n lang="yaml">
de:
  title: Identitäten
  description: Verwalte deine kryptographischen Identitäten (did:key)
  list:
    empty: Keine Identitäten vorhanden
  export:
    title: Identität exportieren
    confirmPrivateKey:
      title: Privaten Schlüssel exportieren?
      description: Wenn jemand diese Datei erhält, kann sie deine Identität vollständig übernehmen. Nur für vollständige Backups.
  import:
    title: Identität importieren
  delete:
    title: Identität löschen
    description: Möchtest du diese Identität wirklich löschen? Diese Aktion ist unwiderruflich.
    confirmLabel: Endgültig löschen
    adminSpacesWarning: 'Diese Spaces werden ebenfalls gelöscht ({count}):'
    memberSpacesInfo: 'Aus {count} weiteren Spaces wirst du entfernt.'
  claims:
    updated: Claim aktualisiert
    added: Claim hinzugefügt
    deleted: Claim gelöscht
    saveFailed: Claim konnte nicht gespeichert werden
    deleteFailed: Claim konnte nicht gelöscht werden
  actions:
    import: Importieren
    create: Erstellen
  success:
    created: Identität erstellt
    saved: Gespeichert
    deleted: Identität gelöscht
    imported: Identität importiert
    importedAsContact: Kontakt importiert
    exported: Identität exportiert
    copied: In die Zwischenablage kopiert
  errors:
    createFailed: Identität konnte nicht erstellt werden
    editFailed: Identität konnte nicht bearbeitet werden
    deleteFailed: Identität konnte nicht gelöscht werden
    exportFailed: Export fehlgeschlagen
    importFailed: Import fehlgeschlagen
    invalidJson: Ungültiges JSON
    invalidIdentityData: Keine gültigen Identitätsdaten gefunden
    passwordUpdateFailed: Passwort konnte nicht aktualisiert werden
    copyFailed: Kopieren fehlgeschlagen
en:
  title: Identities
  description: Manage your cryptographic identities (did:key)
  list:
    empty: No identities found
  export:
    title: Export Identity
    confirmPrivateKey:
      title: Export private key?
      description: Anyone with this file can fully impersonate your identity. Use only for full backups.
  import:
    title: Import Identity
  delete:
    title: Delete Identity
    description: Do you really want to delete this identity? This action cannot be undone.
    confirmLabel: Delete permanently
    adminSpacesWarning: 'These spaces will also be deleted ({count}):'
    memberSpacesInfo: 'You will be removed from {count} more spaces.'
  claims:
    updated: Claim updated
    added: Claim added
    deleted: Claim deleted
    saveFailed: Failed to save claim
    deleteFailed: Failed to delete claim
  actions:
    import: Import
    create: Create
  success:
    created: Identity created
    saved: Saved
    deleted: Identity deleted
    imported: Identity imported
    importedAsContact: Contact imported
    exported: Identity exported
    copied: Copied to clipboard
  errors:
    createFailed: Failed to create identity
    editFailed: Failed to edit identity
    deleteFailed: Failed to delete identity
    exportFailed: Export failed
    importFailed: Import failed
    invalidJson: Invalid JSON
    invalidIdentityData: No valid identity data found
    passwordUpdateFailed: Failed to update password
    copyFailed: Copy failed
</i18n>
