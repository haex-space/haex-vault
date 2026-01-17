<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <!-- Add Backend Form -->
    <UCard v-if="showAddBackendForm" class="relative">
      <!-- Loading Overlay -->
      <div
        v-if="isLoading"
        class="absolute inset-0 z-10 flex items-center justify-center bg-default/80 backdrop-blur-sm rounded-lg"
      >
        <div class="flex flex-col items-center gap-3">
          <div class="loading loading-spinner loading-lg text-primary" />
          <span class="text-sm text-muted">
            {{ t('addBackend.connecting') }}
          </span>
        </div>
      </div>

      <template #header>
        <div class="flex justify-between px-1">
          <h3 class="text-lg font-semibold">
            {{ t('addBackend.title') }}
          </h3>

          <UiButton
            icon="mdi-close"
            variant="ghost"
            color="neutral"
            :disabled="isLoading"
            @click="showAddBackendForm = false"
          />
        </div>
      </template>

      <form class="space-y-4" @submit.prevent="onAddBackendAsync">
        <UFormField :label="t('form.name.label')" required>
          <UiInput
            v-model="newBackend.name"
            :placeholder="t('form.name.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.endpoint.label')" :description="t('form.endpoint.description')">
          <UiInput
            v-model="newBackend.endpoint"
            :placeholder="t('form.endpoint.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.bucket.label')" required>
          <UiInput
            v-model="newBackend.bucket"
            :placeholder="t('form.bucket.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.region.label')" required>
          <UiInput
            v-model="newBackend.region"
            :placeholder="t('form.region.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.accessKeyId.label')" required>
          <UiInput
            v-model="newBackend.accessKeyId"
            :placeholder="t('form.accessKeyId.placeholder')"
          />
        </UFormField>

        <UFormField :label="t('form.secretAccessKey.label')" required>
          <UiInput
            v-model="newBackend.secretAccessKey"
            type="password"
            :placeholder="t('form.secretAccessKey.placeholder')"
          />
        </UFormField>

        <UFormField>
          <UCheckbox
            v-model="newBackend.pathStyle"
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
            @click="cancelAddBackend"
          >
            {{ t('actions.cancel') }}
          </UiButton>

          <UiButton
            icon="mdi-plus"
            :disabled="isLoading || !isFormValid"
            @click="onAddBackendAsync"
          >
            <span class="hidden @sm:inline">
              {{ t('actions.add') }}
            </span>
          </UiButton>
        </div>
      </template>
    </UCard>

    <!-- Storage Backends List -->
    <UCard v-if="!showAddBackendForm || storageBackends.length">
      <template #header>
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-semibold">{{ t('backends.title') }}</h3>
            <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
              {{ t('backends.description') }}
            </p>
          </div>
          <UiButton
            v-if="!showAddBackendForm"
            icon="i-lucide-plus"
            @click="showAddBackendForm = true"
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

const { t } = useI18n()
const { add } = useToast()

// State
const storageBackends = ref<StorageBackendInfo[]>([])
const showAddBackendForm = ref(false)
const isLoading = ref(false)
const testingBackendId = ref<string | null>(null)
const showDeleteDialog = ref(false)
const backendToDelete = ref<StorageBackendInfo | null>(null)

const newBackend = reactive({
  name: '',
  endpoint: '',
  bucket: '',
  region: 'auto',
  accessKeyId: '',
  secretAccessKey: '',
  pathStyle: false,
})

const isFormValid = computed(() => {
  return (
    newBackend.name.trim() !== '' &&
    newBackend.bucket.trim() !== '' &&
    newBackend.region.trim() !== '' &&
    newBackend.accessKeyId.trim() !== '' &&
    newBackend.secretAccessKey.trim() !== ''
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
  newBackend.name = ''
  newBackend.endpoint = ''
  newBackend.bucket = ''
  newBackend.region = 'auto'
  newBackend.accessKeyId = ''
  newBackend.secretAccessKey = ''
  newBackend.pathStyle = false
}

const cancelAddBackend = () => {
  showAddBackendForm.value = false
  resetForm()
}

const onAddBackendAsync = async () => {
  if (!isFormValid.value) return

  isLoading.value = true

  try {
    const config: Record<string, unknown> = {
      bucket: newBackend.bucket,
      region: newBackend.region,
      accessKeyId: newBackend.accessKeyId,
      secretAccessKey: newBackend.secretAccessKey,
    }

    if (newBackend.endpoint.trim()) {
      config.endpoint = newBackend.endpoint.trim()
    }

    if (newBackend.pathStyle) {
      config.pathStyle = true
    }

    const request: AddStorageBackendRequest = {
      name: newBackend.name,
      type: 's3',
      config,
    }

    await invoke('remote_storage_add_backend', { request })

    add({
      title: t('success.backendAdded'),
      color: 'success',
    })

    await loadBackendsAsync()
    cancelAddBackend()
  } catch (error) {
    console.error('Failed to add storage backend:', error)
    add({
      title: t('errors.addFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isLoading.value = false
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
    connecting: Verbindung wird getestet...
  backends:
    title: Storage Backends
    description: S3-kompatible Speicherdienste für Datei-Uploads
    noBackends: Keine Storage Backends konfiguriert
    noBackendsHint: Füge ein S3-kompatibles Backend hinzu, um Dateien zu speichern
    enabled: Aktiviert
    disabled: Deaktiviert
  form:
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
    secretAccessKey:
      label: Secret Access Key
      placeholder: "********"
    pathStyle:
      label: Path-Style URLs verwenden
      description: Aktivieren für MinIO und andere S3-kompatible Dienste
  actions:
    add: Hinzufügen
    cancel: Abbrechen
    test: Testen
    delete: Löschen
  deleteBackend:
    title: Storage Backend löschen
    description: Möchtest du das Backend "{name}" wirklich löschen? Erweiterungen können dann nicht mehr auf dieses Backend zugreifen.
  success:
    backendAdded: Storage Backend hinzugefügt
    backendDeleted: Storage Backend gelöscht
    connectionOk: Verbindung erfolgreich
  errors:
    loadFailed: Backends konnten nicht geladen werden
    addFailed: Backend konnte nicht hinzugefügt werden
    deleteFailed: Backend konnte nicht gelöscht werden
    testFailed: Verbindungstest fehlgeschlagen
en:
  title: Storage
  description: Manage S3-compatible storage backends for extensions
  addBackend:
    title: Add Storage Backend
    connecting: Testing connection...
  backends:
    title: Storage Backends
    description: S3-compatible storage services for file uploads
    noBackends: No storage backends configured
    noBackendsHint: Add an S3-compatible backend to store files
    enabled: Enabled
    disabled: Disabled
  form:
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
    secretAccessKey:
      label: Secret Access Key
      placeholder: "********"
    pathStyle:
      label: Use path-style URLs
      description: Enable for MinIO and other S3-compatible services
  actions:
    add: Add
    cancel: Cancel
    test: Test
    delete: Delete
  deleteBackend:
    title: Delete Storage Backend
    description: Do you really want to delete the backend "{name}"? Extensions will no longer be able to access this backend.
  success:
    backendAdded: Storage backend added
    backendDeleted: Storage backend deleted
    connectionOk: Connection successful
  errors:
    loadFailed: Failed to load backends
    addFailed: Failed to add backend
    deleteFailed: Failed to delete backend
    testFailed: Connection test failed
</i18n>
