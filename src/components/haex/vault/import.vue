<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <!-- Trigger Button -->
    <template #trigger>
      <UiButton
        :label="t('button.label')"
        :ui="{
          base: 'px-4 py-3',
        }"
        icon="mdi:folder-open-outline"
        size="lg"
        variant="outline"
        block
        :loading="isLoading"
        @click="onSelectFileAsync"
      />
    </template>

    <!-- Content -->
    <template #content>
      <div class="space-y-4">
        <!-- Dropzone / File selector -->
        <div
          v-if="!selectedPath"
          ref="dropZoneRef"
          class="border-2 border-dashed rounded-lg p-6 text-center cursor-pointer transition-colors"
          :class="[
            isOverDropZone
              ? 'border-primary bg-primary/10'
              : 'border-gray-300 dark:border-gray-600 hover:border-primary hover:bg-primary/5',
          ]"
          @click="onSelectFileAsync"
        >
          <Icon
            name="mdi:file-upload-outline"
            class="size-12 mx-auto mb-3 text-gray-400"
            :class="{ 'text-primary': isOverDropZone }"
          />
          <p class="text-sm text-gray-500 dark:text-gray-400">
            {{ t('dropzone.text') }}
          </p>
          <p class="text-xs text-gray-400 dark:text-gray-500 mt-1">
            {{ t('dropzone.hint') }}
          </p>
        </div>

        <!-- Selected file display -->
        <div
          v-else
          class="text-sm text-gray-500 dark:text-gray-400"
        >
          <div class="flex items-center justify-between">
            <span class="font-medium">{{ t('selectedFile') }}:</span>
            <UButton
              color="neutral"
              variant="ghost"
              size="xs"
              icon="mdi:close"
              @click="selectedPath = null"
            />
          </div>
          <p class="break-all mt-1 p-2 bg-gray-100 dark:bg-gray-800 rounded">
            {{ selectedPath }}
          </p>
        </div>

        <UForm
          v-if="selectedPath"
          :state="vault"
          class="w-full"
        >
          <UiInputPassword
            v-model="vault.password"
            v-model:errors="errors.password"
            :label="t('password.placeholder')"
            :schema="vaultSchema.password"
            :check="check"
            leading-icon="i-lucide-lock"
            size="lg"
            autofocus
            class="w-full"
            @keyup.enter="onImportAsync"
          />
        </UForm>

        <div class="flex gap-3 pt-4">
          <UButton
            color="neutral"
            variant="outline"
            block
            size="lg"
            @click="onClose"
          >
            {{ t('cancel') }}
          </UButton>
          <UButton
            color="primary"
            block
            size="lg"
            :disabled="!selectedPath"
            :loading="isLoading"
            @click="onImportAsync"
          >
            {{ t('import') }}
          </UButton>
        </div>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { useDropZone } from '@vueuse/core'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import { vaultSchema } from './schema'

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n({
  useScope: 'local',
})

const selectedPath = ref<string | null>(null)
const isLoading = ref(false)
const dropZoneRef = ref<HTMLElement | null>(null)

const vault = reactive({
  password: '',
})

const errors = reactive({
  password: [] as string[],
})

const check = ref(false)

const { add } = useToast()
const { syncLastVaultsAsync } = useLastVaultStore()

// Dropzone handling with VueUse
const onDrop = (files: File[] | null) => {
  if (!files || files.length === 0) return

  const file = files[0]

  // Check if it's a .db file
  if (!file?.name.endsWith('.db')) {
    add({
      color: 'error',
      description: t('error.invalidFile'),
    })
    return
  }

  // In Tauri, the File object has a path property
  const filePath = (file as File & { path?: string }).path

  if (filePath) {
    selectedPath.value = filePath
  } else {
    add({
      color: 'warning',
      description: t('error.dragNotSupported'),
    })
  }
}

const { isOverDropZone } = useDropZone(dropZoneRef, {
  onDrop,
  dataTypes: ['Files'],
})

const onSelectFileAsync = async () => {
  try {
    const file = await openFileDialog({
      multiple: false,
      filters: [
        {
          name: 'SQLite Database',
          extensions: ['db'],
        },
      ],
    })

    if (file) {
      selectedPath.value = file
      open.value = true
    }
  } catch (error) {
    console.error('Failed to open file dialog:', error)
    add({
      color: 'error',
      description: t('error.fileDialog'),
    })
  }
}

