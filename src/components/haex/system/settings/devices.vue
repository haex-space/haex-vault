<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <UCard>
      <template #header>
        <div class="flex items-center gap-3">
          <UIcon name="i-lucide-monitor-smartphone" class="w-8 h-8 text-primary shrink-0" />
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
    <!-- Other devices -->
    <UCard v-if="otherDevices.length > 0">
      <template #header>
        <h3 class="text-lg font-semibold">{{ t('otherDevices.title') }}</h3>
        <p class="text-sm text-muted">{{ t('otherDevices.description') }}</p>
      </template>

      <div class="space-y-3">
        <div
          v-for="device in otherDevices"
          :key="device.deviceEndpointId"
          class="flex items-center gap-3 p-3 rounded-lg bg-muted/30"
        >
          <UIcon
            name="i-lucide-monitor"
            class="w-5 h-5 text-muted shrink-0"
          />
          <div class="flex-1 min-w-0">
            <div
              v-if="editingDeviceId === device.id"
              class="flex items-center gap-2"
            >
              <UiInput
                v-model="editingName"
                class="flex-1"
                @keyup.enter="onSaveOtherDeviceNameAsync(device.id)"
                @keyup.escape="editingDeviceId = null"
              />
              <UiButton
                icon="i-lucide-check"
                color="primary"
                @click="onSaveOtherDeviceNameAsync(device.id)"
              />
              <UiButton
                icon="i-lucide-x"
                variant="ghost"
                color="neutral"
                @click="editingDeviceId = null"
              />
            </div>
            <template v-else>
              <p class="text-sm font-medium truncate">{{ device.deviceName }}</p>
              <p class="text-xs text-muted truncate font-mono">{{ device.deviceEndpointId.slice(0, 16) }}…</p>
            </template>
          </div>
          <UBadge
            v-if="getSpaceName(device.spaceId)"
            variant="subtle"
          >
            {{ getSpaceName(device.spaceId) }}
          </UBadge>
          <div
            v-if="editingDeviceId !== device.id"
            class="flex items-center gap-1 shrink-0"
          >
            <UiButton
              icon="i-lucide-pencil"
              variant="ghost"
              color="neutral"
              @click="startEditing(device)"
            />
            <UiButton
              icon="i-lucide-trash-2"
              variant="ghost"
              color="error"
              @click="deviceToRemove = device"
            />
          </div>
        </div>
      </div>
    </UCard>
    <!-- Remove device confirmation -->
    <UiDialogConfirm
      v-model:open="showRemoveDialog"
      :title="t('removeDevice.title')"
      :description="t('removeDevice.description', { name: deviceToRemove?.deviceName })"
      :confirm-label="t('removeDevice.confirm')"
      confirm-icon="i-lucide-trash-2"
      @confirm="onRemoveDeviceAsync"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { haexSpaceDevices, haexPeerShares } from '~/database/schemas'

const { t } = useI18n()
const { add } = useToast()

const deviceStore = useDeviceStore()
const peerStore = usePeerStorageStore()
const spacesStore = useSpacesStore()
const { deviceId, hostname, platform, deviceName } = storeToRefs(deviceStore)
const { currentVault } = storeToRefs(useVaultStore())

const otherDevices = computed(() => {
  const seen = new Set<string>()
  return peerStore.spaceDevices.filter(d => {
    if (d.deviceEndpointId === deviceId.value) return false
    if (seen.has(d.deviceEndpointId)) return false
    seen.add(d.deviceEndpointId)
    return true
  })
})

const getSpaceName = (spaceId: string) => {
  return spacesStore.spaces.find(s => s.id === spaceId)?.name
}

const isSaving = ref(false)
const editingDeviceId = ref<string | null>(null)
const editingName = ref('')
const deviceToRemove = ref<typeof peerStore.spaceDevices[number] | null>(null)
const showRemoveDialog = computed({
  get: () => deviceToRemove.value !== null,
  set: (v) => { if (!v) deviceToRemove.value = null },
})

const onRemoveDeviceAsync = async () => {
  const device = deviceToRemove.value
  if (!device || !currentVault.value?.drizzle) return

  try {
    const db = currentVault.value.drizzle
    // Delete shares belonging to this device
    await db.delete(haexPeerShares).where(eq(haexPeerShares.deviceEndpointId, device.deviceEndpointId))
    // Delete the device registration
    await db.delete(haexSpaceDevices).where(eq(haexSpaceDevices.id, device.id))
    await peerStore.loadSpaceDevicesAsync()
    await peerStore.loadSharesAsync()
    add({ description: t('removeDevice.success'), color: 'success' })
  } catch (error) {
    console.error('Failed to remove device:', error)
    add({ description: t('removeDevice.error'), color: 'error' })
  } finally {
    deviceToRemove.value = null
  }
}

const startEditing = (device: typeof peerStore.spaceDevices[number]) => {
  editingDeviceId.value = device.id
  editingName.value = device.deviceName
}

const onSaveOtherDeviceNameAsync = async (id: string) => {
  const name = editingName.value.trim()
  if (!name || !currentVault.value?.drizzle) return

  try {
    await currentVault.value.drizzle
      .update(haexSpaceDevices)
      .set({ deviceName: name })
      .where(eq(haexSpaceDevices.id, id))

    await peerStore.loadSpaceDevicesAsync()
    editingDeviceId.value = null
    add({ description: t('deviceName.success'), color: 'success' })
  } catch (error) {
    console.error('Failed to update device name:', error)
    add({ description: t('deviceName.error'), color: 'error' })
  }
}

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
  await Promise.all([
    loadDeviceNameAsync(),
    peerStore.loadSpaceDevicesAsync(),
  ])
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

  otherDevices:
    title: Andere Geräte
    description: Geräte, die diese Vault ebenfalls geöffnet haben

  deviceName:
    success: Gerätename wurde erfolgreich aktualisiert
    error: Gerätename konnte nicht aktualisiert werden

  removeDevice:
    title: Gerät entfernen
    description: "Möchtest du das Gerät \"{name}\" und alle zugehörigen Freigaben wirklich entfernen? Das Gerät kann sich nicht mehr verbinden, bis es sich erneut registriert."
    confirm: Entfernen
    success: Gerät wurde erfolgreich entfernt
    error: Gerät konnte nicht entfernt werden

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

  otherDevices:
    title: Other Devices
    description: Devices that have also opened this vault

  deviceName:
    success: Device name has been successfully updated
    error: Device name could not be updated

  removeDevice:
    title: Remove Device
    description: "Do you really want to remove the device \"{name}\" and all associated shares? The device will not be able to connect until it re-registers."
    confirm: Remove
    success: Device has been successfully removed
    error: Device could not be removed
</i18n>
