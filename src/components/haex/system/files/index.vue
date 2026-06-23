<template>
  <HaexSystem>
    <!-- Header: Breadcrumbs + Actions -->
    <template #header>
      <FilesHeader
        :browser="browser"
        :peer-store="peerStore"
        :ping="ping"
        :connection-type="connectionType"
        :aggregate-bytes-per-sec="aggregateBytesPerSec"
        :is-uploading="isUploading"
        :is-creating-folder="isCreatingFolder"
        :is-toggling-endpoint="isTogglingEndpoint"
        @refresh-peer-status="refreshPeerStatus"
        @toggle-endpoint="toggleEndpointAsync"
        @open-create-folder-dialog="openCreateFolderDialog"
        @upload-files="uploadFilesAsync"
        @open-p2p-settings="openP2PSettings"
      />
    </template>

    <Transition
      :name="
        browser.direction.value === 'back' ? 'slide-back' : 'slide-forward'
      "
      mode="out-in"
    >
      <div
        :key="
          browser.selectedPeer.value
            ? `peer-${browser.currentPath.value}`
            : 'overview'
        "
        class="p-6 space-y-4"
      >
        <!-- File Browser (peer selected via deep-link or click) -->
        <FilesPeerView
          v-if="browser.selectedPeer.value"
          :browser="browser"
          :peer-store="peerStore"
          @open-rename-dialog="openRenameDialog"
        />

        <!-- Storage overview (no peer selected) -->
        <FilesOverview
          v-else
          :browser="browser"
          :overview-groups="overviewGroups"
          :has-any-entries="hasAnyEntries"
          :group-by="groupBy"
          :ping="ping"
          :connection-type="connectionType"
          @update:group-by="groupBy = $event"
          @refresh-peer-status="refreshPeerStatus"
        />
      </div>
    </Transition>

    <!-- New folder dialog -->
    <FilesNewFolderDialog
      v-model:open="newFolderOpen"
      :loading="isCreatingFolder"
      @confirm="confirmCreateFolderAsync"
    />

    <!-- Rename dialog (triggered from per-row context menu) -->
    <FilesRenameDialog
      v-model:open="renameOpen"
      :loading="isRenaming"
      :initial="renameTarget?.name ?? ''"
      @confirm="confirmRenameAsync"
    />

    <!-- Inline media preview -->
    <FilesPreviewModal :browser="browser" />
  </HaexSystem>
</template>

<script setup lang="ts">
import { SettingsCategory } from '~/config/settingsCategories'
import type { RemotePeer } from '~/composables/fileBrowserHelpers'
import { usePeerPing } from '~/composables/usePeerPing'
import { useFilesOverviewGroups } from '~/composables/useFilesOverviewGroups'

const props = defineProps<{
  tabId: string
  windowParams?: Record<string, unknown>
}>()

const { t } = useI18n()
const peerStore = usePeerStorageStore()
const windowManager = useWindowManagerStore()

const browser = useFileBrowser(props.tabId)

// Overview groups + grouping toggle live in a dedicated composable so the
// parent stays focused on slot composition + action handlers.
const {
  groupBy,
  overviewGroups,
  hasAnyEntries,
  remotePeers,
  remotePeerIds,
  loadAsync: loadOverviewAsync,
} = useFilesOverviewGroups()

const ping = usePeerPing(remotePeerIds)
const connectionType = usePeerConnectionType(remotePeerIds)

// Refresh both ping + connection diagnostics for a single peer — wired to
// the StatusDot's `@mouseenter` so a user who pauses on a stale dot gets an
// immediate update instead of waiting for the next 60s heartbeat tick.
const refreshPeerStatus = (endpointId: string) => {
  void ping.refreshOne(endpointId)
  void connectionType.refreshOne(endpointId)
}

// Aggregate live download throughput across all in-flight transfers.
// Today TransferProgress is not peer-keyed so a multi-peer session would
// sum everything; the file browser only ever displays one peer at a time,
// so in practice this matches the visible context.
const aggregateBytesPerSec = computed(() => peerStore.totalBytesPerSec)

const toast = useToast()

// --- Endpoint toggle ---
const isTogglingEndpoint = ref(false)
const toggleEndpointAsync = async () => {
  isTogglingEndpoint.value = true
  try {
    if (peerStore.running) await peerStore.stopAsync()
    else await peerStore.startAsync()
  } finally {
    isTogglingEndpoint.value = false
  }
}

