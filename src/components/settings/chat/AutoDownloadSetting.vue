<script setup lang="ts">
/**
 * The automatic download limit of the chat (spec 072 FR-036, FR-080): files up to this size load
 * by themselves on this device, larger ones on tap; "Nie" loads nothing by itself. Saved on
 * selection; stored per device as `chat.download.autoLimit`, in bytes, 25 MB when missing. A value
 * stored by something else than these choices is kept in the list so the select shows it.
 */
import { toast } from 'vue-sonner'
import { useSettingsDevice } from '~/components/settings/deviceContext'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'

const KEY = 'chat.download.autoLimit'
const DEFAULT_LIMIT = 25_000_000
/** In bytes; the last one is the largest file the chat takes (200 MiB). */
const CHOICES = [0, 5_000_000, 25_000_000, 100_000_000, 209_715_200] as const

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()
const device = useSettingsDevice()

const scope = computed(() => ({
  kind: 'device' as const,
  uuid: device.info.value.vaultDeviceUuid,
}))

const limit = ref<number>(DEFAULT_LIMIT)
const busy = ref(false)

function megabytes(bytes: number): string {
  return t('settings.chat.autoDownload.megabytes', {
    count: Math.round(bytes / 1_000_000),
  })
}

const options = computed<SettingsSelectOption[]>(() => {
  const values: number[] = [...CHOICES]
  if (!values.includes(limit.value)) {
    values.push(limit.value)
    values.sort((a, b) => a - b)
  }
  return values.map((value) => ({
    value: String(value),
    label:
      value === 0
        ? t('settings.chat.autoDownload.never')
        : value === CHOICES[CHOICES.length - 1]
          ? t('settings.chat.autoDownload.all')
          : megabytes(value),
  }))
})

async function loadAsync() {
  try {
    const stored = await getPrefAsync(scope.value, KEY)
    const value = stored === null ? NaN : Number(stored.trim())
    limit.value =
      Number.isSafeInteger(value) && value >= 0 ? value : DEFAULT_LIMIT
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function chooseAsync(value: string) {
  if (busy.value) return
  const previous = limit.value
  limit.value = Number(value)
  busy.value = true
  try {
    // action-exempt: how much this device downloads by itself is the user's own choice; spec 072
    // gives agents no action for it.
    await setPrefAsync(scope.value, KEY, value)
  } catch (cause) {
    limit.value = previous
    toast.error(errString(cause))
  } finally {
    busy.value = false
  }
}

onMounted(loadAsync)
onVaultTablesChanged(['preferences'], () => {
  if (!busy.value) return loadAsync()
})
</script>

<template>
  <SettingsRow
    :title="t('settings.chat.autoDownload.label')"
    :description="t('settings.chat.autoDownload.hint')"
    label-for="settings-chat-auto-download"
  >
    <SettingsSelect
      id="settings-chat-auto-download"
      :model-value="String(limit)"
      :options="options"
      :disabled="busy"
      data-testid="settings-chat-auto-download"
      @update:model-value="chooseAsync"
    />
  </SettingsRow>
</template>
