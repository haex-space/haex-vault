<template>
  <HaexSystemSettingsLayout
    :description="t('extensionDetails')"
    show-back
    @back="emit('back')"
  >
    <template #title>
      <span class="truncate">{{ extension.name }}</span>
      <UBadge
        v-if="extension.devServerUrl"
        color="warning"
        variant="subtle"
        class="ml-2"
      >
        {{ t('devExtension') }}
      </UBadge>
    </template>
      <!-- Extension Info Section -->
      <HaexSystemSettingsLayoutSection
        :title="t('info')"
        default-open
      >
        <template #actions>
          <UiButton
            v-if="hasUpdate && !extension.devServerUrl"
            :label="t('update')"
            icon="i-heroicons-arrow-up-circle"
            color="warning"
            :loading="isUpdating"
            @click="() => void handleUpdateAsync()"
          />
          <UiButton
            :label="t('remove')"
            icon="i-heroicons-trash"
            color="error"
            variant="outline"
            @click="confirmRemove"
          />
          <UiButton
            :label="t('open')"
            icon="i-heroicons-play"
            @click="openExtensionAsync"
          />
        </template>

        <div class="space-y-3">
          <!-- Icon and Info Row -->
          <div class="flex items-start gap-3">
            <div
              class="w-16 h-16 shrink-0 rounded-lg bg-elevated flex items-center justify-center overflow-hidden"
            >
              <HaexIcon
                :name="extension.iconUrl || 'i-heroicons-puzzle-piece'"
                class="w-full h-full object-contain"
              />
            </div>

            <div class="flex-1 min-w-0 text-sm space-y-1">
              <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
                <span class="font-medium">{{ t('version') }}:</span>
                <span>{{ extension.version }}</span>
                <!-- Loading indicator while checking for updates -->
                <UIcon
                  v-if="isCheckingUpdate"
                  name="i-heroicons-arrow-path"
                  class="w-4 h-4 animate-spin text-gray-400"
                />
                <!-- Latest version badge -->
                <UBadge
                  v-if="latestAvailableVersion && !isCheckingUpdate"
                  :color="hasUpdate ? 'warning' : 'success'"
                  variant="subtle"
                  size="md"
                >
                  {{ hasUpdate ? t('latestVersion', { version: latestAvailableVersion }) : t('upToDate') }}
                </UBadge>
              </div>
              <div v-if="extension.author">
                <span class="font-medium">{{ t('author') }}:</span>
                {{ extension.author }}
              </div>
            </div>
          </div>

          <div
            v-if="extension.description"
            class="text-sm text-gray-600 dark:text-gray-300"
          >
            {{ extension.description }}
          </div>
        </div>
      </HaexSystemSettingsLayoutSection>

      <!-- Settings Section -->
      <HaexSystemSettingsLayoutSection
        :title="t('settings')"
      >
        <UiListContainer>
          <UiListItem>
            <div>
              <div class="font-medium text-sm">{{ t('displayMode') }}</div>
              <div class="text-xs text-gray-500 dark:text-gray-400">
                {{ t('displayModeDescription') }}
              </div>
            </div>
            <template #actions>
              <USelectMenu
                v-model="selectedDisplayMode"
                :items="displayModeOptions"
                class="w-40"
                :search-input="false"
                @update:model-value="updateDisplayModeAsync"
              />
            </template>
          </UiListItem>

          <UiListItem>
            <div>
              <div class="font-medium text-sm">{{ t('singleInstance') }}</div>
              <div class="text-xs text-gray-500 dark:text-gray-400">
                {{ t('singleInstanceDescription') }}
              </div>
            </div>
            <template #actions>
              <span class="text-sm">{{
                extension.singleInstance ? t('yes') : t('no')
              }}</span>
            </template>
          </UiListItem>

          <UiListItem>
            <div>
              <div class="font-medium text-sm">{{ t('id') }}</div>
              <div class="text-xs text-gray-500 dark:text-gray-400">
                {{ t('idDescription') }}
              </div>
            </div>
            <template #actions>
              <code
                class="text-xs bg-muted px-2 py-1 rounded break-all max-w-[50%] text-right"
              >
                {{ extension.id }}
              </code>
            </template>
          </UiListItem>

          <UiListItem v-if="extension.homepage">
            <div>
              <div class="font-medium text-sm">{{ t('homepage') }}</div>
            </div>
            <template #actions>
              <a
                :href="extension.homepage"
                target="_blank"
                class="text-sm text-primary hover:underline truncate max-w-[50%]"
              >
                {{ extension.homepage }}
              </a>
            </template>
          </UiListItem>
        </UiListContainer>
      </HaexSystemSettingsLayoutSection>

      <!-- Permissions Section -->
      <HaexSystemSettingsLayoutSection
        :title="t('permissions')"
      >
        <template #actions>
          <UiButton
            v-if="hasAnyPermissions"
            :label="t('savePermissions')"
            :loading="savingPermissions"
            :disabled="!hasPermissionChanges"
            @click="savePermissionsAsync"
          />
        </template>

        <div
          v-if="loadingPermissions"
          class="flex justify-center py-4"
        >
          <UIcon
            name="i-heroicons-arrow-path"
            class="w-6 h-6 animate-spin text-primary"
          />
        </div>

        <div
          v-else
          class="space-y-4"
        >
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
              <HaexExtensionPermissionList
                v-model="editablePermissions.shell"
              />
            </template>
            <template #filesync>
              <HaexExtensionPermissionList
                v-model="editablePermissions.filesync"
              />
            </template>
          </UAccordion>

          <HaexSystemSettingsLayoutEmpty
            v-if="!hasAnyPermissions"
            :message="t('noPermissions')"
            icon="i-heroicons-shield-check"
          />
        </div>
      </HaexSystemSettingsLayoutSection>

      <!-- Limits Section -->
      <HaexExtensionLimitsCard :extension-id="extension.id" />

      <!-- Session Permissions Section -->
      <HaexSystemSettingsLayoutSection
        v-if="sessionPermissions.length > 0"
        :title="t('sessionPermissions')"
        :description="t('sessionPermissionsDescription')"
      >
        <UiListContainer>
          <UiListItem
            v-for="permission in sessionPermissions"
            :key="`${permission.resourceType}-${permission.target}`"
          >
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <UIcon
                  :name="getPermissionIcon(permission.resourceType)"
                  class="w-4 h-4"
                />
                <span class="font-medium">{{ t(`permissionTypes.${getPermissionTypeKey(permission.resourceType)}`) }}</span>
                <UBadge
                  :color="permission.status === 'granted' ? 'success' : 'error'"
                  variant="subtle"
                >
                  {{ permission.status === 'granted' ? t('sessionGranted') : t('sessionDenied') }}
                </UBadge>
              </div>
              <div class="text-sm text-gray-500 dark:text-gray-400 mt-1 font-mono truncate">
                {{ permission.target }}
              </div>
              <div class="text-xs text-gray-400 dark:text-gray-500 mt-1">
                {{ t('sessionHint') }}
              </div>
            </div>
            <template #actions>
              <UButton
                color="error"
                variant="ghost"
                :loading="revokingSessionPermission === `${permission.resourceType}-${permission.target}`"
                @click="revokeSessionPermissionAsync(permission)"
              >
                <UIcon name="i-heroicons-x-mark" class="w-4 h-4" />
                {{ t('revoke') }}
              </UButton>
            </template>
          </UiListItem>
        </UiListContainer>
      </HaexSystemSettingsLayoutSection>

    <!-- Remove Confirmation Dialog -->
    <HaexExtensionDialogRemove
      v-model:open="removeDialogOpen"
      :extension="extension"
      @confirm="handleRemoveAsync"
    />

    <!-- Update Confirmation Dialog -->
    <HaexExtensionDialogReinstall
      v-model:open="updateDialogOpen"
      v-model:preview="updatePreview"
      mode="update"
      :icon-url="extension.iconUrl"
      @confirm="handleUpdateConfirmAsync"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { useMarketplace } from '@haex-space/marketplace-sdk/vue'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { PermissionEntry } from '~~/src-tauri/bindings/PermissionEntry'
