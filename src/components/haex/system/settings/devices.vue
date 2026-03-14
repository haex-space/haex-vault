<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <UCard>
      <template #header>
        <div class="flex items-center gap-3">
          <UIcon name="i-heroicons-device-phone-mobile" class="w-5 h-5 text-primary shrink-0" />
          <div>
            <h3 class="text-lg font-semibold">{{ t('currentDevice.title') }}</h3>
            <p class="text-sm text-muted">{{ t('currentDevice.description') }}</p>
          </div>
        </div>
      </template>

      <div class="space-y-4">
        <UFormField
          :label="t('currentDevice.name')"
          data-tour="settings-device-name"
        >
          <div class="flex items-center gap-2">
            <UiInput
              v-model="deviceName"
              :placeholder="t('currentDevice.namePlaceholder')"
              class="flex-1"
            />
            <UiButton
              icon="i-mdi-content-save"
              color="primary"
              :loading="isSaving"
              :disabled="!deviceName?.trim()"
              @click="onUpdateDeviceNameAsync"
            />
          </div>
        </UFormField>

        <div class="flex items-center justify-between">
          <span class="text-sm text-muted">{{ t('currentDevice.endpointId') }}</span>
          <code class="text-xs bg-muted px-2 py-1 rounded font-mono truncate max-w-[200px]">
            {{ deviceId || t('currentDevice.unknown') }}
          </code>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-sm text-muted">{{ t('currentDevice.hostname') }}</span>
          <span class="font-medium">{{ hostname || t('currentDevice.unknown') }}</span>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-sm text-muted">{{ t('currentDevice.platform') }}</span>
          <span class="font-medium capitalize">{{ platform || t('currentDevice.unknown') }}</span>
        </div>
      </div>
    </UCard>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { haexSpaceDevices } from '~/database/schemas'

const { t } = useI18n()
const { add } = useToast()

const deviceStore = useDeviceStore()
const { deviceId, hostname, platform, deviceName } = storeToRefs(deviceStore)
const { currentVault } = storeToRefs(useVaultStore())

const isSaving = ref(false)

const onUpdateDeviceNameAsync = async () => {
  const name = deviceName.value?.trim()
  if (!name || !currentVault.value?.drizzle || !deviceId.value) return

  isSaving.value = true
  try {
    const existing = await currentVault.value.drizzle.query.haexSpaceDevices.findFirst({
      where: eq(haexSpaceDevices.deviceEndpointId, deviceId.value),
    })

    if (existing) {
      await currentVault.value.drizzle
        .update(haexSpaceDevices)
        .set({ deviceName: name })
        .where(eq(haexSpaceDevices.deviceEndpointId, deviceId.value))
    }

    add({ description: t('deviceName.success'), color: 'success' })
  } catch (error) {
    console.error('Failed to update device name:', error)
    add({ description: t('deviceName.error'), color: 'error' })
  } finally {
    isSaving.value = false
  }
}

const loadDeviceNameAsync = async () => {
  if (!currentVault.value?.drizzle || !deviceId.value) return

  const entry = await currentVault.value.drizzle.query.haexSpaceDevices.findFirst({
    where: eq(haexSpaceDevices.deviceEndpointId, deviceId.value),
  })

  deviceName.value = entry?.deviceName ?? ''
}

onMounted(async () => {
  await loadDeviceNameAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Gerät
  description: Informationen über dieses Gerät

  currentDevice:
    title: Aktuelles Gerät
    description: Dieses Gerät wird automatisch über einen kryptographischen Schlüssel identifiziert
    name: Gerätename
    namePlaceholder: z.B. Mein Laptop
    endpointId: Endpoint-ID
    hostname: Hostname
    platform: Plattform
    unknown: Unbekannt

  deviceName:
    success: Gerätename wurde erfolgreich aktualisiert
    error: Gerätename konnte nicht aktualisiert werden

en:
  title: Device
  description: Information about this device

  currentDevice:
    title: Current Device
    description: This device is automatically identified via a cryptographic key
    name: Device Name
    namePlaceholder: e.g. My Laptop
    endpointId: Endpoint ID
    hostname: Hostname
    platform: Platform
    unknown: Unknown

  deviceName:
    success: Device name has been successfully updated
    error: Device name could not be updated
</i18n>
