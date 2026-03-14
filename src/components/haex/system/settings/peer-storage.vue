<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <!-- Endpoint Control -->
    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-lg font-semibold">{{ t('endpoint.title') }}</h3>
            <p class="text-sm text-muted mt-1">{{ t('endpoint.description') }}</p>
          </div>
          <UiButton
            :icon="store.running ? 'i-lucide-power-off' : 'i-lucide-power'"
            :color="store.running ? 'error' : 'primary'"
            :loading="isToggling"
            size="lg"
            @click="onToggleEndpointAsync"
          >
            {{ store.running ? t('endpoint.stop') : t('endpoint.start') }}
          </UiButton>
        </div>
      </template>

      <div v-if="store.running" class="space-y-3">
        <div class="flex items-center gap-2">
          <UBadge color="success" variant="subtle" size="xs">
            {{ t('endpoint.running') }}
          </UBadge>
        </div>

        <div class="flex items-center gap-2">
          <span class="text-sm text-muted shrink-0">{{ t('endpoint.nodeId') }}:</span>
          <code class="text-xs bg-muted/50 px-2 py-1 rounded font-mono truncate flex-1">
            {{ store.nodeId }}
          </code>
          <UiButton
            icon="i-lucide-copy"
            variant="ghost"
            color="neutral"
            size="md"
            @click="onCopyNodeId"
          />
        </div>
        <p class="text-xs text-muted">{{ t('endpoint.nodeIdHint') }}</p>
      </div>

      <div v-else class="text-center py-4 text-muted">
        <UIcon name="i-lucide-wifi-off" class="w-8 h-8 mx-auto mb-2 opacity-50" />
        <p class="text-sm">{{ t('endpoint.stopped') }}</p>
      </div>
    </UCard>

    <!-- Shared Folders by Space -->
    <UCard>
      <template #header>
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h3 class="text-lg font-semibold">{{ t('shares.title') }}</h3>
            <p class="text-sm text-muted mt-1">{{ t('shares.description') }}</p>
          </div>
        </div>
      </template>

      <!-- No Spaces available -->
      <div v-if="!spacesStore.spaces.length" class="text-center py-8 text-muted">
        <UIcon name="i-lucide-cloud-off" class="w-12 h-12 mx-auto mb-3 opacity-50" />
        <p>{{ t('shares.noSpaces') }}</p>
        <p class="text-sm mt-1">{{ t('shares.noSpacesHint') }}</p>
        <UiButton
          class="mt-4"
          icon="i-heroicons-user-group"
          @click="onNavigateToSpaces"
        >
          {{ t('shares.goToSpaces') }}
        </UiButton>
      </div>

      <!-- Spaces with shares -->
      <div v-else class="space-y-6">
        <div
          v-for="space in spacesStore.spaces"
          :key="space.id"
          class="border border-default rounded-lg overflow-hidden"
        >
          <!-- Space header -->
          <div class="flex items-center justify-between gap-3 px-4 py-3 bg-muted/30">
            <div class="flex items-center gap-2 min-w-0">
              <UIcon name="i-lucide-cloud" class="w-4 h-4 text-primary shrink-0" />
              <span class="font-medium truncate">{{ space.name }}</span>
              <UBadge variant="subtle" size="xs">{{ space.role }}</UBadge>
            </div>
            <UiButton
              icon="i-lucide-folder-plus"
              size="sm"
              variant="ghost"
              @click="onAddShareAsync(space.id)"
            >
              {{ t('shares.add') }}
            </UiButton>
          </div>

          <!-- Shares grouped by device -->
          <div class="divide-y divide-default">
            <!-- Current device shares -->
            <div v-if="getSharesForDevice(space.id, store.nodeId).length" class="px-4 py-3">
              <div class="flex items-center gap-2 mb-2">
                <UIcon name="i-lucide-monitor" class="w-3.5 h-3.5 text-success" />
                <span class="text-sm font-medium text-success">{{ t('shares.thisDevice') }}</span>
              </div>
              <div class="space-y-2 ml-5">
                <div
                  v-for="share in getSharesForDevice(space.id, store.nodeId)"
                  :key="share.id"
                  class="flex items-center justify-between gap-2"
                >
                  <div class="min-w-0 flex-1">
                    <p class="text-sm font-medium">{{ share.name }}</p>
                    <p class="text-xs text-muted truncate">{{ share.localPath }}</p>
                  </div>
                  <UiButton
                    color="error"
                    variant="ghost"
                    icon="i-lucide-trash-2"
                    size="xs"
                    @click="onRemoveShareAsync(share.id)"
                  />
                </div>
              </div>
            </div>

            <!-- Other devices' shares -->
            <div
              v-for="[deviceId, deviceShares] in getOtherDeviceShares(space.id)"
              :key="deviceId"
              class="px-4 py-3"
            >
              <div class="flex items-center gap-2 mb-2">
                <UIcon name="i-lucide-monitor" class="w-3.5 h-3.5 text-muted" />
                <span class="text-sm font-medium text-muted">
                  {{ getDeviceName(deviceId) || deviceId.slice(0, 12) + '…' }}
                </span>
              </div>
              <div class="space-y-2 ml-5">
                <div
                  v-for="share in deviceShares"
                  :key="share.id"
                  class="flex items-center gap-2"
                >
                  <div class="min-w-0 flex-1">
                    <p class="text-sm font-medium">{{ share.name }}</p>
                    <p class="text-xs text-muted truncate">{{ share.localPath }}</p>
                  </div>
                </div>
              </div>
            </div>

            <!-- No shares in this space -->
            <div
              v-if="getSharesForSpace(space.id).length === 0"
              class="px-4 py-6 text-center text-muted"
            >
              <p class="text-sm">{{ t('shares.emptySpace') }}</p>
            </div>
          </div>
        </div>
      </div>
    </UCard>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { open } from '@tauri-apps/plugin-dialog'
