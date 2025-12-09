<template>
  <div>
    <!-- Header with back button -->
    <div class="p-6 border-b border-base-content/10">
      <div class="flex items-center gap-3">
        <UiButton
          icon="i-heroicons-arrow-left"
          variant="ghost"
          size="sm"
          @click="emit('back')"
        />
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <h2 class="text-2xl font-bold truncate">
              {{ extension.name }}
            </h2>
            <UBadge
              v-if="extension.devServerUrl"
              color="warning"
              variant="subtle"
              size="xs"
            >
              {{ t('devExtension') }}
            </UBadge>
          </div>
          <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">
            {{ t('extensionDetails') }}
          </p>
        </div>
      </div>
    </div>

    <div class="p-6 space-y-6">
      <!-- Extension Info Section -->
      <div class="space-y-4">
        <h3 class="text-lg font-semibold">{{ t('info') }}</h3>

        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div class="flex items-start gap-3">
            <div
              class="w-16 h-16 shrink-0 rounded-lg bg-base-200 flex items-center justify-center overflow-hidden"
            >
              <HaexIcon
                :name="extension.icon || 'i-heroicons-puzzle-piece'"
                class="w-full h-full object-contain"
              />
            </div>

            <div class="flex-1 min-w-0">
              <div class="text-sm space-y-1">
                <div>
                  <span class="font-medium">{{ t('version') }}:</span>
                  {{ extension.version }}
                </div>
                <div v-if="extension.author">
                  <span class="font-medium">{{ t('author') }}:</span>
                  {{ extension.author }}
                </div>
                <div v-if="extension.homepage">
                  <span class="font-medium">{{ t('homepage') }}:</span>
                  <a
                    :href="extension.homepage"
                    target="_blank"
                    class="text-primary hover:underline break-all"
                  >
                    {{ extension.homepage }}
                  </a>
                </div>
              </div>
            </div>
          </div>

          <div class="text-sm space-y-1">
            <div>
              <span class="font-medium">{{ t('id') }}:</span>
              <code class="text-xs bg-base-200 px-1 py-0.5 rounded break-all">
                {{ extension.id }}
              </code>
            </div>
            <div>
              <span class="font-medium">{{ t('singleInstance') }}:</span>
              {{ extension.singleInstance ? t('yes') : t('no') }}
            </div>
          </div>
        </div>

        <div v-if="extension.description" class="text-sm text-gray-600 dark:text-gray-300">
          {{ extension.description }}
        </div>
      </div>

      <!-- Settings Section -->
      <div class="space-y-4">
        <h3 class="text-lg font-semibold">{{ t('settings') }}</h3>

        <div class="space-y-3">
          <div class="flex items-center justify-between gap-4">
            <div>
              <div class="font-medium text-sm">{{ t('displayMode') }}</div>
              <div class="text-xs text-gray-500 dark:text-gray-400">
                {{ t('displayModeDescription') }}
              </div>
            </div>
            <USelectMenu
              v-model="selectedDisplayMode"
              :items="displayModeOptions"
              class="w-40"
              :search-input="false"
              @update:model-value="updateDisplayModeAsync"
            />
          </div>
        </div>
      </div>

      <!-- Permissions Section -->
      <div class="space-y-4">
        <h3 class="text-lg font-semibold">{{ t('permissions') }}</h3>

        <div v-if="loadingPermissions" class="flex justify-center py-4">
          <UIcon
            name="i-heroicons-arrow-path"
            class="w-6 h-6 animate-spin text-primary"
          />
        </div>

        <div v-else class="space-y-4">
          <UAccordion
            v-if="hasAnyPermissions"
            :items="permissionAccordionItems"
            :ui="{ root: 'flex flex-col gap-2' }"
          >
            <template #database>
              <HaexExtensionPermissionList
                v-model="editablePermissions.database"
              />
            </template>
            <template #filesystem>
              <HaexExtensionPermissionList
                v-model="editablePermissions.filesystem"
              />
            </template>
            <template #http>
              <HaexExtensionPermissionList v-model="editablePermissions.http" />
            </template>
            <template #shell>
              <HaexExtensionPermissionList v-model="editablePermissions.shell" />
            </template>
          </UAccordion>

          <div
            v-if="!hasAnyPermissions"
            class="text-center py-4 text-gray-500 dark:text-gray-400 bg-base-200 rounded-lg"
          >
            {{ t('noPermissions') }}
          </div>

          <div v-if="hasAnyPermissions" class="flex justify-end">
            <UiButton
              :label="t('savePermissions')"
              :loading="savingPermissions"
              size="sm"
              @click="savePermissionsAsync"
            />
          </div>
        </div>
      </div>

      <!-- Danger Zone -->
      <div class="space-y-4 pt-4 border-t border-error/20">
        <h3 class="text-lg font-semibold text-error">{{ t('dangerZone') }}</h3>

        <div class="flex items-center justify-between p-4 rounded-lg border border-error/30 bg-error/5">
          <div>
            <div class="font-medium">
              {{ extension.devServerUrl ? t('removeDevExtension') : t('removeExtension') }}
            </div>
            <div class="text-sm text-gray-500 dark:text-gray-400">
              {{ extension.devServerUrl ? t('removeDevWarning') : t('removeWarning') }}
            </div>
          </div>
          <UiButton
            :label="t('remove')"
            color="error"
            variant="outline"
            size="sm"
            @click="confirmRemove"
          />
        </div>
      </div>
    </div>

    <!-- Remove Confirmation Dialog -->
    <UModal v-model:open="removeDialogOpen">
      <template #content>
        <UCard>
          <template #header>
            <h3 class="text-lg font-semibold text-error">
              {{ t('confirmRemove') }}
            </h3>
          </template>

          <p>
            {{ t('removeConfirmText', { name: extension.name }) }}
          </p>

          <template #footer>
            <div class="flex justify-end gap-2">
              <UiButton
                :label="t('cancel')"
                variant="outline"
                @click="removeDialogOpen = false"
              />
              <UiButton
                :label="t('remove')"
                color="error"
                :loading="removing"
                @click="handleRemoveAsync"
              />
            </div>
          </template>
        </UCard>
      </template>
    </UModal>
  </div>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { ExtensionInfoResponse } from '~~/src-tauri/bindings/ExtensionInfoResponse'
