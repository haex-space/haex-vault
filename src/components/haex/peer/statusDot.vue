<template>
  <UTooltip :text="label">
    <span
      :class="[
        'inline-block rounded-full shrink-0',
        size === 'sm' ? 'w-1.5 h-1.5' : 'w-2 h-2',
        colorClass,
        isPulsing && 'animate-pulse-dot',
      ]"
    />
  </UTooltip>
</template>

<script setup lang="ts">
import type { PeerPingStatus } from '~/composables/usePeerPing'

const props = withDefaults(
  defineProps<{ status: PeerPingStatus; size?: 'sm' | 'md' }>(),
  { size: 'md' },
)

const { t } = useI18n()

const colorClass = computed(() => {
  switch (props.status) {
    case 'online': return 'bg-success'
    case 'offline': return 'bg-error'
    default: return 'bg-warning'
  }
})

const isPulsing = computed(() => props.status !== 'offline')

const label = computed(() => {
  switch (props.status) {
    case 'online': return t('online')
    case 'offline': return t('offline')
    default: return t('checking')
  }
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
en:
  online: Reachable
  offline: Not reachable
  checking: Checking connectivity…
</i18n>
