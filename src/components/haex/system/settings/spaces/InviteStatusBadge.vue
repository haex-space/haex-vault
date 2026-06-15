<script setup lang="ts">
/**
 * Surfaces the delivery state of a targeted invite by joining the
 * `haex_invite_tokens` row the UI lists with the corresponding
 * `haex_invite_outbox` row (matched on `token_id`). Without this badge the
 * user sees "Pending invitation" indefinitely while the outbox silently
 * retries, has no idea whether the recipient ever saw it, and has no
 * recourse when a permanent rejection (auth mismatch, capability error)
 * occurs.
 *
 * State mapping mirrors docs/plans/2026-06-15-invite-outbox-resilience.md
 * Schicht 3. PENDING with retryCount=0 means "haven't tried yet";
 * retryCount>0 means at least one attempt failed transiently — typically
 * the recipient is offline. The detail tooltip carries the diagnostic
 * fields so the user can tell apart "their phone is off" from "their key
 * is wrong" without opening the logs.
 */
import type { SelectHaexInviteOutbox } from '~/database/schemas'
import { OutboxStatus } from '~/database/constants'

const props = defineProps<{
  outbox: SelectHaexInviteOutbox | null
}>()

const { t } = useI18n()

type BadgeColor = 'info' | 'warning' | 'success' | 'error' | 'neutral'

interface BadgeState {
  color: BadgeColor
  label: string
  detail?: string
}

function relativeTime(date: Date): string {
  const diffMs = date.getTime() - Date.now()
  if (diffMs <= 0) return t('invites.outboxStatus.relativeNow')
  const sec = Math.round(diffMs / 1000)
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
  let detail: string | undefined
  if (nextRetryAt && o.lastError) {
    detail = t('invites.outboxStatus.waitingDetail', {
      next: relativeTime(nextRetryAt),
      error: o.lastError,
    })
  } else if (nextRetryAt) {
    detail = t('invites.outboxStatus.waitingDetailNoError', { next: relativeTime(nextRetryAt) })
  }
  return {
    color: 'warning',
    label: t('invites.outboxStatus.waitingForRecipient'),
    detail,
  }
})
</script>

<template>
  <UBadge
    v-if="state"
    :color="state.color"
    variant="subtle"
    size="xs"
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
      waitingDetail: 'Nächster Versuch in {next} · {error}'
      waitingDetailNoError: Nächster Versuch in {next}
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
      waitingDetail: 'Next attempt in {next} · {error}'
      waitingDetailNoError: Next attempt in {next}
      delivered: Delivered
      failed: Failed
      expired: Invitation expired
      relativeNow: less than 1 s
      relativeSeconds: '{count} s'
      relativeMinutes: '{count} min'
      relativeHours: '{count} h'
</i18n>
