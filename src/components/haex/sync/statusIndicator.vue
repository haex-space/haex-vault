<template>
  <div
    v-if="segments.length > 0"
    class="relative cursor-pointer"
    :class="sizeClass"
    :title="tooltipText"
    @click="openSyncSettings"
  >
    <svg
      viewBox="0 0 32 32"
      class="w-full h-full"
      style="transform: rotate(-90deg)"
    >
      <path
        v-for="segment in segments"
        :key="segment.backendId"
        :d="segment.pathData"
        fill="currentColor"
        :class="[segment.colorClass, { 'animate-pulse-sync': segment.isSyncing }]"
        class="transition-colors duration-300"
      />
    </svg>
  </div>
</template>

<script setup lang="ts">
interface Props {
  size?: 'sm' | 'md' | 'lg'
  tooltipEnabled?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  size: 'md',
  tooltipEnabled: true,
})

const syncOrchestratorStore = useSyncOrchestratorStore()
const syncBackendsStore = useSyncBackendsStore()
const windowManager = useWindowManagerStore()

const { syncStates } = storeToRefs(syncOrchestratorStore)
const { enabledBackends } = storeToRefs(syncBackendsStore)

// Circle geometry for filled pie segments
const cx = 16 // center x
const cy = 16 // center y
const radius = 14 // radius for filled circle
const gap = 3 // Gap between segments in degrees

// Size classes (using Tailwind size-* utility)
const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'size-4'
    case 'lg':
      return 'size-6'
    default:
      return 'size-5'
  }
})

// Color classes based on state (Tailwind semantic colors via text-*)
const getSegmentColorClass = (backendId: string): string => {
  const state = syncStates.value[backendId]

  if (!state) {
    return 'text-warning' // connecting
  }

  if (state.error) {
    return 'text-error' // error
  }

  if (state.isConnected) {
    return 'text-success' // connected
  }

  return 'text-warning' // connecting
}

// Check if a backend is actively syncing
const isBackendSyncing = (backendId: string): boolean => {
  const state = syncStates.value[backendId]
  return state?.isSyncing ?? false
}

// Helper to convert degrees to radians
const degreesToRadians = (degrees: number): number => {
  return (degrees * Math.PI) / 180
}

// Generate SVG path for a pie slice
const createPieSlicePath = (startAngle: number, endAngle: number): string => {
  const startRad = degreesToRadians(startAngle)
  const endRad = degreesToRadians(endAngle)

  const x1 = cx + radius * Math.cos(startRad)
  const y1 = cy + radius * Math.sin(startRad)
  const x2 = cx + radius * Math.cos(endRad)
  const y2 = cy + radius * Math.sin(endRad)

  // Large arc flag: 1 if angle > 180 degrees
  const largeArcFlag = endAngle - startAngle > 180 ? 1 : 0

  return `M ${cx} ${cy} L ${x1} ${y1} A ${radius} ${radius} 0 ${largeArcFlag} 1 ${x2} ${y2} Z`
}

// Calculate segments for the filled circle
const segments = computed(() => {
  const backends = enabledBackends.value
  if (backends.length === 0) return []

  const segmentCount = backends.length

  // Single backend: full circle, no gap
  if (segmentCount === 1) {
    const backend = backends[0]!
    return [{
      backendId: backend.id,
      name: backend.name,
      colorClass: getSegmentColorClass(backend.id),
      isSyncing: isBackendSyncing(backend.id),
      pathData: `M ${cx} ${cy} m -${radius}, 0 a ${radius},${radius} 0 1,1 ${radius * 2},0 a ${radius},${radius} 0 1,1 -${radius * 2},0`,
    }]
  }

  // Multiple backends: pie slices with gaps
  const gapDegrees = gap
  const totalGapDegrees = gapDegrees * segmentCount
  const availableDegrees = 360 - totalGapDegrees
  const segmentDegrees = availableDegrees / segmentCount

  return backends.map((backend, idx) => {
    const startAngle = idx * (segmentDegrees + gapDegrees)
    const endAngle = startAngle + segmentDegrees

    return {
      backendId: backend.id,
      name: backend.name,
      colorClass: getSegmentColorClass(backend.id),
      isSyncing: isBackendSyncing(backend.id),
      pathData: createPieSlicePath(startAngle, endAngle),
    }
  })
})

// Tooltip text showing backend states
const tooltipText = computed(() => {
  return segments.value
    .map((seg) => {
      const state = syncStates.value[seg.backendId]
      let status = 'Connecting...'
      if (state?.error) {
        status = `Error: ${state.error}`
      } else if (state?.isConnected) {
        status = state.isSyncing ? 'Syncing...' : 'Connected'
      }
      return `${seg.name}: ${status}`
    })
    .join('\n')
})

// Open settings window with sync tab
const openSyncSettings = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: 'sync' },
  })
}
</script>

<style scoped>
@keyframes pulse-sync {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.4;
  }
}

.animate-pulse-sync {
  animation: pulse-sync 1s ease-in-out infinite;
}
</style>
