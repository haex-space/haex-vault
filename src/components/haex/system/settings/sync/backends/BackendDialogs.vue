<template>
  <!-- Delete Remote Vault Confirmation Dialog -->
  <UiDialogConfirm
    v-model:open="showDeleteDialog"
    :title="
      t(
        vaultToDeleteSpaceId === currentVaultId
          ? 'deleteCurrentVaultSync.title'
          : 'deleteRemoteVault.title',
      )
    "
    :description="
      t(
        vaultToDeleteSpaceId === currentVaultId
          ? 'deleteCurrentVaultSync.description'
          : 'deleteRemoteVault.description',
        { vaultName: vaultToDeleteName },
      )
    "
    :confirm-label="t('actions.delete')"
    @confirm="$emit('confirmDeleteVault')"
  >
    <template #body>
      <label
        class="flex items-start gap-3 cursor-pointer mt-4 p-3 rounded-lg border border-error/30 bg-error/5"
      >
        <UCheckbox
          v-model="deleteAllServerData"
          color="error"
        />
        <div>
          <p class="text-sm font-medium text-error">
            {{ t('deleteAllData.label') }}
          </p>
          <p class="text-xs text-muted mt-0.5">
            {{ t('deleteAllData.description') }}
          </p>
        </div>
      </label>
    </template>
  </UiDialogConfirm>

  <!-- Delete Backend Confirmation Dialog -->
  <UiDialogConfirm
    v-model:open="showDeleteBackendDialog"
    :title="t('deleteBackend.title')"
    :description="
      t('deleteBackend.description', {
        name: backendToDeleteCompletely?.name,
      })
    "
    :confirm-label="t('actions.delete')"
    @confirm="$emit('confirmDeleteBackend')"
  />

  <!-- Re-Upload Confirmation Dialog -->
  <HaexSyncReUploadDialog
    v-model:open="showReUploadDialog"
    :backend="reUploadBackend"
    :loading="isReUploading"
    @confirm="$emit('confirmReUpload')"
  />
</template>

<script setup lang="ts">
import type { SelectHaexSyncBackends } from '~/database/schemas'

defineProps<{
  vaultToDeleteSpaceId: string | null
  vaultToDeleteName: string | null
  currentVaultId: string | null | undefined
  backendToDeleteCompletely: SelectHaexSyncBackends | null
  reUploadBackend: SelectHaexSyncBackends | null
  isReUploading: boolean
}>()

defineEmits<{
  confirmDeleteVault: []
  confirmDeleteBackend: []
  confirmReUpload: []
}>()

const showDeleteDialog = defineModel<boolean>('showDeleteDialog', {
  required: true,
})
const showDeleteBackendDialog = defineModel<boolean>(
  'showDeleteBackendDialog',
  { required: true },
)
const showReUploadDialog = defineModel<boolean>('showReUploadDialog', {
  required: true,
})
const deleteAllServerData = defineModel<boolean>('deleteAllServerData', {
  required: true,
})

const { t } = useI18n()
</script>