import type { DisplayMode } from '~~/src-tauri/bindings/DisplayMode'
import type { ExtensionPermission } from '~~/src-tauri/bindings/ExtensionPermission'
import { useExtensionUpdate } from '~/composables/useExtensionUpdate'

interface ExtensionPermissionsEditable {
  database?: PermissionEntry[] | null
  filesystem?: PermissionEntry[] | null
  http?: PermissionEntry[] | null
  shell?: PermissionEntry[] | null
  filesync?: PermissionEntry[] | null
}

const props = defineProps<{
  extension: IHaexSpaceExtension
  /** Latest version available from marketplace (optional) */
  latestVersion?: string | null
}>()

const emit = defineEmits<{
  back: []
  removed: []
}>()

const { t } = useI18n()
const { add } = useToast()
const extensionsStore = useExtensionsStore()
const marketplace = useMarketplace()
const windowManager = useWindowManagerStore()

// Update state
const isCheckingUpdate = ref(false)
const marketplaceVersion = ref<string | null>(null)

// Latest available version (from props or marketplace)
const latestAvailableVersion = computed(() => {
  return props.latestVersion || marketplaceVersion.value
})

// Check if update is available
const hasUpdate = computed(() => {
  const latest = latestAvailableVersion.value
  if (!latest || !props.extension.version) return false
  return extensionsStore.compareVersions(props.extension.version, latest) < 0
})