import type { PermissionEntry } from '~~/src-tauri/bindings/PermissionEntry'
import type { DisplayMode } from '~~/src-tauri/bindings/DisplayMode'

interface ExtensionPermissionsEditable {
  database?: PermissionEntry[] | null
  filesystem?: PermissionEntry[] | null
  http?: PermissionEntry[] | null
  shell?: PermissionEntry[] | null
}

const props = defineProps<{
  extension: ExtensionInfoResponse
}>()

const emit = defineEmits<{
  back: []
  removed: []
}>()

const { t } = useI18n()
const { add } = useToast()
const extensionsStore = useExtensionsStore()

// Display Mode
interface IDisplayModeOption {
  value: DisplayMode
  label: string
}

const displayModeOptions = computed<IDisplayModeOption[]>(() => [
  { value: 'auto', label: t('displayModes.auto') },
  { value: 'window', label: t('displayModes.window') },
  { value: 'iframe', label: t('displayModes.iframe') },
])

const getDisplayModeOption = (mode: DisplayMode | null | undefined): IDisplayModeOption => {
  return (
    displayModeOptions.value.find((opt) => opt.value === (mode || 'auto')) || {
      value: 'auto',
      label: t('displayModes.auto'),
    }
  )
}

const selectedDisplayMode = ref<IDisplayModeOption>(
  getDisplayModeOption(props.extension.displayMode),
)

const updateDisplayModeAsync = async (option: IDisplayModeOption | undefined) => {
  if (!option) return

  try {
    await extensionsStore.updateDisplayModeAsync(props.extension.id, option.value)
    add({ description: t('displayModeSaved'), color: 'success' })
  } catch (error) {
    console.error('Error updating display mode:', error)
    add({ description: t('displayModeSaveError'), color: 'error' })
    // Reset to previous value
    selectedDisplayMode.value = getDisplayModeOption(props.extension.displayMode)
  }
}

// Permissions
const loadingPermissions = ref(true)
const savingPermissions = ref(false)
const editablePermissions = ref<ExtensionPermissionsEditable>({
  database: null,
  filesystem: null,
  http: null,
  shell: null,
})

// Remove dialog
const removeDialogOpen = ref(false)
const removing = ref(false)

const hasAnyPermissions = computed(() => {
  return (
    (editablePermissions.value.database?.length ?? 0) > 0 ||
    (editablePermissions.value.filesystem?.length ?? 0) > 0 ||
    (editablePermissions.value.http?.length ?? 0) > 0 ||
    (editablePermissions.value.shell?.length ?? 0) > 0
  )
})

const permissionAccordionItems = computed(() => {
  const items = []

  if ((editablePermissions.value.database?.length ?? 0) > 0) {
    items.push({
      label: t('permissionTypes.database'),
      icon: 'i-heroicons-circle-stack',
      slot: 'database',
      defaultOpen: true,
    })
  }

  if ((editablePermissions.value.filesystem?.length ?? 0) > 0) {
    items.push({
      label: t('permissionTypes.filesystem'),
      icon: 'i-heroicons-folder',
      slot: 'filesystem',
    })
  }

  if ((editablePermissions.value.http?.length ?? 0) > 0) {
    items.push({
      label: t('permissionTypes.http'),
      icon: 'i-heroicons-globe-alt',
      slot: 'http',
    })
  }

  if ((editablePermissions.value.shell?.length ?? 0) > 0) {
    items.push({
      label: t('permissionTypes.shell'),
      icon: 'i-heroicons-command-line',
      slot: 'shell',
    })
  }

  return items
})

const loadPermissionsAsync = async () => {
  loadingPermissions.value = true
  try {
    const permissions = await invoke<ExtensionPermissionsEditable>(
      'get_extension_permissions',
      {
        extensionId: props.extension.id,
      },
    )
    editablePermissions.value = permissions
  } catch (error) {
    console.error('Error loading permissions:', error)
    editablePermissions.value = {
      database: [],
      filesystem: [],
      http: [],
      shell: [],
    }
    add({ description: t('permissionsLoadError'), color: 'error' })
  } finally {
    loadingPermissions.value = false
  }
}

