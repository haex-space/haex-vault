<template>
  <HaexSystemSettingsLayout
    :title="t('currentDevice.title')"
    :description="t('currentDevice.description')"
    show-back
    @back="$emit('back')"
  >
    <div class="space-y-4">
      <!-- Avatar -->
      <div class="flex items-center gap-4">
        <UiAvatarPicker
          :model-value="currentDeviceAvatar"
          :avatar-options="currentDeviceAvatarOptions"
          :seed="deviceId || 'device'"
          avatar-style="bottts"
          size="lg"
          @update:avatar-options="onUpdateAvatarOptionsAsync"
          @update:model-value="onUpdateAvatarAsync"
        />
        <span class="text-sm text-muted">{{ t('currentDevice.avatarHint') }}</span>
      </div>

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
            variant="outline"
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
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { haexDevices, haexSpaceDevices } from '~/database/schemas'
type AvatarOptions = Record<string, unknown>

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()

const deviceStore = useDeviceStore()
const { deviceId, deviceRowId, hostname, platform, deviceName } = storeToRefs(deviceStore)
const { currentVault } = storeToRefs(useVaultStore())

const isSaving = ref(false)
const currentDeviceAvatar = ref<string | null>(null)
const currentDeviceAvatarOptions = ref<AvatarOptions | null>(null)

// Picker emits avatarOptions before modelValue
const pendingOptions = ref<AvatarOptions | null | undefined>(undefined)

const onUpdateAvatarOptionsAsync = (options: AvatarOptions | null) => {
  pendingOptions.value = options
}

const onUpdateAvatarAsync = async (avatar: string | null) => {
  if (!currentVault.value?.drizzle || !deviceRowId.value) return

  const avatarOptions = pendingOptions.value !== undefined
    ? (pendingOptions.value ? JSON.stringify(pendingOptions.value) : null)
    : undefined

  // haex_devices is the canonical local source of truth for this device's
  // metadata. Write here first; haex_space_devices then mirrors it so other
  // space members see the change via CRDT sync.
  await currentVault.value.drizzle
    .update(haexDevices)
    .set({ avatar, ...(avatarOptions !== undefined ? { avatarOptions } : {}) })
    .where(eq(haexDevices.id, deviceRowId.value))

  await currentVault.value.drizzle
    .update(haexSpaceDevices)
    .set({ avatar, ...(avatarOptions !== undefined ? { avatarOptions } : {}) })
    .where(eq(haexSpaceDevices.deviceId, deviceRowId.value))

  currentDeviceAvatar.value = avatar
  if (pendingOptions.value !== undefined) {
    currentDeviceAvatarOptions.value = pendingOptions.value
  }
  pendingOptions.value = undefined
}

const onUpdateDeviceNameAsync = async () => {
  const name = deviceName.value?.trim()
  if (!name || !currentVault.value?.drizzle || !deviceRowId.value) return

  isSaving.value = true
  try {
    // Symmetric with avatar updates: write the canonical haex_devices row
    // first, then mirror to every haex_space_devices row that references it
    // so peers see the change via CRDT sync.
    await currentVault.value.drizzle
      .update(haexDevices)
      .set({ name })
      .where(eq(haexDevices.id, deviceRowId.value))

    await currentVault.value.drizzle
      .update(haexSpaceDevices)
      .set({ name })
      .where(eq(haexSpaceDevices.deviceId, deviceRowId.value))

    add({ description: t('deviceName.success'), color: 'success' })
  } catch (error) {
    console.error('Failed to update device name:', error)
    add({ description: t('deviceName.error'), color: 'error' })
  } finally {
    isSaving.value = false
  }
}

const loadDeviceNameAsync = async () => {
  if (!currentVault.value?.drizzle || !deviceRowId.value) return

  // Read from haex_devices (canonical local source) instead of
  // haex_space_devices: the latter only carries avatar/name once the device
  // has been published into at least one space, which leaves a fresh vault's
  // settings view showing seed-only fallbacks for the avatar the user just
  // confirmed in the Welcome dialog.
  const entry = await currentVault.value.drizzle.query.haexDevices.findFirst({
    where: eq(haexDevices.id, deviceRowId.value),
  })

  deviceName.value = entry?.name ?? ''
  currentDeviceAvatar.value = entry?.avatar ?? null
  if (entry?.avatarOptions) {
    try { currentDeviceAvatarOptions.value = JSON.parse(entry.avatarOptions) } catch { /* ignore */ }
  }
}

onMounted(async () => {
  await loadDeviceNameAsync()
})
</script>

<i18n lang="yaml">
de:
  currentDevice:
    title: Aktuelles Gerät
    description: Dieses Gerät wird automatisch über einen kryptographischen Schlüssel identifiziert
    avatarHint: Klicke auf den Avatar, um ihn anzupassen
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
  currentDevice:
    title: Current Device
    description: This device is automatically identified via a cryptographic key
    avatarHint: Click the avatar to customize it
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