// --- Upload + create folder ---
const isUploading = ref(false)
const isCreatingFolder = ref(false)
const newFolderOpen = ref(false)

const openCreateFolderDialog = () => {
  newFolderOpen.value = true
}

const confirmCreateFolderAsync = async (name: string) => {
  if (!name.trim()) return
  isCreatingFolder.value = true
  try {
    const ok = await browser.createFolderAsync(name)
    if (ok) {
      newFolderOpen.value = false
    } else {
      toast.add({ title: t('folderNameInvalid'), color: 'error' })
    }
  } catch (error) {
    toast.add({
      title: t('createFolderFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isCreatingFolder.value = false
  }
}

const uploadFilesAsync = async () => {
  isUploading.value = true
  try {
    const count = await browser.uploadFilesAsync()
    if (count > 0) {
      toast.add({
        title: t('uploadSuccess', { count }),
        color: 'success',
      })
    }
  } catch (error) {
    toast.add({
      title: t('uploadFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isUploading.value = false
  }
}

// --- Rename dialog (driven by the per-row context menu) ---
type RenameTarget = (typeof browser.filteredFiles.value)[number] | null
const renameOpen = ref(false)
const renameTarget = ref<RenameTarget>(null)
const isRenaming = ref(false)

const openRenameDialog = (file: NonNullable<RenameTarget>) => {
  renameTarget.value = file
  renameOpen.value = true
}

const confirmRenameAsync = async (newName: string) => {
  const target = renameTarget.value
  if (!target) return
  if (!newName.trim() || newName === target.name) {
    renameOpen.value = false
    return
  }
  isRenaming.value = true
  try {
    const ok = await browser.renameFile(target, newName)
    if (ok) {
      renameOpen.value = false
      renameTarget.value = null
    } else {
      toast.add({ title: t('renameInvalid'), color: 'error' })
    }
  } catch (error) {
    toast.add({
      title: t('renameFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isRenaming.value = false
  }
}

// --- P2P settings deep-link ---
const openP2PSettings = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.Spaces },
  })
}

// --- Deep-linking ---
const applyDeepLink = async (params?: Record<string, unknown>) => {
  if (!params?.endpointId) return

  const endpointId = params.endpointId as string
  const peerName =
    (params.peerName as string) || endpointId.slice(0, 16) + '...'
  const localPath = params.localPath as string | undefined
  const shareName = params.shareName as string | undefined

  const existing = remotePeers.value.find((p) => p.endpointId === endpointId)
  const peer: RemotePeer = existing || {
    endpointId,
    name: peerName,
    source: 'space' as const,
    detail: shareName || '',
    localPath,
  }
  if (existing && localPath && !existing.localPath) {
    peer.localPath = localPath
  }
  browser.setInitialPeer(peer)
  await browser.loadFiles()
}

// React to param changes (singleton window gets params merged on re-open)
watch(
  () => props.windowParams,
  (params) => {
    if (params?.endpointId) applyDeepLink(params)
  },
  { deep: true },
)

onMounted(async () => {
  // Load identities first so `spacesStore.visibleSpaces` can resolve owner
  // and membership filters against the user's own identities — without it,
  // the membership cross-check inside the spaces store would run against an
  // empty ownIdentities set and hide every legitimate space until the next
  // reload.
  const identityStore = useIdentityStore()
  const spacesStore = useSpacesStore()
  await identityStore.loadIdentitiesAsync()
  await Promise.all([
    peerStore.refreshStatusAsync(),
    peerStore.loadSharesAsync(),
    peerStore.loadSpaceDevicesAsync(),
    spacesStore.loadSpacesFromDbAsync(),
    loadOverviewAsync(),
  ])
  await applyDeepLink(props.windowParams)
})
</script>

<i18n lang="yaml">
de:
  title: Dateien
  description: Dateien von verbundenen Geräten durchsuchen und herunterladen
  devices: Geräte
  endpointStopped: P2P-Endpoint ist nicht gestartet
  startEndpoint: Endpoint starten
  stopEndpoint: Endpoint stoppen
  emptyFolder: Ordner ist leer
  noResults: Keine Treffer
  searching: Verzeichnisse werden durchsucht…
  retry: Erneut versuchen
  downloaded: '"{name}" heruntergeladen'
  downloadFailed: Download fehlgeschlagen

  search: Suchen…
  viewList: Listenansicht
  viewGrid: Kachelansicht
  download: Herunterladen
  moreFiles: weitere Dateien werden geladen…
  selected: ausgewählt
  selectAll: Alle auswählen
  copy: Kopieren
  cut: Ausschneiden
  paste: Einfügen
  delete: Löschen
  cancel: Abbrechen
  p2pSettings: P2P-Einstellungen
  noStorage: Keine Speicherquellen verfügbar
  noStorageHint: Teile Ordner in den P2P-Einstellungen oder verbinde dich mit anderen Geräten.
  uploadFiles: Dateien hochladen
  uploadSuccess: '{count} Datei(en) hinzugefügt'
  uploadFailed: Upload fehlgeschlagen
  newFolder: Neuer Ordner
  folderNamePlaceholder: Ordnername
  folderNameInvalid: Ungültiger Ordnername
  createFolderFailed: Ordner konnte nicht erstellt werden
  create: Erstellen
  play: Abspielen
  rename: Umbenennen
  renameTitle: Datei umbenennen
  renamePlaceholder: Neuer Name
  renameInvalid: Ungültiger Name
  renameFailed: Umbenennen fehlgeschlagen
  deleteFailed: Löschen fehlgeschlagen
  openFailed: Öffnen fehlgeschlagen
  cancelTransfer: Übertragung abbrechen
  cancelTransferFailed: Übertragung konnte nicht abgebrochen werden
  pauseTransfer: Übertragung pausieren
  resumeTransfer: Übertragung fortsetzen
  pauseTransferFailed: Übertragung konnte nicht pausiert werden
  mediaPlaybackFailed: Wiedergabe fehlgeschlagen
  mediaCodecMissing: 'Dieses Format kann nicht abgespielt werden – möglicherweise fehlen Codecs (z. B. H.264/AAC). Unter Linux: „gstreamer1.0-libav" und „gstreamer1.0-plugins-bad" installieren.'
  maximizePreview: Maximieren
  restorePreview: Verkleinern
  downloadThroughputTooltip: Aktuelle Download-Geschwindigkeit
  sections:
    local: Dieses Gerät
    peers: Andere Geräte
    thisDevice: Lokaler Ordner
  groupBy:
    label: Gruppierung
    space: Nach Space
    contact: Nach Kontakt
  groups:
    myDevices: Meine Geräte
    directContacts: Direkte Kontakte
    unknown: Ohne Zuordnung
    cloudStorage: Cloud-Speicher
en:
  title: Files
  description: Browse and download files from connected devices
  devices: Devices
  endpointStopped: P2P endpoint is not running
  startEndpoint: Start endpoint
  stopEndpoint: Stop endpoint
  emptyFolder: Folder is empty
  noResults: No matches
  searching: Searching directories…
  retry: Retry
  downloaded: '"{name}" downloaded'
  downloadFailed: Download failed

  search: Search…
  viewList: List view
  viewGrid: Grid view
  download: Download
  moreFiles: more files loading…
  selected: selected
  selectAll: Select all
  copy: Copy
  cut: Cut
  paste: Paste
  delete: Delete
  cancel: Cancel
  p2pSettings: P2P Settings
  noStorage: No storage sources available
  noStorageHint: Share folders in P2P settings or connect with other devices.
  uploadFiles: Upload files
  uploadSuccess: '{count} file(s) added'
  uploadFailed: Upload failed
  newFolder: New folder
  folderNamePlaceholder: Folder name
  folderNameInvalid: Invalid folder name
  createFolderFailed: Could not create folder
  create: Create
  play: Play
  rename: Rename
  renameTitle: Rename file
  renamePlaceholder: New name
  renameInvalid: Invalid name
  renameFailed: Rename failed
  deleteFailed: Delete failed
  openFailed: Open failed
  cancelTransfer: Cancel transfer
  cancelTransferFailed: Could not cancel transfer
  pauseTransfer: Pause transfer
  resumeTransfer: Resume transfer
  pauseTransferFailed: Could not pause transfer
  mediaPlaybackFailed: Playback failed
  mediaCodecMissing: 'This format can''t be played – codecs may be missing (e.g. H.264/AAC). On Linux, install "gstreamer1.0-libav" and "gstreamer1.0-plugins-bad".'
  maximizePreview: Maximize
  restorePreview: Restore
  downloadThroughputTooltip: Current download speed
  sections:
    local: This device
    peers: Other devices
    thisDevice: Local folder
  groupBy:
    label: Group by
    space: By space
    contact: By contact
  groups:
    myDevices: My devices
    directContacts: Direct contacts
    unknown: Unattributed
    cloudStorage: Cloud storage
</i18n>
