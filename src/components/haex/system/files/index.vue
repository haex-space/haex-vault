<template>
  <HaexSystem :title="t('title')" :description="t('description')">
    <!-- No endpoint running -->
    <div
      v-if="!peerStore.running"
      class="flex flex-col items-center justify-center py-16 gap-4"
    >
      <UIcon
        name="i-lucide-wifi-off"
        class="w-12 h-12 opacity-30"
      />
      <p class="text-muted">{{ t('endpointStopped') }}</p>
      <UiButton
        icon="i-lucide-power"
        @click="peerStore.startAsync()"
      >
        {{ t('startEndpoint') }}
      </UiButton>
    </div>

    <!-- No remote peers -->
    <div
      v-else-if="remotePeers.length === 0"
      class="flex flex-col items-center justify-center py-16 gap-4"
    >
      <UIcon
        name="i-lucide-monitor-off"
        class="w-12 h-12 opacity-30"
      />
      <p class="text-muted">{{ t('noPeers') }}</p>
      <p class="text-xs text-muted">{{ t('noPeersHint') }}</p>
    </div>

    <!-- File Browser -->
    <div
      v-else
      class="flex flex-col gap-4 h-full"
    >
      <!-- Breadcrumb navigation -->
      <div class="flex items-center gap-2 flex-wrap">
        <UButton
          v-if="!selectedPeer"
          variant="ghost"
          color="neutral"
          icon="i-lucide-monitor"
          disabled
        >
          {{ t('devices') }}
        </UButton>
        <template v-else>
          <UButton
            variant="ghost"
            color="neutral"
            icon="i-lucide-monitor"
            @click="navigateToRoot"
          >
            {{ t('devices') }}
          </UButton>
          <UIcon
            name="i-lucide-chevron-right"
            class="w-4 h-4 text-muted"
          />
          <UButton
            variant="ghost"
            color="neutral"
            :disabled="currentPath === '/'"
            @click="currentPath = '/'"
          >
            {{ selectedPeerName }}
          </UButton>
          <template
            v-for="(segment, i) in pathSegments"
            :key="i"
          >
            <UIcon
              name="i-lucide-chevron-right"
              class="w-4 h-4 text-muted"
            />
            <UButton
              variant="ghost"
              color="neutral"
              :disabled="i === pathSegments.length - 1"
              @click="navigateToSegment(i)"
            >
              {{ segment }}
            </UButton>
          </template>
        </template>
      </div>

      <!-- Device list -->
      <div
        v-if="!selectedPeer"
        class="space-y-2"
      >
        <div
          v-for="peer in remotePeers"
          :key="peer.deviceEndpointId"
          class="flex items-center gap-3 p-4 rounded-lg bg-muted/30 hover:bg-muted/50 cursor-pointer transition-colors"
          @click="selectPeer(peer)"
        >
          <UIcon
            name="i-lucide-monitor"
            class="w-6 h-6 text-primary shrink-0"
          />
          <div class="flex-1 min-w-0">
            <p class="font-medium truncate">
              {{ peer.deviceName || peer.deviceEndpointId.slice(0, 16) + '...' }}
            </p>
            <p class="text-xs text-muted truncate">
              {{ getSpaceName(peer.spaceId) }}
            </p>
          </div>
          <UIcon
            name="i-lucide-chevron-right"
            class="w-5 h-5 text-muted shrink-0"
          />
        </div>
      </div>

      <!-- File listing -->
      <div
        v-else-if="isLoading"
        class="flex items-center justify-center py-16"
      >
        <UIcon
          name="i-lucide-loader-2"
          class="w-8 h-8 animate-spin text-muted"
        />
      </div>

      <div
        v-else-if="loadError"
        class="flex flex-col items-center justify-center py-16 gap-3"
      >
        <UIcon
          name="i-lucide-alert-circle"
          class="w-8 h-8 text-error"
        />
        <p class="text-sm text-error">{{ loadError }}</p>
        <UiButton
          variant="ghost"
          icon="i-lucide-refresh-cw"
          @click="loadFiles"
        >
          {{ t('retry') }}
        </UiButton>
      </div>

      <div
        v-else-if="files.length === 0"
        class="text-center py-16"
      >
        <UIcon
          name="i-lucide-folder-open"
          class="w-12 h-12 mx-auto mb-2 opacity-30"
        />
        <p class="text-muted">{{ t('emptyFolder') }}</p>
      </div>

      <div
        v-else
        class="space-y-1"
      >
        <!-- Back button -->
        <div
          v-if="currentPath !== '/'"
          class="flex items-center gap-3 p-3 rounded-lg hover:bg-muted/50 cursor-pointer transition-colors"
          @click="navigateUp"
        >
          <UIcon
            name="i-lucide-arrow-up"
            class="w-5 h-5 text-muted shrink-0"
          />
          <span class="text-sm text-muted">..</span>
        </div>

        <!-- Files and folders -->
        <div
          v-for="file in sortedFiles"
          :key="file.name"
          class="flex items-center gap-3 p-3 rounded-lg hover:bg-muted/50 cursor-pointer transition-colors"
          @click="onFileClick(file)"
        >
          <UIcon
            :name="file.isDir ? 'i-lucide-folder' : getFileIcon(file.name)"
            :class="[
              'w-5 h-5 shrink-0',
              file.isDir ? 'text-primary' : 'text-muted',
            ]"
          />
          <div class="flex-1 min-w-0">
            <p class="text-sm truncate">{{ file.name }}</p>
          </div>
          <span
            v-if="!file.isDir"
            class="text-xs text-muted shrink-0"
          >
            {{ formatSize(file.size) }}
          </span>
          <UIcon
            v-if="!file.isDir"
            name="i-lucide-download"
            class="w-4 h-4 text-muted shrink-0 opacity-0 group-hover:opacity-100"
          />
        </div>
      </div>
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
import type { FileEntry } from '@bindings/FileEntry'
import type { SelectHaexSpaceDevices } from '~/database/schemas'

