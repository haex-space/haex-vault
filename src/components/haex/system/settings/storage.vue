<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <!-- Add/Edit Backend Form -->
    <UCard v-if="showBackendForm" class="relative">
      <!-- Loading Overlay -->
      <div
        v-if="isLoading"
        class="absolute inset-0 z-10 flex items-center justify-center bg-default/80 backdrop-blur-sm rounded-lg"
      >
        <div class="flex flex-col items-center gap-3">
          <div class="loading loading-spinner loading-lg text-primary" />
          <span class="text-sm text-muted">
            {{ t('form.connecting') }}
          </span>
        </div>
      </div>

      <template #header>
        <div class="flex justify-between px-1">
          <h3 class="text-lg font-semibold">
            {{ isEditMode ? t('editBackend.title') : t('addBackend.title') }}
          </h3>

          <UiButton
            icon="mdi-close"
            variant="ghost"
            color="neutral"
            :disabled="isLoading"
            @click="closeForm"
          />
        </div>
      </template>

      <form class="space-y-4" @submit.prevent="onSubmitFormAsync">
        <UFormField :label="t('form.name.label')" required>
          <UiInput
            v-model="formData.name"
            :placeholder="t('form.name.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.endpoint.label')" :description="t('form.endpoint.description')">
          <UiInput
            v-model="formData.endpoint"
            :placeholder="t('form.endpoint.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.bucket.label')" required>
          <UiInput
            v-model="formData.bucket"
            :placeholder="t('form.bucket.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.region.label')" required>
          <UiInput
            v-model="formData.region"
            :placeholder="t('form.region.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.accessKeyId.label')" :required="!isEditMode">
          <UiInput
            v-model="formData.accessKeyId"
            :placeholder="isEditMode ? t('form.accessKeyId.keepExisting') : t('form.accessKeyId.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.secretAccessKey.label')" :required="!isEditMode">
          <UiInput
            v-model="formData.secretAccessKey"
            type="password"
            :placeholder="isEditMode ? t('form.secretAccessKey.keepExisting') : t('form.secretAccessKey.placeholder')"
          />
        </UFormField>

        <UFormField>
          <UCheckbox
            v-model="formData.pathStyle"
            :label="t('form.pathStyle.label')"
            :description="t('form.pathStyle.description')"
          />
        </UFormField>
      </form>

      <template #footer>
        <div class="flex justify-between">
          <UiButton
            color="neutral"
            variant="outline"
            :disabled="isLoading"
            @click="closeForm"
          >
            {{ t('actions.cancel') }}
          </UiButton>

          <UiButton
            :icon="isEditMode ? 'i-lucide-save' : 'mdi-plus'"
            :disabled="isLoading || !isFormValid"
            @click="onSubmitFormAsync"
          >
            <span class="hidden @sm:inline">
              {{ isEditMode ? t('actions.save') : t('actions.add') }}
            </span>
          </UiButton>
        </div>
      </template>
    </UCard>

    <!-- Storage Backends List -->
    <UCard v-if="!showBackendForm || storageBackends.length">
      <template #header>
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-semibold">{{ t('backends.title') }}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
              {{ t('backends.description') }}
            </p>
          </div>
          <UiButton
            v-if="!showBackendForm"
            icon="i-lucide-plus"
            @click="openAddForm"
          >
            <span class="hidden @sm:inline">
              {{ t('actions.add') }}
            </span>
          </UiButton>
        </div>
      </template>

      <div v-if="storageBackends.length" class="space-y-3">
        <div
          v-for="backend in storageBackends"
          :key="backend.id"
          class="p-4 rounded-lg border border-default bg-muted/30"
        >
          <div class="flex flex-col @md:flex-row @md:items-center @md:justify-between gap-3">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <h4 class="font-medium">{{ backend.name }}</h4>
                <UBadge
                  :color="backend.enabled ? 'success' : 'neutral'"
                  variant="subtle"
                  size="xs"
                >
                  {{ backend.enabled ? t('backends.enabled') : t('backends.disabled') }}
                </UBadge>
              </div>
              <div class="text-sm text-muted mt-1 space-y-0.5">
                <p v-if="backend.config?.endpoint">
                  <span class="font-medium">{{ t('form.endpoint.label') }}:</span>
                  {{ backend.config.endpoint }}
                </p>
                <p>
                  <span class="font-medium">{{ t('form.bucket.label') }}:</span>
                  {{ backend.config?.bucket }}
                </p>
                <p>
                  <span class="font-medium">{{ t('form.region.label') }}:</span>
                  {{ backend.config?.region }}
                </p>
              </div>
            </div>

            <div class="flex items-center gap-2 shrink-0">
              <UiButton
                color="neutral"
                variant="outline"
                size="sm"
                :loading="testingBackendId === backend.id"
                :disabled="testingBackendId !== null"
                @click="onTestBackendAsync(backend.id)"
              >
                {{ t('actions.test') }}
              </UiButton>
              <UiButton
                color="neutral"
                variant="ghost"
                icon="i-lucide-pencil"
                size="sm"
                @click="openEditForm(backend)"
              />
              <UiButton
                color="error"
                variant="ghost"
                icon="i-lucide-trash-2"
                size="sm"
                @click="prepareDeleteBackend(backend)"
              />
            </div>
          </div>
        </div>
      </div>

      <div
        v-else
        class="text-center py-8 text-gray-500 dark:text-gray-400"
      >
        <UIcon name="i-heroicons-cloud" class="w-12 h-12 mx-auto mb-3 opacity-50" />
        <p>{{ t('backends.noBackends') }}</p>
        <p class="text-sm mt-1">{{ t('backends.noBackendsHint') }}</p>
      </div>
    </UCard>

    <!-- Delete Confirmation Dialog -->
    <UiDialogConfirm
      v-model:open="showDeleteDialog"
      :title="t('deleteBackend.title')"
      :description="t('deleteBackend.description', { name: backendToDelete?.name })"
      :confirm-label="t('actions.delete')"
      @confirm="onConfirmDeleteAsync"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { StorageBackendInfo } from '~/../src-tauri/bindings/StorageBackendInfo'
