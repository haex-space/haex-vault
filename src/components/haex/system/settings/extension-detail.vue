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

    <ExtensionInfoSection
      :extension="extension"
      :has-update="hasUpdate"
      :is-updating="isUpdating"
      :is-checking-update="isCheckingUpdate"
      :latest-available-version="latestAvailableVersion"
      @update="() => void handleUpdateAsync()"
      @remove="confirmRemove"
      @open="openExtensionAsync"
    />

    <ExtensionSettingsSection
      v-model:selected-display-mode="selectedDisplayMode"
      :extension="extension"
      :display-mode-options="displayModeOptions"
      @update-display-mode="updateDisplayModeAsync"
    />

    <ExtensionPermissionsSection
      v-model:editable-permissions="editablePermissions"
      :loading-permissions="loadingPermissions"
      :saving-permissions="savingPermissions"
      :has-any-permissions="hasAnyPermissions"
      :has-permission-changes="hasPermissionChanges"
      :permission-accordion-items="permissionAccordionItems"
      @save="() => void savePermissionsAsync(extension.id)"
    />

    <!-- Limits Section -->
    <HaexExtensionLimitsCard :extension-id="extension.id" />

    <ExtensionSessionPermissions
      v-if="sessionPermissions.length > 0"
      :session-permissions="sessionPermissions"
      :revoking-key="revokingSessionPermission"
      @revoke="(p) => void revokeSessionPermissionAsync(extension.id, p)"
    />

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
import { useMarketplaces } from '~/composables/useMarketplaces'
import { useExtensionDetailState } from '~/composables/useExtensionDetailState'
import { useExtensionUpdate } from '~/composables/useExtensionUpdate'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { DisplayMode } from '~~/src-tauri/bindings/DisplayMode'
import ExtensionInfoSection from './extension-detail/ExtensionInfoSection.vue'
import ExtensionSettingsSection from './extension-detail/ExtensionSettingsSection.vue'
import ExtensionPermissionsSection from './extension-detail/ExtensionPermissionsSection.vue'
import ExtensionSessionPermissions from './extension-detail/ExtensionSessionPermissions.vue'

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
const marketplace = useMarketplaces()
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

// Permissions + session permissions state (persistent + in-memory)
const {
  loadingPermissions,
  savingPermissions,
  editablePermissions,
  sessionPermissions,
  revokingSessionPermission,
  hasAnyPermissions,
  hasPermissionChanges,
  permissionAccordionItems,
  loadPermissionsAsync,
  savePermissionsAsync,
  loadSessionPermissionsAsync,
  revokeSessionPermissionAsync,
} = useExtensionDetailState()

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
  await Promise.all([
    loadPermissionsAsync(props.extension.id),
    loadSessionPermissionsAsync(props.extension.id),
    fetchLatestVersionAsync(),
  ])
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
    syncServers: Sync-Server
    cloudStorage: Cloud-Speicher
    syncRules: Sync-Regeln
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
    syncServers: Sync Servers
    cloudStorage: Cloud Storage
    syncRules: Sync Rules
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