const savePermissionsAsync = async () => {
  savingPermissions.value = true
  try {
    await invoke('update_extension_permissions', {
      extensionId: props.extension.id,
      permissions: editablePermissions.value,
    })
    add({ description: t('permissionsSaved'), color: 'success' })
  } catch (error) {
    console.error('Error saving permissions:', error)
    add({ description: t('permissionsSaveError'), color: 'error' })
  } finally {
    savingPermissions.value = false
  }
}

const confirmRemove = () => {
  removeDialogOpen.value = true
}

const handleRemoveAsync = async () => {
  removing.value = true
  try {
    if (props.extension.devServerUrl) {
      // Dev extension - use removeDevExtensionAsync
      await extensionsStore.removeDevExtensionAsync(
        props.extension.publicKey,
        props.extension.name,
      )
    } else {
      // Regular extension - use removeExtensionAsync
      await extensionsStore.removeExtensionAsync(
        props.extension.publicKey,
        props.extension.name,
        props.extension.version,
      )
    }
    add({ description: t('removeSuccess'), color: 'success' })
    removeDialogOpen.value = false
    emit('removed')
  } catch (error) {
    console.error('Error removing extension:', error)
    add({ description: t('removeError'), color: 'error' })
  } finally {
    removing.value = false
  }
}

onMounted(async () => {
  await loadPermissionsAsync()
})
</script>

<i18n lang="yaml">
de:
  extensionDetails: Erweiterungsdetails und Konfiguration
  devExtension: Entwicklung
  info: Informationen
  version: Version
  author: Autor
  homepage: Homepage
  id: ID
  singleInstance: Einzelinstanz
  settings: Einstellungen
  displayMode: Anzeigemodus
  displayModeDescription: Bestimmt, wie die Erweiterung angezeigt wird.
  displayModes:
    auto: Automatisch
    window: Fenster
    iframe: Eingebettet
  displayModeSaved: Anzeigemodus gespeichert
  displayModeSaveError: Fehler beim Speichern des Anzeigemodus
  yes: Ja
  no: Nein
  permissions: Berechtigungen
  permissionTypes:
    database: Datenbank
    filesystem: Dateisystem
    http: Internet
    shell: Systembefehle
  noPermissions: Diese Erweiterung hat keine Berechtigungen.
  savePermissions: Berechtigungen speichern
  dangerZone: Gefahrenzone
  removeExtension: Erweiterung entfernen
  removeDevExtension: Entwicklungserweiterung entfernen
  removeWarning: Diese Aktion kann nicht rückgängig gemacht werden.
  removeDevWarning: Die Erweiterung wird aus der Liste entfernt. Du kannst sie jederzeit erneut verbinden.
  remove: Entfernen
  confirmRemove: Erweiterung entfernen
  removeConfirmText: Bist du sicher, dass du "{name}" entfernen möchtest? Alle Daten dieser Erweiterung werden gelöscht.
  cancel: Abbrechen
  devExtensionNote: Entwicklungs-Erweiterungen werden über den Dev-Server verwaltet und können hier nicht entfernt werden.
  permissionsLoadError: Fehler beim Laden der Berechtigungen
  permissionsSaved: Berechtigungen gespeichert
  permissionsSaveError: Fehler beim Speichern der Berechtigungen
  removeSuccess: Erweiterung erfolgreich entfernt
  removeError: Fehler beim Entfernen der Erweiterung
en:
  extensionDetails: Extension details and configuration
  devExtension: Development
  info: Information
  version: Version
  author: Author
  homepage: Homepage
  id: ID
  singleInstance: Single Instance
  settings: Settings
  displayMode: Display Mode
  displayModeDescription: Determines how the extension is displayed.
  displayModes:
    auto: Automatic
    window: Window
    iframe: Embedded
  displayModeSaved: Display mode saved
  displayModeSaveError: Error saving display mode
  yes: Yes
  no: No
  permissions: Permissions
  permissionTypes:
    database: Database
    filesystem: Filesystem
    http: Internet
    shell: Shell Commands
  noPermissions: This extension has no permissions.
  savePermissions: Save Permissions
  dangerZone: Danger Zone
  removeExtension: Remove Extension
  removeDevExtension: Remove Development Extension
  removeWarning: This action cannot be undone.
  removeDevWarning: The extension will be removed from the list. You can reconnect it at any time.
  remove: Remove
  confirmRemove: Remove Extension
  removeConfirmText: Are you sure you want to remove "{name}"? All data for this extension will be deleted.
  cancel: Cancel
  devExtensionNote: Development extensions are managed via the dev server and cannot be removed here.
  permissionsLoadError: Error loading permissions
  permissionsSaved: Permissions saved
  permissionsSaveError: Error saving permissions
  removeSuccess: Extension successfully removed
  removeError: Error removing extension
</i18n>