const onImportAsync = async () => {
  if (!selectedPath.value) return

  // Trigger validation
  check.value = true
  await nextTick()

  // Check for validation errors
  if (errors.password.length > 0) {
    return
  }

  isLoading.value = true

  try {
    // 1. Import the vault file (copy to vaults directory)
    const importedPath = await invoke<string>('import_vault', {
      sourcePath: selectedPath.value,
    })

    // 2. Try to open the vault with the provided password
    const { openAsync } = useVaultStore()
    const localePath = useLocalePath()

    const vaultId = await openAsync({
      path: importedPath,
      password: vault.password,
    })

    if (!vaultId) {
      add({
        color: 'error',
        description: t('error.open'),
      })
      return
    }

    // 3. Refresh the vault list
    await syncLastVaultsAsync()

    // 4. Close dialog and navigate to vault
    onClose()

    await navigateTo(
      localePath({
        name: 'desktop',
        params: {
          vaultId,
        },
      }),
    )

    // 5. Auto-login and start sync (non-blocking)
    const { autoLoginAndStartSyncAsync } = useVaultStore()
    autoLoginAndStartSyncAsync().catch((error) => {
      console.warn('[HaexSpace] Auto-login and sync start failed:', error)
    })
  } catch (error) {
    console.error('Failed to import vault:', error)

    const errorDetails =
      error && typeof error === 'object' && 'type' in error
        ? (error as {
            type?: string
            details?: { vaultName?: string; reason?: string }
          })
        : undefined

    if (errorDetails?.type === 'VaultAlreadyExists') {
      add({
        color: 'error',
        title: t('error.alreadyExists.title'),
        description: t('error.alreadyExists.description', {
          vaultName: errorDetails.details?.vaultName || '',
        }),
      })
    } else if (errorDetails?.details?.reason === 'file is not a database') {
      add({
        color: 'error',
        title: t('error.password.title'),
        description: t('error.password.description'),
      })
    } else {
      add({
        color: 'error',
        description:
          errorDetails?.details?.reason ||
          (typeof error === 'string' ? error : JSON.stringify(error)),
      })
    }
  } finally {
    isLoading.value = false
  }
}

const onClose = () => {
  selectedPath.value = null
  vault.password = ''
  errors.password = []
  check.value = false
  open.value = false
}

// Reset state when dialog closes
watch(open, (isOpen) => {
  if (!isOpen) {
    selectedPath.value = null
    vault.password = ''
    errors.password = []
    check.value = false
  }
})
</script>

<i18n lang="yaml">
de:
  button:
    label: Vault öffnen
  title: Vault importieren
  description: Wähle eine bestehende Vault-Datei aus
  dropzone:
    text: Datei hierher ziehen oder klicken zum Auswählen
    hint: Nur .db Dateien
  selectedFile: Ausgewählte Datei
  password:
    label: Passwort
    placeholder: Passwort eingeben
  import: Importieren & Öffnen
  cancel: Abbrechen
  error:
    fileDialog: Dateiauswahl konnte nicht geöffnet werden
    open: Vault konnte nicht geöffnet werden
    invalidFile: Bitte wähle eine .db Datei aus
    dragNotSupported: Drag & Drop wird nicht unterstützt. Bitte klicke zum Auswählen.
    alreadyExists:
      title: Vault existiert bereits
      description: Eine Vault mit dem Namen "{vaultName}" existiert bereits. Bitte lösche sie zuerst oder benenne die zu importierende Datei um.
    password:
      title: Vault konnte nicht geöffnet werden
      description: Bitte überprüfe das Passwort

en:
  button:
    label: Open Vault
  title: Import Vault
  description: Select an existing vault file
  dropzone:
    text: Drag file here or click to select
    hint: Only .db files
  selectedFile: Selected file
  password:
    label: Password
    placeholder: Enter password
  import: Import & Open
  cancel: Cancel
  error:
    fileDialog: Could not open file dialog
    open: Vault could not be opened
    invalidFile: Please select a .db file
    dragNotSupported: Drag & drop is not supported. Please click to select.
    alreadyExists:
      title: Vault already exists
      description: A vault named "{vaultName}" already exists. Please delete it first or rename the file to import.
    password:
      title: Vault could not be opened
      description: Please check your password
</i18n>