// Handle update button click - downloads and shows update dialog
const handleUpdateAsync = () => downloadForUpdateAsync(props.extension)

// Handle update confirmation - go back to list after successful update
const handleUpdateConfirmAsync = async () => {
  if (await confirmUpdateAsync()) {
    emit('removed')
  }
}

// Fetch latest version from marketplace on mount
const fetchLatestVersionAsync = async () => {
  // Skip for dev extensions or if already provided via props
  if (props.extension.devServerUrl || props.latestVersion) return

  isCheckingUpdate.value = true
  try {
    await marketplace.fetchExtensions({
      search: props.extension.name,
      limit: 10,
    })

    // Try to find exact match by name
    const found = marketplace.extensions.value.find(
      (ext) => ext.name === props.extension.name,
    )

    // versions is an array, first entry is the latest version
    const latestVer = (found as { versions?: { version?: string }[] | null })
      ?.versions?.[0]?.version

    if (latestVer) {
      marketplaceVersion.value = latestVer
    }
  } catch (error) {
    // Silently ignore - marketplace may be unavailable
    console.warn('Could not fetch latest version from marketplace:', error)
  } finally {
    isCheckingUpdate.value = false
  }
}

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

const getDisplayModeOption = (
  mode: DisplayMode | null | undefined,
): IDisplayModeOption => {
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

const updateDisplayModeAsync = async (
  option: IDisplayModeOption | undefined,
) => {
  if (!option) return

  try {
    await extensionsStore.updateDisplayModeAsync(
      props.extension.id,
      option.value,
    )
    add({ description: t('displayModeSaved'), color: 'success' })
  } catch (error) {
    console.error('Error updating display mode:', error)
    add({ description: t('displayModeSaveError'), color: 'error' })
    // Reset to previous value
    selectedDisplayMode.value = getDisplayModeOption(
      props.extension.displayMode,
    )
  }
}

// Permissions
const loadingPermissions = ref(true)
const savingPermissions = ref(false)
const originalPermissions = ref<ExtensionPermissionsEditable>({
  database: null,
  filesystem: null,
  http: null,
  shell: null,
  filesync: null,
})
const editablePermissions = ref<ExtensionPermissionsEditable>({
  database: null,
  filesystem: null,
  http: null,
  shell: null,
  filesync: null,
})

// Session permissions (in-memory, not persisted)
const sessionPermissions = ref<ExtensionPermission[]>([])
const revokingSessionPermission = ref<string | null>(null)


// Remove dialog
const removeDialogOpen = ref(false)

// Update composable
const {
  isDownloading: isUpdating,
  updateDialogOpen,
  updatePreview,
  downloadForUpdateAsync,
  confirmUpdateAsync,
} = useExtensionUpdate()

const hasAnyPermissions = computed(() => {
  return (
    (editablePermissions.value.database?.length ?? 0) > 0 ||
    (editablePermissions.value.filesystem?.length ?? 0) > 0 ||
    (editablePermissions.value.http?.length ?? 0) > 0 ||
    (editablePermissions.value.shell?.length ?? 0) > 0 ||
    (editablePermissions.value.filesync?.length ?? 0) > 0
  )
})

const hasPermissionChanges = computed(() => {
  const compareArrays = (a: PermissionEntry[] | null | undefined, b: PermissionEntry[] | null | undefined) => {
    if (!a && !b) return true
    if (!a || !b) return false
    if (a.length !== b.length) return false
    return a.every((item, index) => {
      const other = b[index]
      return item.target === other?.target && item.status === other?.status
    })
  }

  return (
    !compareArrays(editablePermissions.value.database, originalPermissions.value.database) ||
    !compareArrays(editablePermissions.value.filesystem, originalPermissions.value.filesystem) ||
    !compareArrays(editablePermissions.value.http, originalPermissions.value.http) ||
    !compareArrays(editablePermissions.value.shell, originalPermissions.value.shell) ||
    !compareArrays(editablePermissions.value.filesync, originalPermissions.value.filesync)
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

  if ((editablePermissions.value.filesync?.length ?? 0) > 0) {
    items.push({
      label: t('permissionTypes.filesync'),
      icon: 'i-heroicons-cloud-arrow-up',
      slot: 'filesync',
    })
  }

  return items
})

// Helper functions for session permissions
const getPermissionIcon = (resourceType: string): string => {
  const icons: Record<string, string> = {
    db: 'i-heroicons-circle-stack',
    fs: 'i-heroicons-folder',
    web: 'i-heroicons-globe-alt',
    shell: 'i-heroicons-command-line',
    filesync: 'i-heroicons-cloud-arrow-up',
  }
  return icons[resourceType] || 'i-heroicons-shield-check'
}

const getPermissionTypeKey = (resourceType: string): string => {
  const keys: Record<string, string> = {
    db: 'database',
    fs: 'filesystem',
    web: 'http',
    shell: 'shell',
    filesync: 'filesync',
  }
  return keys[resourceType] || resourceType
}

const loadSessionPermissionsAsync = async () => {
  try {
    sessionPermissions.value = await invoke<ExtensionPermission[]>(
      'get_extension_session_permissions',
      { extensionId: props.extension.id },
    )
  } catch (error) {
    console.error('Error loading session permissions:', error)
    sessionPermissions.value = []
  }
}

const revokeSessionPermissionAsync = async (permission: ExtensionPermission) => {
  const key = `${permission.resourceType}-${permission.target}`
  revokingSessionPermission.value = key
  try {
    await invoke('remove_extension_session_permission', {
      extensionId: props.extension.id,
      resourceType: permission.resourceType,
      target: permission.target,
    })
    add({ description: t('sessionPermissionRevoked'), color: 'success' })
    await loadSessionPermissionsAsync()
  } catch (error) {
    console.error('Error revoking session permission:', error)
    add({ description: t('sessionPermissionRevokeError'), color: 'error' })
  } finally {
    revokingSessionPermission.value = null
  }
}

const loadPermissionsAsync = async () => {
  loadingPermissions.value = true
  try {
    const permissions = await invoke<ExtensionPermissionsEditable>(
      'get_extension_permissions',
      {
        extensionId: props.extension.id,
      },
    )
    // Store original for comparison
    originalPermissions.value = JSON.parse(JSON.stringify(permissions))
    editablePermissions.value = permissions
  } catch (error) {
    console.error('Error loading permissions:', error)
    editablePermissions.value = {
      database: [],
      filesystem: [],
      http: [],
      shell: [],
    }
    originalPermissions.value = {
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
    // Update original after successful save
    originalPermissions.value = JSON.parse(JSON.stringify(editablePermissions.value))
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

const openExtensionAsync = async () => {
  try {
    await windowManager.openWindowAsync({
      type: 'extension',
      sourceId: props.extension.id,
    })
  } catch (error) {
    console.error('Error opening extension:', error)
    add({ description: t('openError'), color: 'error' })
  }
}

const handleRemoveAsync = async (deleteMode: 'device' | 'complete') => {
  try {
    await extensionsStore.uninstallExtensionAsync(
      props.extension.id,
      deleteMode,
    )
    add({ description: t('removeSuccess'), color: 'success' })
    emit('removed')
  } catch (error) {
    console.error('Error removing extension:', error)
    add({ description: t('removeError'), color: 'error' })
  }
}

onMounted(async () => {
  await Promise.all([loadPermissionsAsync(), loadSessionPermissionsAsync(), fetchLatestVersionAsync()])
})
</script>

<i18n lang="yaml">
de:
  extensionDetails: Erweiterungsdetails und Konfiguration
  devExtension: Entwicklung
  update: Aktualisieren
  open: Öffnen
  openError: Fehler beim Öffnen der Erweiterung
  info: Informationen
  version: Version
  latestVersion: 'Neu: v{version}'
  upToDate: Aktuell
  updateAvailable: 'Update auf v{version}'
  author: Autor
  homepage: Homepage
  id: ID
  idDescription: Eindeutige Kennung der Erweiterung
  singleInstance: Einzelinstanz
  singleInstanceDescription: Ob nur eine Instanz gleichzeitig laufen kann.
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
    filesync: Dateisynchronisierung
  noPermissions: Diese Erweiterung hat keine Berechtigungen.
  savePermissions: Berechtigungen speichern
  dangerZone: Gefahrenzone
  removeExtension: Erweiterung entfernen
  removeDevExtension: Entwicklungserweiterung entfernen
  removeWarning: Diese Aktion kann nicht rückgängig gemacht werden.
  removeDevWarning: Die Erweiterung wird aus der Liste entfernt. Du kannst sie jederzeit erneut verbinden.
  remove: Entfernen
  permissionsLoadError: Fehler beim Laden der Berechtigungen
  permissionsSaved: Berechtigungen gespeichert
  permissionsSaveError: Fehler beim Speichern der Berechtigungen
  sessionPermissions: Temporäre Berechtigungen (diese Sitzung)
  sessionPermissionsDescription: Diese Berechtigungen wurden für diese Sitzung erteilt oder verweigert und werden beim Neustart von haex-vault entfernt.
  sessionGranted: Erlaubt
  sessionDenied: Verweigert
  sessionHint: Wird beim Neustart von haex-vault entfernt
  sessionPermissionRevoked: Temporäre Berechtigung wurde widerrufen
  sessionPermissionRevokeError: Fehler beim Widerrufen der Berechtigung
  revoke: Widerrufen
  removeSuccess: Erweiterung erfolgreich entfernt
  removeError: Fehler beim Entfernen der Erweiterung
en:
  extensionDetails: Extension details and configuration
  devExtension: Development
  update: Update
  open: Open
  openError: Error opening extension
  info: Information
  version: Version
  latestVersion: 'New: v{version}'
  upToDate: Up to date
  updateAvailable: 'Update to v{version}'
  author: Author
  homepage: Homepage
  id: ID
  idDescription: Unique identifier of the extension
  singleInstance: Single Instance
  singleInstanceDescription: Whether only one instance can run at a time.
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
    filesync: File Sync
  noPermissions: This extension has no permissions.
  savePermissions: Save Permissions
  dangerZone: Danger Zone
  removeExtension: Remove Extension
  removeDevExtension: Remove Development Extension
  removeWarning: This action cannot be undone.
  removeDevWarning: The extension will be removed from the list. You can reconnect it at any time.
  remove: Remove
  permissionsLoadError: Error loading permissions
  permissionsSaved: Permissions saved
  permissionsSaveError: Error saving permissions
  sessionPermissions: Temporary Permissions (this session)
  sessionPermissionsDescription: These permissions were granted or denied for this session and will be removed when haex-vault restarts.
  sessionGranted: Allowed
  sessionDenied: Denied
  sessionHint: Will be removed when haex-vault restarts
  sessionPermissionRevoked: Temporary permission revoked
  sessionPermissionRevokeError: Error revoking permission
  revoke: Revoke
  removeSuccess: Extension successfully removed
  removeError: Error removing extension
</i18n>