import type { AddStorageBackendRequest } from '~/../src-tauri/bindings/AddStorageBackendRequest'
import type { UpdateStorageBackendRequest } from '~/../src-tauri/bindings/UpdateStorageBackendRequest'

const { t } = useI18n()
const { add } = useToast()

// State
const storageBackends = ref<StorageBackendInfo[]>([])
const showBackendForm = ref(false)
const isEditMode = ref(false)
const editingBackendId = ref<string | null>(null)
const isLoading = ref(false)
const testingBackendId = ref<string | null>(null)
const showDeleteDialog = ref(false)
const backendToDelete = ref<StorageBackendInfo | null>(null)

const formData = reactive({
  name: '',
  endpoint: '',
  bucket: '',
  region: 'auto',
  accessKeyId: '',
  secretAccessKey: '',
  pathStyle: false,
})

const isFormValid = computed(() => {
  const baseValid =
    formData.name.trim() !== '' &&
    formData.bucket.trim() !== '' &&
    formData.region.trim() !== ''

  // In edit mode, credentials are optional (keep existing)
  if (isEditMode.value) {
    return baseValid
  }

  // In add mode, credentials are required
  return (
    baseValid &&
    formData.accessKeyId.trim() !== '' &&
    formData.secretAccessKey.trim() !== ''
  )
})

// Load backends on mount
onMounted(async () => {
  await loadBackendsAsync()
})