const { t } = useI18n()
const { add } = useToast()
const peerStore = usePeerStorageStore()
const spacesStore = useSpacesStore()

const selectedPeer = ref<SelectHaexSpaceDevices | null>(null)
const currentPath = ref('/')
const files = ref<FileEntry[]>([])
const isLoading = ref(false)
const loadError = ref<string | null>(null)

// Remote peers (exclude own device)
const remotePeers = computed(() =>
  peerStore.spaceDevices.filter(d => d.deviceEndpointId !== peerStore.nodeId),
)

const selectedPeerName = computed(() =>
  selectedPeer.value?.deviceName || selectedPeer.value?.deviceEndpointId.slice(0, 16) + '...',
)

const pathSegments = computed(() =>
  currentPath.value.split('/').filter(Boolean),
)

const sortedFiles = computed(() =>
  [...files.value].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1
    return a.name.localeCompare(b.name)
  }),
)

const getSpaceName = (spaceId: string) => {
  const space = spacesStore.spaces.find(s => s.id === spaceId)
  return space?.name || spaceId.slice(0, 8)
}

const getFileIcon = (name: string) => {
  const ext = name.split('.').pop()?.toLowerCase()
  switch (ext) {
    case 'jpg': case 'jpeg': case 'png': case 'gif': case 'webp': case 'svg':
      return 'i-lucide-image'
    case 'mp4': case 'mov': case 'avi': case 'mkv':
      return 'i-lucide-video'
    case 'mp3': case 'wav': case 'flac': case 'ogg':
      return 'i-lucide-music'
    case 'pdf':
      return 'i-lucide-file-text'
    case 'zip': case 'tar': case 'gz': case '7z': case 'rar':
      return 'i-lucide-archive'
    default:
      return 'i-lucide-file'
  }
}

const formatSize = (bytes: number) => {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${units[i]}`
}

const selectPeer = (peer: SelectHaexSpaceDevices) => {
  selectedPeer.value = peer
  currentPath.value = '/'
  loadFiles()
}

const navigateToRoot = () => {
  selectedPeer.value = null
  currentPath.value = '/'
  files.value = []
}

const navigateUp = () => {
  const segments = pathSegments.value
  segments.pop()
  currentPath.value = segments.length ? '/' + segments.join('/') : '/'
  loadFiles()
}

const navigateToSegment = (index: number) => {
  const segments = pathSegments.value.slice(0, index + 1)
  currentPath.value = '/' + segments.join('/')
  loadFiles()
}

const onFileClick = async (file: FileEntry) => {
  if (file.isDir) {
    const newPath = currentPath.value === '/'
      ? `/${file.name}`
      : `${currentPath.value}/${file.name}`
    currentPath.value = newPath
    loadFiles()
  } else {
    await downloadFile(file)
  }
}

const loadFiles = async () => {
  if (!selectedPeer.value) return

  isLoading.value = true
  loadError.value = null

  try {
    files.value = await peerStore.remoteListAsync(
      selectedPeer.value.deviceEndpointId,
      currentPath.value,
    )
  } catch (error) {
    loadError.value = error instanceof Error ? error.message : String(error)
    files.value = []
  } finally {
    isLoading.value = false
  }
}

const downloadFile = async (file: FileEntry) => {
  if (!selectedPeer.value) return

  try {
    const filePath = currentPath.value === '/'
      ? `/${file.name}`
      : `${currentPath.value}/${file.name}`

    const base64 = await peerStore.remoteReadAsync(
      selectedPeer.value.deviceEndpointId,
      filePath,
    )

    // Convert base64 to blob and trigger download
    const binaryString = atob(base64)
    const bytes = new Uint8Array(binaryString.length)
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i)
    }
    const blob = new Blob([bytes])
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = file.name
    a.click()
    URL.revokeObjectURL(url)

    add({ title: t('downloaded', { name: file.name }), color: 'success' })
  } catch (error) {
    add({
      title: t('downloadFailed'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

onMounted(async () => {
  await peerStore.refreshStatusAsync()
  await peerStore.loadSpaceDevicesAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Dateien
  description: Dateien von verbundenen Geräten durchsuchen und herunterladen
  devices: Geräte
  endpointStopped: P2P-Endpoint ist nicht gestartet
  startEndpoint: Endpoint starten
  noPeers: Keine verbundenen Geräte
  noPeersHint: Andere Geräte müssen die gleiche Vault geöffnet und den P2P-Endpoint gestartet haben.
  emptyFolder: Ordner ist leer
  retry: Erneut versuchen
  downloaded: '"{name}" heruntergeladen'
  downloadFailed: Download fehlgeschlagen
en:
  title: Files
  description: Browse and download files from connected devices
  devices: Devices
  endpointStopped: P2P endpoint is not running
  startEndpoint: Start endpoint
  noPeers: No connected devices
  noPeersHint: Other devices must have the same vault open and the P2P endpoint running.
  emptyFolder: Folder is empty
  retry: Retry
  downloaded: '"{name}" downloaded'
  downloadFailed: Download failed
</i18n>
