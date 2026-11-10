<script setup lang="ts">
/**
 * "Allgemein → Grundeinstellung" (spec 042, FR-003, FR-010): the interface language of the vault,
 * saved on selection through `settings.general.setLanguage` and applied at once on every device.
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()
const { language, options } = useLanguage()
const { errString } = useErrorString()
const setLanguage = useActionOrThrow('settings.general.setLanguage')

const busy = ref(false)

async function chooseAsync(value: string) {
  busy.value = true
  try {
    await setLanguage({ language: value })
  } catch (e) {
    toast.error(`${t('settings.language.failed')}: ${errString(e)}`)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="flex flex-col gap-2">
    <SettingsGroup>
      <SettingsRow
        :title="t('settings.language.label')"
        label-for="settings-language"
      >
        <SettingsSelect
          id="settings-language"
          :model-value="language"
          :options="options"
          :disabled="busy"
          data-testid="settings-language"
          @update:model-value="chooseAsync"
        />
      </SettingsRow>
    </SettingsGroup>
  </section>
</template>