import type { SelectHaexPeerShares } from '~/database/schemas'

const { t } = useI18n()
const { add } = useToast()
const store = usePeerStorageStore()
const spacesStore = useSpacesStore()
const windowManager = useWindowManagerStore()

const isToggling = ref(false)

onMounted(async () => {
  await store.refreshStatusAsync()
  await store.loadSharesAsync()
  await store.loadSpaceDevicesAsync()
})

// =========================================================================
// Computed helpers for grouping shares
// =========================================================================

const getSharesForSpace = (spaceId: string): SelectHaexPeerShares[] => {
  return store.shares.filter(s => s.spaceId === spaceId)
}

const getSharesForDevice = (spaceId: string, deviceEndpointId: string): SelectHaexPeerShares[] => {
  return store.shares.filter(s => s.spaceId === spaceId && s.deviceEndpointId === deviceEndpointId)
}

const getOtherDeviceShares = (spaceId: string): [string, SelectHaexPeerShares[]][] => {
  const spaceShares = getSharesForSpace(spaceId).filter(s => s.deviceEndpointId !== store.nodeId)

  const grouped = new Map<string, SelectHaexPeerShares[]>()
  for (const share of spaceShares) {
    const existing = grouped.get(share.deviceEndpointId) || []
    existing.push(share)
    grouped.set(share.deviceEndpointId, existing)
  }

  return [...grouped.entries()]
}

const getDeviceName = (deviceEndpointId: string): string | undefined => {
  return store.spaceDevices.find(d => d.deviceEndpointId === deviceEndpointId)?.deviceName
}

// =========================================================================
// Actions
// =========================================================================

const onNavigateToSpaces = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: 'spaces' },
  })
}

const onToggleEndpointAsync = async () => {
  isToggling.value = true
  try {
    if (store.running) {
      await store.stopAsync()
      add({ title: t('toast.stopped'), color: 'neutral' })
    } else {
      await store.startAsync()
      add({ title: t('toast.started'), color: 'success' })
    }
  } catch (error) {
    add({
      title: t('toast.error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isToggling.value = false
  }
}

const onCopyNodeId = async () => {
  await navigator.clipboard.writeText(store.nodeId)
  add({ title: t('toast.copied'), color: 'success' })
}

const onAddShareAsync = async (spaceId: string) => {
  const selected = await open({ directory: true, multiple: false })
  if (!selected) return

  const path = typeof selected === 'string' ? selected : selected[0]
  if (!path) return

  const name = path.split(/[/\\]/).pop() || 'Shared Folder'

  try {
    await store.addShareAsync(spaceId, name, path)
    add({ title: t('toast.shareAdded'), color: 'success' })
  } catch (error) {
    add({
      title: t('toast.error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const onRemoveShareAsync = async (shareId: string) => {
  try {
    await store.removeShareAsync(shareId)
    add({ title: t('toast.shareRemoved'), color: 'neutral' })
  } catch (error) {
    add({
      title: t('toast.error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Peer Storage
  description: Teile lokale Ordner direkt mit anderen Peers über eine verschlüsselte P2P-Verbindung
  endpoint:
    title: Verbindung
    description: Starte den P2P-Endpoint, um Dateien zu teilen und zu empfangen
    start: Starten
    stop: Stoppen
    running: Aktiv
    stopped: Endpoint ist nicht aktiv. Starte ihn, um Dateien zu teilen.
    nodeId: Node-ID
    nodeIdHint: Teile diese ID mit Peers, damit sie sich mit dir verbinden können.
  shares:
    title: Geteilte Ordner
    description: Ordner pro Space und Device, die für verbundene Peers zugänglich sind
    add: Ordner hinzufügen
    noSpaces: Keine Spaces vorhanden
    noSpacesHint: Erstelle oder trete einem Space bei, um Ordner zu teilen
    emptySpace: Noch keine Ordner in diesem Space geteilt
    thisDevice: Dieses Gerät
    goToSpaces: Spaces verwalten
  toast:
    started: P2P-Endpoint gestartet
    stopped: P2P-Endpoint gestoppt
    copied: Node-ID kopiert
    shareAdded: Ordner hinzugefügt
    shareRemoved: Ordner entfernt
    error: Fehler
en:
  title: Peer Storage
  description: Share local folders directly with other peers over an encrypted P2P connection
  endpoint:
    title: Connection
    description: Start the P2P endpoint to share and receive files
    start: Start
    stop: Stop
    running: Active
    stopped: Endpoint is not active. Start it to share files.
    nodeId: Node ID
    nodeIdHint: Share this ID with peers so they can connect to you.
  shares:
    title: Shared Folders
    description: Folders per space and device, accessible to connected peers
    add: Add Folder
    noSpaces: No spaces available
    noSpacesHint: Create or join a space to share folders
    emptySpace: No folders shared in this space yet
    thisDevice: This device
    goToSpaces: Manage Spaces
  toast:
    started: P2P endpoint started
    stopped: P2P endpoint stopped
    copied: Node ID copied
    shareAdded: Folder added
    shareRemoved: Folder removed
    error: Error
</i18n>
