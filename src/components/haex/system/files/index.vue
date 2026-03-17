<template>
  <HaexSystem>
    <!-- Header: Breadcrumbs + Actions -->
    <template #header>
      <div class="flex items-center gap-2 min-h-8">
        <!-- Breadcrumbs -->
        <div class="flex items-center gap-1 flex-wrap flex-1 min-w-0">
          <UButton
            variant="ghost"
            color="neutral"
            icon="i-lucide-hard-drive"
            @click="browser.navigateToRoot()"
          >
            {{ t('title') }}
          </UButton>
          <template v-if="browser.selectedPeer.value">
            <UIcon
              name="i-lucide-chevron-right"
              class="w-3.5 h-3.5 text-muted shrink-0"
            />
            <UButton
              variant="ghost"
              color="neutral"
              :disabled="browser.currentPath.value === '/'"
              @click="browser.navigateToPath('/')"
            >
              {{ browser.selectedPeerName.value }}
            </UButton>
            <template
              v-for="(segment, i) in browser.pathSegments.value"
              :key="i"
            >
              <UIcon
                name="i-lucide-chevron-right"
                class="w-3.5 h-3.5 text-muted shrink-0"
              />
              <UButton
                variant="ghost"
                color="neutral"
                :disabled="i === browser.pathSegments.value.length - 1"
                @click="browser.navigateToSegment(i)"
              >
                {{ segment }}
              </UButton>
            </template>
          </template>
        </div>

        <!-- Selection actions -->
        <template v-if="browser.selectionCount.value > 0">
          <span class="text-xs font-medium text-primary shrink-0">
            {{ browser.selectionCount.value }} {{ t('selected') }}
          </span>
          <UiButton
            v-if="browser.selectedPeer.value?.localPath"
            variant="ghost"
            icon="i-lucide-copy"
            :title="t('copy')"
            @click="browser.copySelected()"
          />
          <UiButton
            v-if="browser.selectedPeer.value?.localPath"
            variant="ghost"
            icon="i-lucide-scissors"
            :title="t('cut')"
            @click="browser.cutSelected()"
          />
          <UiButton
            variant="ghost"
            icon="i-lucide-download"
            :title="t('download')"
            @click="browser.downloadSelectedAsync()"
          />
          <UiButton
            v-if="browser.selectedPeer.value?.localPath"
            variant="ghost"
            color="error"
            icon="i-lucide-trash-2"
            :title="t('delete')"
            @click="browser.deleteSelectedAsync()"
          />
          <UiButton
            variant="ghost"
            color="neutral"
            icon="i-lucide-x"
            @click="browser.clearSelection()"
          />
        </template>

        <!-- Paste button (no selection, clipboard has content) -->
        <UiButton
          v-else-if="browser.canPaste.value"
          variant="ghost"
          icon="i-lucide-clipboard-paste"
          @click="browser.pasteAsync()"
        >
          {{ t('paste') }} ({{ browser.clipboard.clipboardCount.value }})
        </UiButton>
      </div>
    </template>

   <div class="p-6 space-y-4">
    <!-- File Browser (peer selected via deep-link or click) -->
    <div
      v-if="browser.selectedPeer.value"
      class="flex flex-col gap-4 h-full"
    >
      <!-- Loading -->
      <div
        v-if="browser.isLoading.value"
        class="flex items-center justify-center py-16"
      >
        <UIcon
          name="i-lucide-loader-2"
          class="w-8 h-8 animate-spin text-muted"
        />
      </div>

      <!-- Error -->
      <div
        v-else-if="browser.loadError.value"
        class="flex flex-col items-center justify-center py-16 gap-3"
      >
        <UIcon
          name="i-lucide-alert-circle"
          class="w-8 h-8 text-error"
        />
        <p class="text-sm text-error">{{ browser.loadError.value }}</p>
        <UiButton
          variant="ghost"
          icon="i-lucide-refresh-cw"
          @click="browser.loadFiles()"
        >
          {{ t('retry') }}
        </UiButton>
      </div>

      <!-- Empty folder -->
      <div
        v-else-if="browser.sortedFiles.value.length === 0"
        class="text-center py-16"
      >
        <UIcon
          name="i-lucide-folder-open"
          class="w-12 h-12 mx-auto mb-2 opacity-30"
        />
        <p class="text-muted">{{ t('emptyFolder') }}</p>
      </div>

      <!-- File listing -->
      <div
        v-else
        class="space-y-1"
      >
        <!-- Select all / Back row -->
        <div class="flex items-center gap-3 p-3">
          <UCheckbox
            :model-value="browser.allSelected.value"
            @update:model-value="browser.allSelected.value ? browser.clearSelection() : browser.selectAll()"
          />
          <div
            v-if="browser.currentPath.value !== '/'"
            class="flex items-center gap-2 cursor-pointer hover:text-primary transition-colors"
            @click="browser.navigateUp()"
          >
            <UIcon
              name="i-lucide-arrow-up"
              class="w-4 h-4 text-muted"
            />
            <span class="text-sm text-muted">..</span>
          </div>
          <span
            v-else
            class="text-xs text-muted"
          >
            {{ t('selectAll') }}
          </span>
        </div>

        <!-- Files and folders -->
        <div
          v-for="file in browser.sortedFiles.value"
          :key="file.name"
          :class="[
            'flex items-center gap-3 p-3 rounded-lg cursor-pointer transition-colors',
            browser.isSelected(file) ? 'bg-primary/10' : 'hover:bg-muted/50',
            browser.isCutFile(file) && 'opacity-40',
          ]"
          @click="browser.onFileClick(file)"
        >
          <UCheckbox
            :model-value="browser.isSelected(file)"
            @click.stop
            @update:model-value="browser.toggleSelect(file)"
          />
          <UIcon
            :name="file.isDir ? 'i-lucide-folder' : browser.getFileIcon(file.name)"
            :class="[
              'w-5 h-5 shrink-0',
              file.isDir ? 'text-primary' : 'text-muted',
            ]"
          />
          <div class="flex-1 min-w-0">
            <p class="text-sm truncate">{{ file.name }}</p>
          </div>
          <span
            v-if="file.modified"
            class="text-xs text-muted w-16 text-right shrink-0"
          >
            {{ browser.formatDate(file.modified) }}
          </span>
          <span
            class="text-xs text-muted w-16 text-right shrink-0"
          >
            {{ file.isDir ? '' : browser.formatSize(file.size) }}
          </span>
        </div>

        <!-- Loading more indicator -->
        <div
          v-if="browser.isLoadingMore.value"
          class="flex items-center justify-center gap-2 py-3 text-muted"
        >
          <UIcon
            name="i-lucide-loader-2"
            class="w-4 h-4 animate-spin"
          />
          <span class="text-xs">{{ browser.totalFiles.value - browser.sortedFiles.value.length }} {{ t('moreFiles') }}</span>
        </div>
      </div>
    </div>

    <!-- Storage overview (no peer selected) -->
    <div
      v-else
      class="flex flex-col gap-6 h-full"
    >
      <!-- Local shares (this device) -->
      <div v-if="localShares.length > 0">
        <p class="text-xs font-medium text-muted uppercase tracking-wider mb-2">
          {{ t('sections.local') }}
        </p>
        <div class="space-y-1">
          <div
            v-for="share in localShares"
            :key="share.id"
            class="flex items-center gap-3 p-3 rounded-lg bg-muted/30 hover:bg-muted/50 cursor-pointer transition-colors"
            @click="browser.selectPeer({
              endpointId: peerStore.nodeId,
              name: share.name,
              source: 'space',
              detail: t('sections.thisDevice'),
              localPath: share.localPath,
            })"
          >
            <UIcon
              name="i-lucide-folder"
              class="w-5 h-5 text-primary shrink-0"
            />
            <div class="flex-1 min-w-0">
              <p class="text-sm font-medium truncate">{{ share.name }}</p>
              <p class="text-xs text-muted truncate">{{ t('sections.thisDevice') }}</p>
            </div>
            <UIcon
              name="i-lucide-chevron-right"
              class="w-4 h-4 text-muted shrink-0"
            />
          </div>
        </div>
      </div>

      <!-- Remote peers -->
      <div v-if="remotePeers.length > 0">
        <p class="text-xs font-medium text-muted uppercase tracking-wider mb-2">
          {{ t('sections.peers') }}
        </p>
        <div class="space-y-1">
          <div
            v-for="peer in remotePeers"
            :key="peer.endpointId"
            class="flex items-center gap-3 p-3 rounded-lg bg-muted/30 hover:bg-muted/50 cursor-pointer transition-colors"
            @click="browser.selectPeer(peer)"
          >
            <UIcon
              :name="peer.source === 'contact' ? 'i-lucide-user' : 'i-lucide-monitor'"
              class="w-5 h-5 text-primary shrink-0"
            />
            <div class="flex-1 min-w-0">
              <p class="text-sm font-medium truncate">{{ peer.name }}</p>
              <p class="text-xs text-muted truncate">{{ peer.detail }}</p>
            </div>
            <UIcon
              name="i-lucide-chevron-right"
              class="w-4 h-4 text-muted shrink-0"
            />
          </div>
        </div>
      </div>

      <!-- P2P not running hint -->
      <div
        v-if="!peerStore.running"
        class="flex items-center gap-3 p-3 rounded-lg border border-dashed border-default"
      >
        <UIcon
          name="i-lucide-wifi-off"
          class="w-5 h-5 text-muted shrink-0"
        />
        <div class="flex-1 min-w-0">
          <p class="text-sm text-muted">{{ t('endpointStopped') }}</p>
        </div>
        <UiButton
          @click="peerStore.startAsync()"
        >
          {{ t('startEndpoint') }}
        </UiButton>
      </div>

      <!-- Empty state -->
      <div
        v-if="localShares.length === 0 && remotePeers.length === 0"
        class="flex flex-col items-center justify-center py-12 gap-3"
      >
        <UIcon
          name="i-lucide-hard-drive"
          class="w-12 h-12 opacity-30"
        />
        <p class="text-muted">{{ t('noStorage') }}</p>
        <p class="text-xs text-muted text-center">{{ t('noStorageHint') }}</p>
      </div>
    </div>

    <!-- File Preview Overlay -->
    <div
      v-if="browser.preview.isOpen.value"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/80"
      @click.self="browser.preview.close()"
    >
      <!-- Close button — always visible, fixed top-right -->
      <button
        class="absolute top-4 right-4 z-10 w-10 h-10 flex items-center justify-center rounded-full bg-black/50 hover:bg-black/70 text-white transition-colors"
        @click="browser.preview.close()"
      >
        <UIcon
          name="i-lucide-x"
          class="w-5 h-5"
        />
      </button>

      <!-- Prev / Next navigation -->
      <button
        v-if="browser.hasPrevPreview.value"
        class="absolute left-4 top-1/2 -translate-y-1/2 z-10 w-10 h-10 flex items-center justify-center rounded-full bg-black/50 hover:bg-black/70 text-white transition-colors"
        @click.stop="browser.previewPrev()"
      >
        <UIcon
          name="i-lucide-chevron-left"
          class="w-5 h-5"
        />
      </button>
      <button
        v-if="browser.hasNextPreview.value"
        class="absolute right-4 top-1/2 -translate-y-1/2 z-10 w-10 h-10 flex items-center justify-center rounded-full bg-black/50 hover:bg-black/70 text-white transition-colors"
        @click.stop="browser.previewNext()"
      >
        <UIcon
          name="i-lucide-chevron-right"
          class="w-5 h-5"
        />
      </button>

      <div class="max-w-[90vw] max-h-[90vh] flex flex-col items-center gap-4">

        <!-- Loading -->
        <div
          v-if="browser.preview.previewLoading.value"
          class="flex items-center justify-center py-16"
        >
          <UIcon
            name="i-lucide-loader-2"
            class="w-8 h-8 animate-spin text-white"
          />
        </div>

        <!-- Image -->
        <img
          v-else-if="browser.preview.previewType.value === 'image' && browser.preview.previewUrl.value"
          :src="browser.preview.previewUrl.value"
          :alt="browser.preview.previewFilename.value || ''"
          class="max-w-full max-h-[85vh] object-contain rounded"
        >

        <!-- Video -->
        <video
          v-else-if="browser.preview.previewType.value === 'video' && browser.preview.previewUrl.value"
          :src="browser.preview.previewUrl.value"
          controls
          autoplay
          class="max-w-full max-h-[85vh] rounded"
        />

        <!-- Audio -->
        <div
          v-else-if="browser.preview.previewType.value === 'audio' && browser.preview.previewUrl.value"
          class="flex flex-col items-center gap-4 p-8"
        >
          <UIcon
            name="i-lucide-music"
            class="w-16 h-16 text-white opacity-50"
          />
          <p class="text-white text-sm">{{ browser.preview.previewFilename.value }}</p>
          <audio
            :src="browser.preview.previewUrl.value"
            controls
            autoplay
          />
        </div>

        <!-- Unsupported file type -->
        <div
          v-else-if="!browser.preview.previewLoading.value"
          class="flex flex-col items-center gap-4 p-8"
        >
          <UIcon
            name="i-lucide-file"
            class="w-16 h-16 text-white opacity-50"
          />
          <p class="text-white text-sm">{{ browser.preview.previewFilename.value }}</p>
          <p class="text-white/60 text-xs">{{ t('noPreview') }}</p>
        </div>

        <!-- Filename -->
        <p
          v-if="!browser.preview.previewLoading.value && browser.preview.previewType.value !== 'unsupported'"
          class="text-white/80 text-xs"
        >
          {{ browser.preview.previewFilename.value }}
        </p>
      </div>
    </div>
   </div>
  </HaexSystem>
