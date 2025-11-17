<template>
  <div>
    <div class="p-6 border-b border-base-content/10">
      <h2 class="text-2xl font-bold">
        {{ t('title') }}
      </h2>
    </div>

    <div class="p-6 space-y-6">
      <UFormField :label="t('language')" :description="t('language.description')">
        <UiDropdownLocale @select="onSelectLocaleAsync" />
      </UFormField>

      <UFormField
        :label="t('vaultName.label')"
        :description="t('vaultName.description')"
      >
        <UiInput
          v-model="currentVaultName"
          :placeholder="t('vaultName.label')"
          @change="onSetVaultNameAsync"
        />
      </UFormField>

      <UFormField
        :label="t('deviceName.label')"
        :description="t('deviceName.description')"
      >
        <UiInput
          v-model="deviceName"
          :placeholder="t('deviceName.label')"
          @change="onUpdateDeviceNameAsync"
        />
      </UFormField>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { Locale } from 'vue-i18n'

const { t, setLocale } = useI18n()
const { add } = useToast()

const { currentVaultName } = storeToRefs(useVaultStore())
const { updateVaultNameAsync, updateLocaleAsync } = useVaultSettingsStore()

const { deviceName } = storeToRefs(useDeviceStore())
const { updateDeviceNameAsync, readDeviceNameAsync } = useDeviceStore()

const onSelectLocaleAsync = async (locale: Locale) => {
  await updateLocaleAsync(locale)
  await setLocale(locale)
}

const onSetVaultNameAsync = async () => {
  try {
    await updateVaultNameAsync(currentVaultName.value)
    add({ description: t('vaultName.update.success'), color: 'success' })
  } catch (error) {
    console.error(error)
    add({ description: t('vaultName.update.error'), color: 'error' })
  }
}

const onUpdateDeviceNameAsync = async () => {
  const check = vaultDeviceNameSchema.safeParse(deviceName.value)
  if (!check.success) return
  try {
    await updateDeviceNameAsync({ name: deviceName.value })
    add({ description: t('deviceName.update.success'), color: 'success' })
  } catch (error) {
    console.log(error)
    add({ description: t('deviceName.update.error'), color: 'error' })
  }
}

onMounted(async () => {
  await readDeviceNameAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Allgemein
  language: Sprache
  language.description: Wähle deine bevorzugte Sprache
  vaultName:
    label: Vaultname
    description: Der Name deiner Vault
    update:
      success: Vaultname erfolgreich aktualisiert
      error: Vaultname konnte nicht aktualisiert werden
  deviceName:
    label: Gerätename
    description: Ein Name für dieses Gerät zur besseren Identifikation
    update:
      success: Gerätename wurde erfolgreich aktualisiert
      error: Gerätename konnte nich aktualisiert werden
en:
  title: General
  language: Language
  language.description: Choose your preferred language
  vaultName:
    label: Vault Name
    description: The name of your vault
    update:
      success: Vault Name successfully updated
      error: Vault name could not be updated
  deviceName:
    label: Device name
    description: A name for this device for better identification
    update:
      success: Device name has been successfully updated
      error: Device name could not be updated
</i18n>
