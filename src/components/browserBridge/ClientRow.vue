<script setup lang="ts">
/**
 * One approved or blocked browser extension in the settings (spec 049, US5, FR-037): its name as
 * its own claim, fingerprint, origin, grant, whether the decision is remembered, when it was made
 * and last seen, and whether it is connected. An approved one opens the editor; a blocked one
 * offers unblocking. `now` comes from the list, so all rows age together.
 */
import type { BridgeClientView } from '~/types/bindings/BridgeClientView'
import { elapsedSince } from '~/lib/sync/deviceStatus'

const props = defineProps<{
  client: BridgeClientView
  now: number
}>()

const emit = defineEmits<{
  select: []
  unblock: []
}>()

const { t } = useI18n()

const blocked = computed(() => props.client.access === null)
const fingerprint = computed(
  () => props.client.clientId.match(/.{1,4}/g)?.join(' ') ?? '',
)

function ago(at: number): string {
  const elapsed = elapsedSince(at, props.now)
  return elapsed.unit === 'justNow'
    ? t('browserBridge.time.justNow')
    : t(`browserBridge.time.${elapsed.unit}`, { n: elapsed.value })
}

const grant = computed(() => {
  const { access, scopeTags, defaultTag } = props.client
  if (!access) return ''
  const parts = [
    t(`browserBridge.access.${access}`),
    scopeTags
      ? t('browserBridge.clients.scopeTags', { tags: scopeTags.join(', ') })
      : t('browserBridge.scope.all'),
  ]
  if (defaultTag) {
    parts.push(t('browserBridge.clients.defaultTag', { tag: defaultTag }))
  }
  return parts.join(' · ')
})

const times = computed(() => {
  const parts = [
    props.client.remembered
      ? t('browserBridge.clients.remembered')
      : t('browserBridge.clients.session'),
    t('browserBridge.clients.decided', { when: ago(props.client.decidedAt) }),
  ]
  if (!blocked.value) {
    parts.push(
      props.client.lastSeenAt === null
        ? t('browserBridge.clients.neverSeen')
        : t('browserBridge.clients.lastSeen', {
            when: ago(props.client.lastSeenAt),
          }),
    )
  }
  return parts.join(' · ')
})
</script>

<template>
  <SettingsRow
    :navigates="!blocked"
    icon="lucide:puzzle"
    data-testid="bridge-client"
    :data-client-id="client.clientId"
    :data-access="client.access ?? 'blocked'"
    :data-connected="client.connected || undefined"
    @select="emit('select')"
  >
    <template #title>
      <span class="flex min-w-0 flex-wrap items-center gap-x-2">
        <span class="min-w-0 break-words font-medium">
          {{ client.clientName || t('browserBridge.client.noName') }}
        </span>
        <span class="text-xs text-muted-foreground">
          ({{ t('browserBridge.client.claimed') }})
        </span>
        <span
          v-if="client.connected"
          class="flex items-center gap-1 text-xs text-primary"
          data-testid="bridge-client-connected"
        >
          <span class="size-2 rounded-full bg-primary" aria-hidden="true" />
          {{ t('browserBridge.clients.connected') }}
        </span>
      </span>
    </template>
    <template #description>
      <span class="block font-mono text-xs break-all">{{ fingerprint }}</span>
      <span class="block font-mono text-xs break-all">
        {{ client.origin }}
      </span>
      <span v-if="grant" class="block" data-testid="bridge-client-grant">
        {{ grant }}
      </span>
      <span class="block">{{ times }}</span>
    </template>
    <UiButton
      v-if="blocked"
      type="button"
      variant="outline"
      data-testid="bridge-client-unblock"
      @click="emit('unblock')"
    >
      {{ t('browserBridge.clients.unblock') }}
    </UiButton>
  </SettingsRow>
</template>