</template>

<script setup lang="ts">
import type { RemotePeer } from '~/composables/useFileBrowser'

const props = defineProps<{
  windowParams?: Record<string, unknown>
}>()

const { t } = useI18n()
const peerStore = usePeerStorageStore()
const spacesStore = useSpacesStore()
const contactsStore = useContactsStore()

const browser = useFileBrowser()

// Aggregate remote peers from spaces + contacts
const contactClaims = ref<Record<string, { type: string; value: string }[]>>({})
const loadContactClaimsAsync = async () => {
  for (const contact of contactsStore.contacts) {
    const claims = await contactsStore.getClaimsAsync(contact.id)
    contactClaims.value[contact.id] = claims.map(c => ({ type: c.type, value: c.value }))
  }
}

// Own device shares (browsable locally without P2P)
// When endpoint is running, filter by nodeId. Otherwise show all shares
// (they were all registered by this device since they have local paths).
const localShares = computed(() => {
  if (peerStore.nodeId) {
    return peerStore.shares.filter(s => s.deviceEndpointId === peerStore.nodeId)
  }
  return peerStore.shares
})

const remotePeers = computed(() => {
  const peers: RemotePeer[] = []
  const seen = new Set<string>()

  for (const device of peerStore.spaceDevices) {
    if (device.deviceEndpointId === peerStore.nodeId) continue
    if (seen.has(device.deviceEndpointId)) continue
    seen.add(device.deviceEndpointId)
    peers.push({
      endpointId: device.deviceEndpointId,
      name: device.deviceName || device.deviceEndpointId.slice(0, 16) + '...',
      source: 'space',
      detail: getSpaceName(device.spaceId),
    })
  }

  for (const contact of contactsStore.contacts) {
    const claims = contactClaims.value[contact.id] || []
    for (const claim of claims) {
      if (!claim.type.startsWith('device:') || !claim.value) continue
      if (seen.has(claim.value)) continue
      seen.add(claim.value)
      peers.push({
        endpointId: claim.value,
        name: `${contact.label} (${claim.type.replace('device:', '')})`,
        source: 'contact',
        detail: contact.label,
      })
    }
  }

  return peers
})

