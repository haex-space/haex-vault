<script setup lang="ts">
/**
 * The clipboard clearing time of the password manager (spec 034, FR-006): a choice in a boxed list,
 * saved as soon as it is made, without a save button (spec 023 FR-021). Stored as the vault
 * setting `passwords.clipboard_clear_seconds`; the backend accepts only these values.
 */
import { toast } from 'vue-sonner'

const open = defineModel<boolean>('open', { required: true })

const CHOICES = [0, 15, 30, 60, 120] as const
const DEFAULT_SECONDS = 30
const KEY = 'passwords.clipboard_clear_seconds'

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()

const seconds = ref<number>(DEFAULT_SECONDS)

async function loadAsync() {
  try {
    const stored = await getPrefAsync({ kind: 'vault' }, KEY)
    const value = stored === null ? DEFAULT_SECONDS : Number(stored)
    seconds.value = (CHOICES as readonly number[]).includes(value)
      ? value
      : DEFAULT_SECONDS
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function chooseAsync(value: number) {
  const previous = seconds.value
  seconds.value = value
  try {
    // action-exempt: a setting of the password manager window only; the agents get no action that
    // reads, copies or changes anything of the password manager but the title search (FR-027).
    await setPrefAsync({ kind: 'vault' }, KEY, String(value))
  } catch (cause) {
    seconds.value = previous
    toast.error(errString(cause))
  }
}

function label(value: number): string {
  return value === 0
    ? t('passwords.settings.clipboard.off')
    : t('passwords.settings.clipboard.seconds', { seconds: value })
}

watch(open, (isOpen) => {
  if (isOpen) void loadAsync()
})
onVaultTablesChanged(['preferences'], () => {
  if (open.value) return loadAsync()
})
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('passwords.settings.title')">
    <template #content>
      <SettingsGroup
        radio
        :label="t('passwords.settings.clipboard.title')"
        :model-value="seconds"
        @update:model-value="chooseAsync(Number($event))"
      >
        <SettingsOptionRow
          v-for="value in CHOICES"
          :key="value"
          type="radio"
          :value="value"
          :title="label(value)"
          :data-testid="`passwords-clipboard-${value}`"
        />
      </SettingsGroup>
      <p class="mt-2 px-1 text-sm text-muted-foreground">
        {{ t('passwords.settings.clipboard.description') }}
      </p>
    </template>
  </UiDrawerModal>
</template>
