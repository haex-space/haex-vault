<template>
  <IdentitiesListView
    :identities="identities"
    :is-loading="isLoading"
    :expanded-identity="expandedIdentity"
    :claims-for="claimsFor"
    :is-deletable="isDeletable"
    @import-click="showImportDialog = true"
    @create-click="showCreateDialog = true"
    @toggle="onToggleIdentity"
    @share-qr="onShareQr"
    @copy-did="copyText"
    @export="onExport"
    @edit="openEditDialog"
    @delete="prepareDelete"
    @add-claim="openAddClaim"
    @copy-claim="copyText"
    @edit-claim="openEditClaim"
    @delete-claim="onDeleteClaim"
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

  <IdentityDeleteDialog
    v-model:open="showDeleteConfirm"
    v-model:accepted-sync-backend-loss="acceptedSyncBackendLoss"
    :affected-sync-backends="affectedSyncBackends"
    :affected-admin-spaces="affectedAdminSpaces"
    :affected-member-spaces="affectedMemberSpaces"
    :confirm-label="deleteConfirmLabel"
    :confirm-disabled="!canConfirmDelete"
    :title="t('delete.title')"
    :description="t('delete.description')"
    :sync-backend-warning-title="t('delete.syncBackendWarningTitle')"
    :sync-backend-warning-body="
      t(
        'delete.syncBackendWarningBody',
        { count: affectedSyncBackends.length },
        affectedSyncBackends.length,
      )
    "
    :sync-backend-confirm="t('delete.syncBackendConfirm')"
    :admin-spaces-warning="
      t('delete.adminSpacesWarning', { count: affectedAdminSpaces.length })
    "
    :member-spaces-info="
      t('delete.memberSpacesInfo', { count: affectedMemberSpaces.length })
    "
    @confirm="onConfirmDeleteAsync"
  />

  <ShareIdentityDialog
    v-model:open="showShareQrDialog"
    :pre-selected-identity-id="shareQrIdentityId"
  />
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'
import IdentitiesListView from './identities/IdentitiesListView.vue'
import IdentityDeleteDialog from './identities/IdentityDeleteDialog.vue'
import ShareIdentityDialog from './contacts/ShareIdentityDialog.vue'
import IdentityCreateDialog from './identities/IdentityCreateDialog.vue'
import IdentityEditDialog from './identities/IdentityEditDialog.vue'
import IdentityImportDialog from './identities/IdentityImportDialog.vue'
import IdentityExportDialog from './identities/IdentityExportDialog.vue'
import IdentityClaimDialog from './identities/IdentityClaimDialog.vue'
import { useIdentitiesActions } from '@/composables/useIdentitiesActions'
import { SpaceType } from '~/database/constants'

const { t } = useI18n()

const identityStore = useIdentityStore()
const spacesStore = useSpacesStore()
const { ownIdentities: identities } = storeToRefs(identityStore)
const { spaces } = storeToRefs(spacesStore)

const vaultOwnerIdentityId = computed(
  () => spaces.value.find((s) => s.type === SpaceType.VAULT)?.ownerIdentityId ?? null,
)
const isDeletable = (identity: SelectHaexIdentities) =>
  identity.id !== vaultOwnerIdentityId.value

const claimsFor = (identityId: string) =>
  identityStore.getClaimsForIdentity(identityId).value

const {
  // Loading
  isLoading,
  isCreating,
  isRenaming,
  isImporting,
  isExporting,
  // Dialog visibility
  showCreateDialog,
  showEditDialog,
  showDeleteConfirm,
  showImportDialog,
  showExportDialog,
  showShareQrDialog,
  showPrivateKeyConfirm,
  showClaimDialog,
  // Per-dialog target state
  shareQrIdentityId,
  editTarget,
  affectedAdminSpaces,
  affectedMemberSpaces,
  affectedSyncBackends,
  acceptedSyncBackendLoss,
  importJson,
  importParsed,
  exportTarget,
  exportClaims,
  editingClaim,
  expandedIdentity,
  // Computed
  canConfirmDelete,
  deleteConfirmLabel,
  // Vault password (for CreateDialog prop)
  currentVaultPassword,
  // Lifecycle
  loadIdentitiesAndSpacesAsync,
  // Handlers
  onToggleIdentity,
  copyText,
  onCreateAsync,
  onSelectImportFileAsync,
  onParseImport,
  onImportAsync,
  onShareQr,
  onExport,
  onExportSubmit,
  onConfirmExportWithPrivateKeyAsync,
  openEditDialog,
  onEditAvatarUpdateAsync,
  onRenameAsync,
  prepareDelete,
  onConfirmDeleteAsync,
  openAddClaim,
  openEditClaim,
  onClaimSubmitAsync,
  onDeleteClaim,
} = useIdentitiesActions()

onMounted(loadIdentitiesAndSpacesAsync)
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
    syncBackendWarningTitle: 'Achtung: Datenverlust auf Sync-Servern'
    syncBackendWarningBody: 'Diese Identität ist die einzige Authentifizierung für den folgenden Sync-Server. Nach dem Löschen kannst du nicht mehr auf bereits hochgeladene Daten dort zugreifen — sie sind dauerhaft verloren, sofern du kein anderes Gerät mit derselben Identität besitzt. | Diese Identität ist die einzige Authentifizierung für die folgenden {count} Sync-Server. Nach dem Löschen kannst du nicht mehr auf bereits hochgeladene Daten dort zugreifen — sie sind dauerhaft verloren, sofern du kein anderes Gerät mit derselben Identität besitzt.'
    syncBackendConfirm: Ich habe verstanden, dass Daten auf diesen Servern unwiederbringlich verloren gehen.
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
    vaultOwnerDelete: Die Eigentümer-Identität des Tresors kann nicht gelöscht werden
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
    syncBackendWarningTitle: 'Warning: data loss on sync servers'
    syncBackendWarningBody: 'This identity is the only credential for the sync server listed below. After deletion, any data already uploaded there becomes unreachable from this vault — it is permanently lost unless you have another device holding the same identity. | This identity is the only credential for the {count} sync servers listed below. After deletion, any data already uploaded there becomes unreachable from this vault — it is permanently lost unless you have another device holding the same identity.'
    syncBackendConfirm: I understand that data on these servers will be permanently lost.
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
    vaultOwnerDelete: The vault owner identity cannot be deleted
    exportFailed: Export failed
    importFailed: Import failed
    invalidJson: Invalid JSON
    invalidIdentityData: No valid identity data found
    passwordUpdateFailed: Failed to update password
    copyFailed: Copy failed
</i18n>
