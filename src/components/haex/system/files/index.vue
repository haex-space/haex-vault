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
            v-if="!browser.selectedPeer.value?.localPath"
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

        <!-- P2P endpoint toggle + settings -->
        <template v-if="!browser.selectedPeer.value">
          <UiButton
            variant="ghost"
            icon="i-lucide-settings"
            :title="t('p2pSettings')"
            @click="openP2PSettings"
          />
          <UiButton
            :icon="peerStore.running ? 'i-lucide-power-off' : 'i-lucide-power'"
            :color="peerStore.running ? 'error' : 'primary'"
            :loading="isTogglingEndpoint"
            :title="peerStore.running ? t('stopEndpoint') : t('startEndpoint')"
            @click="toggleEndpointAsync"
          />
        </template>
      </div>
    </template>

    <Transition :name="browser.direction.value === 'back' ? 'slide-back' : 'slide-forward'" mode="out-in">
      <div :key="browser.selectedPeer.value ? `peer-${browser.currentPath.value}` : 'overview'" class="p-6 space-y-4">
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
              @update:model-value="
                browser.allSelected.value
                  ? browser.clearSelection()
                  : browser.selectAll()
              "
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
              'flex items-center gap-3 p-3 rounded-lg cursor-pointer transition-colors relative overflow-hidden',
              browser.isSelected(file) ? 'bg-primary/10' : 'hover:bg-muted/50',
              browser.isCutFile(file) && 'opacity-40',
            ]"
            @click="browser.onFileClick(file)"
          >
            <!-- Download progress background -->
            <div
              v-if="getFileTransferProgress(file) !== undefined"
              class="absolute inset-0 bg-primary/15 transition-all duration-300 ease-out"
              :style="{ width: `${(getFileTransferProgress(file) ?? 0) * 100}%` }"
            />
            <UCheckbox
              :model-value="browser.isSelected(file)"
              class="relative z-10"
              @click.stop
              @update:model-value="browser.toggleSelect(file)"
            />
            <UIcon
              :name="
                file.isDir ? 'i-lucide-folder' : browser.getFileIcon(file.name)
              "
              :class="[
                'w-5 h-5 shrink-0 relative z-10',
                file.isDir ? 'text-primary' : 'text-muted',
              ]"
            />
            <div class="flex-1 min-w-0 relative z-10">
              <p class="text-sm truncate">{{ file.name }}</p>
              <div class="flex gap-3 text-xs text-muted mt-0.5">
                <span v-if="file.modified">{{ browser.formatDate(file.modified) }}</span>
                <span v-if="!file.isDir && file.size">{{ browser.formatSize(file.size) }}</span>
              </div>
            </div>
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
            <span class="text-xs"
              >{{ browser.totalFiles.value - browser.sortedFiles.value.length }}
              {{ t('moreFiles') }}</span
            >
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
          <p
            class="text-xs font-medium text-muted uppercase tracking-wider mb-2"
          >
            {{ t('sections.local') }}
          </p>
          <div class="space-y-1">
            <div
              v-for="share in localShares"
              :key="share.id"
              class="flex items-center gap-3 p-3 rounded-lg bg-muted/30 hover:bg-muted/50 cursor-pointer transition-colors"
              @click="
                browser.selectPeer({
                  endpointId: peerStore.nodeId,
                  name: share.name,
                  source: 'space',
                  detail: t('sections.thisDevice'),
                  localPath: share.localPath,
                })
              "
            >
              <UIcon
                name="i-lucide-folder"
                class="w-5 h-5 text-primary shrink-0"
              />
              <div class="flex-1 min-w-0">
                <p class="text-sm font-medium truncate">{{ share.name }}</p>
                <p class="text-xs text-muted truncate">
                  {{ t('sections.thisDevice') }}
                </p>
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
          <p
            class="text-xs font-medium text-muted uppercase tracking-wider mb-2"
          >
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
                :name="
                  peer.source === 'contact'
                    ? 'i-lucide-user'
                    : 'i-lucide-monitor'
                "
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

      </div>
    </Transition>
  </HaexSystem>
</template>

<script setup lang="ts">
import { SettingsCategory } from '~/config/settingsCategories'
import type { RemotePeer } from '~/composables/useFileBrowser'

const props = defineProps<{
  tabId: string
  windowParams?: Record<string, unknown>
}>()

const { t } = useI18n()
const peerStore = usePeerStorageStore()
const spacesStore = useSpacesStore()
const identityStore = useIdentityStore()

const browser = useFileBrowser(props.tabId)

/** Get transfer progress for a file (0-1, or undefined if not downloading) */
const getFileTransferProgress = (file: { name: string; path?: string }) => {
  if (!browser.selectedPeer.value) return undefined
  const fullPath = (file.path || `${browser.currentPath.value}/${file.name}`).replace(/\/+/g, '/')
  return peerStore.getTransferProgress(fullPath)
}

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

// Aggregate remote peers from spaces + contacts
const contactClaims = ref<Record<string, { type: string; value: string }[]>>({})
const loadContactClaimsAsync = async () => {
  for (const contact of identityStore.contacts) {
    const claims = await identityStore.getClaimsAsync(contact.id)
    contactClaims.value[contact.id] = claims.map((c) => ({
      type: c.type,
      value: c.value,
    }))
  }
}

// Own device shares (browsable locally without P2P)
const windowManager = useWindowManagerStore()
const openP2PSettings = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.PeerStorage },
  })
}

// When endpoint is running, filter by nodeId. Otherwise show all shares
// (they were all registered by this device since they have local paths).
const localShares = computed(() => {
  if (peerStore.nodeId) {
    return peerStore.shares.filter(
      (s) => s.deviceEndpointId === peerStore.nodeId,
    )
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

  for (const contact of identityStore.contacts) {
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
  return (
    spacesStore.spaces.find((s) => s.id === spaceId)?.name ||
    spaceId.slice(0, 8)
  )
}

const applyDeepLink = async (params?: Record<string, unknown>) => {
  if (!params?.endpointId) return

  const endpointId = params.endpointId as string
  const peerName =
    (params.peerName as string) || endpointId.slice(0, 16) + '...'
  const localPath = params.localPath as string | undefined
  const shareName = params.shareName as string | undefined

  const existing = remotePeers.value.find((p) => p.endpointId === endpointId)
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
watch(
  () => props.windowParams,
  (params) => {
    if (params?.endpointId) applyDeepLink(params)
  },
  { deep: true },
)

onMounted(async () => {
  await Promise.all([
    peerStore.refreshStatusAsync(),
    peerStore.loadSharesAsync(),
    peerStore.loadSpaceDevicesAsync(),
    identityStore.loadIdentitiesAsync(),
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
  stopEndpoint: Endpoint stoppen
  emptyFolder: Ordner ist leer
  retry: Erneut versuchen
  downloaded: '"{name}" heruntergeladen'
  downloadFailed: Download fehlgeschlagen

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
  stopEndpoint: Stop endpoint
  emptyFolder: Folder is empty
  retry: Retry
  downloaded: '"{name}" downloaded'
  downloadFailed: Download failed

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
  sections:
    local: This device
    peers: Other devices
    thisDevice: Local folder
</i18n>
