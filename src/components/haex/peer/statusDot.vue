<template>
  <UTooltip :text="label">
    <span
      :class="[
        'inline-block rounded-full shrink-0',
        size === 'sm' ? 'w-1.5 h-1.5' : 'w-2 h-2',
        colorClass,
        isPulsing && 'animate-pulse-dot',
      ]"
      @mouseenter="emit('hover')"
    />
  </UTooltip>
</template>

<script setup lang="ts">
import type { PeerPingStatus } from '~/composables/usePeerPing'
import type { PathType } from '~/composables/usePeerConnectionType'

const props = withDefaults(
  defineProps<{
    status: PeerPingStatus
    size?: 'sm' | 'md'
    pathType?: PathType | null
    rttMs?: number | null
  }>(),
  { size: 'md', pathType: null, rttMs: null },
)

/**
 * `hover` fires on first mouseenter — used by the file browser to trigger an
 * on-demand peer status refresh (heartbeat is sparse, 60s, so the dot may be
 * up to 60s stale when the user looks at it).
 */
const emit = defineEmits<{
  hover: []
}>()

const { t } = useI18n()

const colorClass = computed(() => {
  switch (props.status) {
    case 'online': return 'bg-success'
    case 'offline': return 'bg-error'
    default: return 'bg-warning'
  }
})

const isPulsing = computed(() => props.status !== 'offline')

const pathLabel = computed(() => {
  if (props.status !== 'online' || !props.pathType) return null
  const base = props.pathType === 'direct' ? t('direct') : props.pathType === 'relay' ? t('relay') : null
  if (!base) return null
  return props.rttMs != null ? `${base} (${props.rttMs.toFixed(0)} ms)` : base
})

const label = computed(() => {
  const base = (() => {
    switch (props.status) {
      case 'online': return t('online')
      case 'offline': return t('offline')
      default: return t('checking')
    }
  })()
  return pathLabel.value ? `${base} · ${pathLabel.value}` : base
})
</script>

<style scoped>
@keyframes pulse-dot {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.5; transform: scale(0.75); }
}

.animate-pulse-dot {
  animation: pulse-dot 1.5s ease-in-out infinite;
}
</style>

<i18n lang="yaml">
de:
  online: Erreichbar
  offline: Nicht erreichbar
  checking: Verbindung wird geprüft…
  direct: Direkt
  relay: Relay
en:
  online: Reachable
  offline: Not reachable
  checking: Checking connectivity…
  direct: Direct
  relay: Relay
</i18n>
