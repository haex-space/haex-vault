<script setup lang="ts">
/**
 * "Protokoll" (spec 098): how much the app writes into the log of the open vault. "Standard" is
 * warnings, errors and information; "Ausführlich" adds detail lines for a person with a problem
 * and ends by itself after 24 hours or at the next start. The choice applies at once, backend and
 * interface alike. Writes go through the catalog action, which no agent may call (a guardrail).
 */
import { toast } from 'vue-sonner'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import type { DiagnosticLogStatus } from '~/types/bindings/DiagnosticLogStatus'

const { t, locale } = useI18n()
const { errString } = useErrorString()
const setLevel = useActionOrThrow('settings.diagnostics.logLevel.set')

const status = ref<DiagnosticLogStatus | null>(null)
const busy = ref(false)
let disposed = false
let statusRevision = 0
let reloadRequest = 0

const options = computed<SettingsSelectOption[]>(() => [
  { value: 'standard', label: t('settings.diagnosticLog.levelStandard') },
  { value: 'verbose', label: t('settings.diagnosticLog.levelVerbose') },
])

const endsAt = computed(() => {
  const until = status.value?.untilMs
  if (status.value?.level !== 'verbose' || until == null) return null
  return new Intl.DateTimeFormat(locale.value, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(until))
})

async function reloadAsync() {
  const revision = statusRevision
  const request = ++reloadRequest
  try {
    const loaded = await invoke<DiagnosticLogStatus>('diagnostic_log_get')
    if (!disposed && revision === statusRevision && request === reloadRequest) {
      status.value = loaded
    }
  } catch (error: unknown) {
    if (!disposed) {
      toast.error(`${t('errors.prefLoadFailed')}: ${errString(error)}`)
    }
  }
}

async function chooseAsync(value: string) {
  if (status.value === null || busy.value) return
  busy.value = true
  try {
    await setLevel({ level: value })
    await reloadAsync()
  } catch (error: unknown) {
    toast.error(`${t('settings.diagnosticLog.failed')}: ${errString(error)}`)
    await reloadAsync()
  } finally {
    busy.value = false
  }
}

const unlisten: UnlistenFn[] = []

onMounted(async () => {
  const stopChanged = await listen<DiagnosticLogStatus>(
    'haex:diagnostic-log-changed',
    (event) => {
      statusRevision += 1
      status.value = event.payload
    },
  )
  if (disposed) {
    stopChanged()
    return
  }
  unlisten.push(stopChanged)

  const stopUnwritable = await listen('haex:diagnostic-log-unwritable', () => {
    statusRevision += 1
    if (status.value) status.value = { ...status.value, writable: false }
  })
  if (disposed) {
    stopUnwritable()
    return
  }
  unlisten.push(stopUnwritable)
  await reloadAsync()
})

onBeforeUnmount(() => {
  disposed = true
  for (const stop of unlisten) stop()
})
</script>

<template>
  <section class="flex flex-col gap-2">
    <SettingsGroup v-if="status !== null">
      <SettingsRow
        :title="t('settings.diagnosticLog.title')"
        :description="t('settings.diagnosticLog.description')"
        label-for="diagnostic-log-level"
      >
        <SettingsSelect
          id="diagnostic-log-level"
          :model-value="status.level"
          :options="options"
          :disabled="busy"
          data-testid="diagnostic-log-level"
          @update:model-value="chooseAsync"
        />
      </SettingsRow>
    </SettingsGroup>
    <template v-if="status !== null">
      <p
        v-if="endsAt"
        class="px-1 text-xs text-muted-foreground"
        data-testid="diagnostic-log-ends"
      >
        {{ t('settings.diagnosticLog.endsAt', { time: endsAt }) }}
      </p>
      <p
        v-if="!status.writable"
        class="px-1 text-xs text-destructive"
        data-testid="diagnostic-log-unwritable"
      >
        {{ t('settings.diagnosticLog.unwritable') }}
      </p>
      <p class="px-1 text-xs text-muted-foreground">
        {{ t('settings.diagnosticLog.location') }}
        {{ t('settings.diagnosticLog.moreLater') }}
      </p>
    </template>
  </section>
</template>