const getSpaceName = (spaceId: string) => {
  return spacesStore.spaces.find(s => s.id === spaceId)?.name || spaceId.slice(0, 8)
}

const applyDeepLink = async (params?: Record<string, unknown>) => {
  if (!params?.endpointId) return

  const endpointId = params.endpointId as string
  const peerName = (params.peerName as string) || endpointId.slice(0, 16) + '...'
  const localPath = params.localPath as string | undefined
  const shareName = params.shareName as string | undefined

  const existing = remotePeers.value.find(p => p.endpointId === endpointId)
  const peer = existing || {
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
watch(() => props.windowParams, (params) => {
  if (params?.endpointId) applyDeepLink(params)
}, { deep: true })

onMounted(async () => {
  await Promise.all([
    peerStore.refreshStatusAsync(),
    peerStore.loadSharesAsync(),
    peerStore.loadSpaceDevicesAsync(),
    contactsStore.loadContactsAsync(),
  ])
  await loadContactClaimsAsync()
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
  emptyFolder: Ordner ist leer
  retry: Erneut versuchen
  downloaded: '"{name}" heruntergeladen'
  downloadFailed: Download fehlgeschlagen
  noPreview: Vorschau nicht verfügbar
  download: Herunterladen
  moreFiles: weitere Dateien werden geladen…
  selected: ausgewählt
  selectAll: Alle auswählen
  copy: Kopieren
  cut: Ausschneiden
  paste: Einfügen
  delete: Löschen
  cancel: Abbrechen
  noStorage: Keine Speicherquellen verfügbar
  noStorageHint: Teile Ordner in den P2P-Einstellungen oder verbinde dich mit anderen Geräten.
  sections:
    local: Dieses Gerät
    peers: Andere Geräte
    thisDevice: Lokaler Ordner
en:
  title: Files
  description: Browse and download files from connected devices
  devices: Devices
  endpointStopped: P2P endpoint is not running
  startEndpoint: Start endpoint
  emptyFolder: Folder is empty
  retry: Retry
  downloaded: '"{name}" downloaded'
  downloadFailed: Download failed
  noPreview: Preview not available
  download: Download
  moreFiles: more files loading…
  selected: selected
  selectAll: Select all
  copy: Copy
  cut: Cut
  paste: Paste
  delete: Delete
  cancel: Cancel
  noStorage: No storage sources available
  noStorageHint: Share folders in P2P settings or connect with other devices.
  sections:
    local: This device
    peers: Other devices
    thisDevice: Local folder
</i18n>
