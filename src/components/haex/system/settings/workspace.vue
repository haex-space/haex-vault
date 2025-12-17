<template>
  <HaexSystemSettingsLayout :title="t('title')">
    <UFormField
      :label="t('iconSize.label')"
      :description="t('iconSize.description')"
    >
      <USelect v-model="iconSizePreset" :items="iconSizePresetOptions" />
    </UFormField>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { DesktopIconSizePreset } from '~/stores/vault/settings'

const { t } = useI18n()

const desktopStore = useDesktopStore()
const { iconSizePreset } = storeToRefs(desktopStore)
const { syncDesktopIconSizeAsync, updateDesktopIconSizeAsync } = desktopStore

const iconSizePresetOptions = [
  {
    label: t('iconSize.presets.small'),
    value: DesktopIconSizePreset.small,
  },
  {
    label: t('iconSize.presets.medium'),
    value: DesktopIconSizePreset.medium,
  },
  {
    label: t('iconSize.presets.large'),
    value: DesktopIconSizePreset.large,
  },
  {
    label: t('iconSize.presets.extraLarge'),
    value: DesktopIconSizePreset.extraLarge,
  },
]

watch(iconSizePreset, async (newPreset) => {
  if (newPreset) {
    await updateDesktopIconSizeAsync(newPreset)
  }
})

onMounted(async () => {
  await syncDesktopIconSizeAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Arbeitsbereich
  iconSize:
    label: Icon-Größe
    description: Wähle die Größe der Desktop-Icons
    presets:
      small: Klein
      medium: Mittel
      large: Groß
      extraLarge: Sehr groß
en:
  title: Workspace
  iconSize:
    label: Icon Size
    description: Choose the size of desktop icons
    presets:
      small: Small
      medium: Medium
      large: Large
      extraLarge: Extra Large
</i18n>
