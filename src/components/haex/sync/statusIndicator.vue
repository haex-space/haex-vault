<template>
  <div
    v-if="groups.length > 0"
    class="flex items-center gap-3"
  >
    <UTooltip
      v-for="group in groups"
      :key="group.id"
      :text="group.tooltip"
    >
      <button
        class="relative cursor-pointer"
        :class="sizeClass"
        @click="group.onClick"
      >
        <!-- Single item: render icon directly -->
        <UIcon
          v-if="group.segments.length === 1"
          :name="group.icon"
          class="w-full h-full transition-colors duration-300"
          :class="[group.segments[0]!.colorClass, { 'animate-pulse-status': group.segments[0]!.isPulsing }]"
        />

        <!-- Multiple items: split icon via clip-path overlays -->
        <template v-else>
          <UIcon
            v-for="(seg, idx) in group.segments"
            :key="seg.id"
            :name="group.icon"
            class="w-full h-full transition-colors duration-300"
            :class="[
              seg.colorClass,
              { 'animate-pulse-status': seg.isPulsing },
              idx > 0 ? 'absolute inset-0' : '',
            ]"
            :style="clipStyle(idx, group.segments.length)"
          />
        </template>
      </button>
    </UTooltip>
  </div>
</template>

<script setup lang="ts">
import { SettingsCategory, SettingsCategoryIcon } from '~/config/settingsCategories'

interface Props {
  size?: 'sm' | 'md' | 'lg'
}

const props = withDefaults(defineProps<Props>(), {
  size: 'md',
})

const { t } = useI18n()
const syncOrchestratorStore = useSyncOrchestratorStore()
const syncBackendsStore = useSyncBackendsStore()
const peerStore = usePeerStorageStore()
const windowManager = useWindowManagerStore()

const { syncStates } = storeToRefs(syncOrchestratorStore)
const { enabledBackends } = storeToRefs(syncBackendsStore)

const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm': return 'size-4'
    case 'lg': return 'size-6'
    default: return 'size-5'
  }
})

/** Generate clip-path for segment idx of total — vertical slices left to right */
const clipStyle = (idx: number, total: number) => {
  const sliceWidth = 100 / total
  const left = sliceWidth * idx
  const right = 100 - sliceWidth * (idx + 1)
  return { clipPath: `inset(0 ${right}% 0 ${left}%)` }
}

const openSettings = (category: SettingsCategory) => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category },
  })
}

interface Segment {
  id: string
  colorClass: string
  isPulsing: boolean
  label: string
}

interface IndicatorGroup {
  id: string
  icon: string
  segments: Segment[]
  tooltip: string
  onClick: () => void
}

const groups = computed(() => {
  const result: IndicatorGroup[] = []

  // Sync backends — all share the same icon, split by backend count
  const backends = enabledBackends.value
  if (backends.length > 0) {
    const segments: Segment[] = backends.map((backend) => {
      const state = syncStates.value[backend.id]
      let colorClass = 'text-warning'
      let isPulsing = false
      let label = t('sync.connecting')

      if (state?.error) {
        colorClass = 'text-error'
        label = t('sync.error')
      } else if (state?.isConnected) {
        colorClass = 'text-success'
        if (state.isSyncing) {
          isPulsing = true
          label = t('sync.syncing')
        } else {
          label = t('sync.connected')
        }
      }

      return { id: backend.id, colorClass, isPulsing, label: `${backend.name}: ${label}` }
    })

    result.push({
      id: 'sync',
      icon: SettingsCategoryIcon[SettingsCategory.Sync],
      segments,
      tooltip: segments.map(s => s.label).join('\n'),
      onClick: () => openSettings(SettingsCategory.Sync),
    })
  }

  // Active downloads — only show when transfers are in progress
  if (peerStore.activeDownloads.length > 0) {
    const downloads = peerStore.activeDownloads
    const totalProgress = downloads.reduce((sum, d) => sum + d.progress, 0) / downloads.length

    result.push({
      id: 'downloads',
      icon: 'i-lucide-download',
      segments: [{
        id: 'downloads',
        colorClass: 'text-primary',
        isPulsing: true,
        label: t('downloads.active', { count: downloads.length, progress: Math.round(totalProgress * 100) }),
      }],
      tooltip: t('downloads.active', { count: downloads.length, progress: Math.round(totalProgress * 100) }),
      onClick: () => openSettings(SettingsCategory.Spaces),
    })
  }

  // P2P Storage — only show when running
  {
    const isRunning = peerStore.running
    if (!isRunning) return result

    const peerCount = peerStore.spaceDevices.filter(d => d.deviceEndpointId !== peerStore.nodeId).length

    result.push({
      id: 'p2p',
      icon: 'i-mdi-lan-connect',
      segments: [{
        id: 'p2p',
        colorClass: 'text-success',
        isPulsing: peerStore.isTransferring,
        label: peerCount > 0
          ? t('p2p.active', { count: peerCount })
          : t('p2p.noPeers'),
      }],
      tooltip: peerCount > 0
        ? t('p2p.active', { count: peerCount })
        : t('p2p.noPeers'),
      onClick: () => openSettings(SettingsCategory.Spaces),
    })
  }

  return result
})
</script>

<style scoped>
@keyframes pulse-status {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

.animate-pulse-status {
  animation: pulse-status 1s ease-in-out infinite;
}
</style>

<i18n lang="yaml">
de:
  sync:
    connecting: Verbindet...
    connected: Verbunden
    syncing: Synchronisiert...
    error: Fehler
  p2p:
    stopped: "P2P gestoppt"
    noPeers: "P2P aktiv \u2014 keine Peers"
    active: "P2P aktiv \u2014 {count} Peer(s)"
  downloads:
    active: "{count} Download(s) \u2014 {progress}%"

en:
  sync:
    connecting: Connecting...
    connected: Connected
    syncing: Syncing...
    error: Error
  p2p:
    stopped: "P2P stopped"
    noPeers: "P2P active \u2014 no peers"
    active: "P2P active \u2014 {count} peer(s)"
  downloads:
    active: "{count} download(s) \u2014 {progress}%"
</i18n>
