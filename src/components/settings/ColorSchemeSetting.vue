<script setup lang="ts">
/**
 * "Allgemein → Erscheinungsbild" (spec 023-settings-app, FR-013, FR-024, spec 042): the color
 * scheme of the vault, saved on selection (FR-021) through `settings.appearance.setColorScheme`.
 * The value comes from `useColorScheme`, which the workspace loads on open.
 */
import { toast } from 'vue-sonner'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'

const { t } = useI18n()
const { errString } = useErrorString()
const { scheme } = useColorScheme()
const setScheme = useActionOrThrow('settings.appearance.setColorScheme')

const options = computed<SettingsSelectOption[]>(() =>
  (['light', 'dark', 'system'] as const).map((value) => ({
    value,
    label: t(`settings.colorScheme.${value}`),
  })),
)

const busy = ref(false)

async function chooseAsync(value: string) {
  busy.value = true
  try {
    await setScheme({ scheme: value })
  } catch (e) {
    toast.error(`${t('settings.colorScheme.failed')}: ${errString(e)}`)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-2">
    <SettingsGroup>
      <SettingsRow
        :title="t('settings.colorScheme.label')"
        label-for="settings-color-scheme"
      >
        <SettingsSelect
          id="settings-color-scheme"
          :model-value="scheme"
          :options="options"
          :disabled="busy"
          data-testid="settings-color-scheme"
          @update:model-value="chooseAsync"
        />
      </SettingsRow>
    </SettingsGroup>
  </section>
</template>