const loadBackendsAsync = async () => {
  try {
    storageBackends.value = await invoke<StorageBackendInfo[]>('remote_storage_list_backends')
  } catch (error) {
    console.error('Failed to load storage backends:', error)
    add({
      title: t('errors.loadFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const resetForm = () => {
  formData.name = ''
  formData.endpoint = ''
  formData.bucket = ''
  formData.region = 'auto'
  formData.accessKeyId = ''
  formData.secretAccessKey = ''
  formData.pathStyle = false
  isEditMode.value = false
  editingBackendId.value = null
}

const openAddForm = () => {
  resetForm()
  showBackendForm.value = true
}

const openEditForm = (backend: StorageBackendInfo) => {
  resetForm()
  isEditMode.value = true
  editingBackendId.value = backend.id
  formData.name = backend.name
  formData.endpoint = backend.config?.endpoint || ''
  formData.bucket = backend.config?.bucket || ''
  formData.region = backend.config?.region || 'auto'
  // Credentials are not returned from the backend for security
  formData.accessKeyId = ''
  formData.secretAccessKey = ''
  formData.pathStyle = false // TODO: Add pathStyle to S3PublicConfig if needed
  showBackendForm.value = true
}

const closeForm = () => {
  showBackendForm.value = false
  resetForm()
}

const onSubmitFormAsync = async () => {
  if (!isFormValid.value) return

  isLoading.value = true

  try {
    if (isEditMode.value && editingBackendId.value) {
      await onUpdateBackendAsync()
    } else {
      await onAddBackendAsync()
    }
  } finally {
    isLoading.value = false
  }
}

const onAddBackendAsync = async () => {
  try {
    const config: Record<string, unknown> = {
      bucket: formData.bucket,
      region: formData.region,
      accessKeyId: formData.accessKeyId,
      secretAccessKey: formData.secretAccessKey,
    }

    if (formData.endpoint.trim()) {
      config.endpoint = formData.endpoint.trim()
    }

    if (formData.pathStyle) {
      config.pathStyle = true
    }

    const request: AddStorageBackendRequest = {
      name: formData.name,
      type: 's3',
      config,
    }

    await invoke('remote_storage_add_backend', { request })

    add({
      title: t('success.backendAdded'),
      color: 'success',
    })

    await loadBackendsAsync()
    closeForm()
  } catch (error) {
    console.error('Failed to add storage backend:', error)
    add({
      title: t('errors.addFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const onUpdateBackendAsync = async () => {
  if (!editingBackendId.value) return

  try {
    const config: Record<string, unknown> = {
      bucket: formData.bucket,
      region: formData.region,
    }

    if (formData.endpoint.trim()) {
      config.endpoint = formData.endpoint.trim()
    }

    if (formData.pathStyle) {
      config.pathStyle = true
    }

    // Only include credentials if provided (otherwise keep existing)
    if (formData.accessKeyId.trim()) {
      config.accessKeyId = formData.accessKeyId.trim()
    }
    if (formData.secretAccessKey.trim()) {
      config.secretAccessKey = formData.secretAccessKey.trim()
    }

    const request: UpdateStorageBackendRequest = {
      backendId: editingBackendId.value,
      name: formData.name,
      config,
    }

    await invoke('remote_storage_update_backend', { request })

    add({
      title: t('success.backendUpdated'),
      color: 'success',
    })

    await loadBackendsAsync()
    closeForm()
  } catch (error) {
    console.error('Failed to update storage backend:', error)
    add({
      title: t('errors.updateFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const onTestBackendAsync = async (backendId: string) => {
  testingBackendId.value = backendId

  try {
    await invoke('remote_storage_test_backend', { backendId })
    add({
      title: t('success.connectionOk'),
      color: 'success',
    })
  } catch (error) {
    console.error('Connection test failed:', error)
    add({
      title: t('errors.testFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    testingBackendId.value = null
  }
}

const prepareDeleteBackend = (backend: StorageBackendInfo) => {
  backendToDelete.value = backend
  showDeleteDialog.value = true
}

const onConfirmDeleteAsync = async () => {
  if (!backendToDelete.value) return

  try {
    await invoke('remote_storage_remove_backend', { backendId: backendToDelete.value.id })

    add({
      title: t('success.backendDeleted'),
      color: 'success',
    })

    await loadBackendsAsync()
  } catch (error) {
    console.error('Failed to delete storage backend:', error)
    add({
      title: t('errors.deleteFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    showDeleteDialog.value = false
    backendToDelete.value = null
  }
}
</script>

<i18n lang="yaml">
de:
  title: Storage
  description: Verwalte S3-kompatible Storage Backends für Erweiterungen
  addBackend:
    title: Storage Backend hinzufügen
  editBackend:
    title: Storage Backend bearbeiten
  backends:
    title: Storage Backends
    description: S3-kompatible Speicherdienste für Datei-Uploads
    noBackends: Keine Storage Backends konfiguriert
    noBackendsHint: Füge ein S3-kompatibles Backend hinzu, um Dateien zu speichern
    enabled: Aktiviert
    disabled: Deaktiviert
  form:
    connecting: Verbindung wird getestet...
    name:
      label: Name
      placeholder: Mein S3 Speicher
    endpoint:
      label: Endpoint
      description: Nur für S3-kompatible Dienste wie MinIO, Cloudflare R2, etc.
      placeholder: https://s3.example.com
    bucket:
      label: Bucket
      placeholder: my-bucket
    region:
      label: Region
      placeholder: eu-central-1
    accessKeyId:
      label: Access Key ID
      placeholder: AKIAIOSFODNN7EXAMPLE
      keepExisting: Leer lassen um bestehenden Key zu behalten
    secretAccessKey:
      label: Secret Access Key
      placeholder: "********"
      keepExisting: Leer lassen um bestehendes Secret zu behalten
    pathStyle:
      label: Path-Style URLs verwenden
      description: Aktivieren für MinIO und andere S3-kompatible Dienste
  actions:
    add: Hinzufügen
    save: Speichern
    cancel: Abbrechen
    test: Testen
    delete: Löschen
  deleteBackend:
    title: Storage Backend löschen
    description: Möchtest du das Backend "{name}" wirklich löschen? Erweiterungen können dann nicht mehr auf dieses Backend zugreifen.
  success:
    backendAdded: Storage Backend hinzugefügt
    backendUpdated: Storage Backend aktualisiert
    backendDeleted: Storage Backend gelöscht
    connectionOk: Verbindung erfolgreich
  errors:
    loadFailed: Backends konnten nicht geladen werden
    addFailed: Backend konnte nicht hinzugefügt werden
    updateFailed: Backend konnte nicht aktualisiert werden
    deleteFailed: Backend konnte nicht gelöscht werden
    testFailed: Verbindungstest fehlgeschlagen
en:
  title: Storage
  description: Manage S3-compatible storage backends for extensions
  addBackend:
    title: Add Storage Backend
  editBackend:
    title: Edit Storage Backend
  backends:
    title: Storage Backends
    description: S3-compatible storage services for file uploads
    noBackends: No storage backends configured
    noBackendsHint: Add an S3-compatible backend to store files
    enabled: Enabled
    disabled: Disabled
  form:
    connecting: Testing connection...
    name:
      label: Name
      placeholder: My S3 Storage
    endpoint:
      label: Endpoint
      description: Only for S3-compatible services like MinIO, Cloudflare R2, etc.
      placeholder: https://s3.example.com
    bucket:
      label: Bucket
      placeholder: my-bucket
    region:
      label: Region
      placeholder: eu-central-1
    accessKeyId:
      label: Access Key ID
      placeholder: AKIAIOSFODNN7EXAMPLE
      keepExisting: Leave empty to keep existing key
    secretAccessKey:
      label: Secret Access Key
      placeholder: "********"
      keepExisting: Leave empty to keep existing secret
    pathStyle:
      label: Use path-style URLs
      description: Enable for MinIO and other S3-compatible services
  actions:
    add: Add
    save: Save
    cancel: Cancel
    test: Test
    delete: Delete
  deleteBackend:
    title: Delete Storage Backend
    description: Do you really want to delete the backend "{name}"? Extensions will no longer be able to access this backend.
  success:
    backendAdded: Storage backend added
    backendUpdated: Storage backend updated
    backendDeleted: Storage backend deleted
    connectionOk: Connection successful
  errors:
    loadFailed: Failed to load backends
    addFailed: Failed to add backend
    updateFailed: Failed to update backend
    deleteFailed: Failed to delete backend
    testFailed: Connection test failed
</i18n>
