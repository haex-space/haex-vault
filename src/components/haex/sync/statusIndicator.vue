<template>
  <div
    v-if="indicators.length > 0"
    class="flex items-center gap-1.5"
  >
    <UTooltip
      v-for="item in indicators"
      :key="item.id"
      :text="item.tooltip"
    >
      <button
        class="cursor-pointer transition-colors duration-300"
        :class="sizeClass"
        @click="item.onClick"
      >
        <UIcon
          :name="item.icon"
          class="w-full h-full"
          :class="[item.colorClass, { 'animate-pulse-status': item.isPulsing }]"
        />
      </button>
    </UTooltip>
  </div>
</template>

<script setup lang="ts">
import { SettingsCategory } from '~/config/settingsCategories'

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

const openSettings = (category: SettingsCategory) => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category },
  })
}

// Build unified indicator list: sync backends + P2P
const indicators = computed(() => {
  const items: {
    id: string
    icon: string
    colorClass: string
    isPulsing: boolean
    tooltip: string
    onClick: () => void
  }[] = []

  // Sync backends
  for (const backend of enabledBackends.value) {
    const state = syncStates.value[backend.id]
    let colorClass = 'text-warning'
    let isPulsing = false
    let status = t('sync.connecting')

    if (state?.error) {
      colorClass = 'text-error'
      status = t('sync.error')
    } else if (state?.isConnected) {
      colorClass = 'text-success'
      if (state.isSyncing) {
        isPulsing = true
        status = t('sync.syncing')
      } else {
        status = t('sync.connected')
      }
    }

    items.push({
      id: `sync-${backend.id}`,
      icon: 'i-lucide-refresh-cw',
      colorClass,
      isPulsing,
      tooltip: `${backend.name}: ${status}`,
      onClick: () => openSettings(SettingsCategory.Sync),
    })
  }

  // P2P Storage
  if (peerStore.running) {
    const peerCount = peerStore.spaceDevices.filter(
      d => d.deviceEndpointId !== peerStore.nodeId,
    ).length

    items.push({
      id: 'p2p',
      icon: 'i-mdi-lan-connect',
      colorClass: peerCount > 0 ? 'text-success' : 'text-warning',
      isPulsing: false,
      tooltip: peerCount > 0
        ? t('p2p.active', { count: peerCount })
        : t('p2p.noPeers'),
      onClick: () => openSettings(SettingsCategory.PeerStorage),
    })
  }

  return items
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
    noPeers: "P2P aktiv \u2014 keine Peers"
    active: "P2P aktiv \u2014 {count} Peer(s)"

en:
  sync:
    connecting: Connecting...
    connected: Connected
    syncing: Syncing...
    error: Error
  p2p:
    noPeers: "P2P active \u2014 no peers"
    active: "P2P active \u2014 {count} peer(s)"
</i18n>
