<script setup lang="ts">
/**
 * State mapping per docs/plans/2026-06-15-invite-outbox-resilience.md
 * Schicht 3 — PENDING+retryCount=0 means "haven't tried yet";
 * retryCount>0 means at least one transient failure. The tooltip surfaces
 * timing + last error so the user can tell "their phone is off" from
 * "their key is wrong" without opening logs.
 */
import type { SelectHaexInviteOutbox } from '~/database/schemas'
import { OutboxStatus } from '~/database/constants'
import { outboxLastAttemptAt } from '~/composables/useInviteOutbox'

const props = defineProps<{
  outbox: SelectHaexInviteOutbox | null
}>()

const { t } = useI18n()

// Tick a reactive `now` so the relative-time tooltip keeps counting down
// while the drawer stays open (Date.now() alone wouldn't re-evaluate).
const now = ref(Date.now())
useIntervalFn(() => {
  now.value = Date.now()
}, 15_000)

type BadgeColor = 'info' | 'warning' | 'success' | 'error' | 'neutral'

interface BadgeState {
  color: BadgeColor
  label: string
  detail?: string
}

function relativeDistance(date: Date): string {
  const diffMs = Math.abs(date.getTime() - now.value)
  const sec = Math.round(diffMs / 1000)
  if (sec < 1) return t('invites.outboxStatus.relativeNow')
  if (sec < 60) return t('invites.outboxStatus.relativeSeconds', { count: sec })
  const min = Math.round(sec / 60)
  if (min < 60) return t('invites.outboxStatus.relativeMinutes', { count: min })
  const hrs = Math.round(min / 60)
  return t('invites.outboxStatus.relativeHours', { count: hrs })
}

const state = computed<BadgeState | null>(() => {
  const o = props.outbox
  if (!o) return null

  if (o.status === OutboxStatus.DELIVERED) {
    return { color: 'success', label: t('invites.outboxStatus.delivered') }
  }
  if (o.status === OutboxStatus.EXPIRED) {
    return { color: 'neutral', label: t('invites.outboxStatus.expired') }
  }
  if (o.status === OutboxStatus.FAILED) {
    return {
      color: 'error',
      label: t('invites.outboxStatus.failed'),
      detail: o.lastError ?? undefined,
    }
  }
  // PENDING
  if (o.retryCount === 0) {
    return { color: 'info', label: t('invites.outboxStatus.sending') }
  }
  const nextRetryAt = o.nextRetryAt ? new Date(o.nextRetryAt) : null
  const lastAttempt = outboxLastAttemptAt(o.retryCount, o.nextRetryAt)
  const parts: string[] = []
  if (lastAttempt) {
    parts.push(t('invites.outboxStatus.waitingLast', { last: relativeDistance(lastAttempt) }))
  }
  if (nextRetryAt) {
    parts.push(t('invites.outboxStatus.waitingNext', { next: relativeDistance(nextRetryAt) }))
  }
  if (o.lastError) {
    parts.push(o.lastError)
  }
  return {
    color: 'warning',
    label: t('invites.outboxStatus.waitingForRecipient'),
    detail: parts.length > 0 ? parts.join(' · ') : undefined,
  }
})

const ariaLabel = computed(() => {
  if (!state.value) return undefined
  return state.value.detail
    ? `${state.value.label} — ${state.value.detail}`
    : state.value.label
})
</script>

<template>
  <UBadge
    v-if="state"
    :color="state.color"
    variant="subtle"
    size="xs"
    role="status"
    :aria-label="ariaLabel"
    :title="state.detail"
  >
    {{ state.label }}
  </UBadge>
</template>

<i18n lang="yaml">
de:
  invites:
    outboxStatus:
      sending: Wird gesendet
      waitingForRecipient: Warten auf Empfänger
      waitingLast: 'Letzter Versuch vor {last}'
      waitingNext: 'nächster in {next}'
      delivered: Zugestellt
      failed: Fehlgeschlagen
      expired: Einladung abgelaufen
      relativeNow: weniger als 1 s
      relativeSeconds: '{count} s'
      relativeMinutes: '{count} min'
      relativeHours: '{count} h'
en:
  invites:
    outboxStatus:
      sending: Sending
      waitingForRecipient: Waiting for recipient
      waitingLast: 'Last attempt {last} ago'
      waitingNext: 'next in {next}'
      delivered: Delivered
      failed: Failed
      expired: Invitation expired
      relativeNow: less than 1 s
      relativeSeconds: '{count} s'
      relativeMinutes: '{count} min'
      relativeHours: '{count} h'
</i18n>
