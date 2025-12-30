<template>
  <HaexSystemSettingsLayout :title="t('title')" :description="t('description')">
    <!-- Current Device Info -->
    <UCard>
      <template #header>
        <div class="flex items-center gap-3">
          <UIcon name="i-heroicons-device-phone-mobile" class="w-5 h-5 text-primary" />
          <div>
            <h3 class="text-lg font-semibold">{{ t('currentDevice.title') }}</h3>
            <p class="text-sm text-muted">{{ t('currentDevice.description') }}</p>
          </div>
        </div>
      </template>

      <div class="space-y-3">
        <div class="flex items-center justify-between">
          <span class="text-sm text-muted">{{ t('currentDevice.id') }}</span>
          <code class="text-xs bg-muted px-2 py-1 rounded font-mono">
            {{ currentDeviceId || t('currentDevice.unknown') }}
          </code>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-sm text-muted">{{ t('currentDevice.name') }}</span>
          <span class="font-medium">{{ currentDeviceName || t('currentDevice.unnamed') }}</span>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-sm text-muted">{{ t('currentDevice.registered') }}</span>
          <UBadge
            :color="isCurrentDeviceRegistered ? 'success' : 'warning'"
            variant="subtle"
            size="xs"
          >
            {{ isCurrentDeviceRegistered ? t('currentDevice.yes') : t('currentDevice.no') }}
          </UBadge>
        </div>
      </div>
    </UCard>

    <!-- Explanation Box -->
    <UAlert
      class="mt-4"
      color="info"
      variant="subtle"
      icon="i-heroicons-light-bulb"
      :title="t('explanation.title')"
      :description="t('explanation.description')"
    />

    <!-- All Devices List -->
    <UCard class="mt-4">
      <template #header>
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-3">
            <UIcon name="i-heroicons-device-tablet" class="w-5 h-5" />
            <div>
              <h3 class="text-lg font-semibold">{{ t('allDevices.title') }}</h3>
              <p class="text-sm text-muted">{{ t('allDevices.description') }}</p>
            </div>
          </div>
          <UButton
            color="neutral"
            variant="ghost"
            icon="i-heroicons-arrow-path"
            :loading="isLoading"
            @click="loadDevicesAsync"
          />
        </div>
      </template>

      <!-- Loading State -->
      <div v-if="isLoading" class="flex items-center justify-center py-8">
        <UIcon name="i-lucide-loader-2" class="w-6 h-6 animate-spin text-primary" />
      </div>

      <!-- Empty State -->
      <div v-else-if="devices.length === 0" class="text-center py-8 text-muted">
        <UIcon name="i-heroicons-device-phone-mobile" class="w-12 h-12 mx-auto mb-2 opacity-50" />
        <p>{{ t('allDevices.empty') }}</p>
      </div>

      <!-- Device List -->
      <div v-else class="space-y-2">
        <div
          v-for="device in devices"
          :key="device.id"
          :class="[
            'flex items-center justify-between p-3 rounded-lg border transition-colors',
            device.deviceId === currentDeviceId
              ? 'border-primary bg-primary/5'
              : 'border-default hover:bg-muted/50',
          ]"
        >
          <div class="flex items-center gap-3">
            <UIcon
              :name="device.current ? 'i-heroicons-device-phone-mobile-solid' : 'i-heroicons-device-phone-mobile'"
              :class="[
                'w-5 h-5',
                device.deviceId === currentDeviceId ? 'text-primary' : 'text-muted',
              ]"
            />
            <div>
              <div class="font-medium flex items-center gap-2">
                {{ device.name || t('allDevices.unnamed') }}
                <UBadge
                  v-if="device.deviceId === currentDeviceId"
                  color="primary"
                  variant="subtle"
                  size="xs"
                >
                  {{ t('allDevices.thisDevice') }}
                </UBadge>
                <UBadge
                  v-if="device.current"
                  color="success"
                  variant="subtle"
                  size="xs"
                >
                  {{ t('allDevices.active') }}
                </UBadge>
              </div>
              <code class="text-xs text-muted font-mono">{{ device.deviceId }}</code>
            </div>
          </div>

          <!-- Adopt Button (only for other devices) -->
          <UButton
            v-if="device.deviceId !== currentDeviceId"
            color="primary"
            variant="outline"
            size="sm"
            icon="i-heroicons-arrow-path-rounded-square"
            :loading="adoptingDeviceId === device.deviceId"
            @click="openAdoptDialog(device)"
          >
            {{ t('actions.adopt') }}
          </UButton>
        </div>
      </div>
    </UCard>

    <!-- Generate New ID -->
    <UCard class="mt-4">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <UIcon name="i-heroicons-plus-circle" class="w-5 h-5 text-muted shrink-0" />
          <div>
            <h3 class="font-medium">{{ t('generateNew.title') }}</h3>
            <p class="text-sm text-muted">{{ t('generateNew.description') }}</p>
          </div>
        </div>
        <UButton
          color="neutral"
          variant="outline"
          icon="i-heroicons-sparkles"
          @click="openGenerateNewDialog"
        >
          {{ t('generateNew.button') }}
        </UButton>
      </div>
    </UCard>

    <!-- Generate New ID Confirmation Dialog -->
    <UModal v-model:open="isGenerateNewDialogOpen">
      <template #content>
        <UCard>
          <template #header>
            <div class="flex items-center gap-2">
              <UIcon name="i-heroicons-sparkles" class="w-5 h-5 text-primary" />
              <h3 class="text-lg font-semibold">{{ t('generateNew.dialogTitle') }}</h3>
            </div>
          </template>

          <div class="space-y-4">
            <p>{{ t('generateNew.dialogDescription') }}</p>

            <UAlert
              color="warning"
              variant="subtle"
              icon="i-heroicons-exclamation-triangle"
              :title="t('generateNew.warning.title')"
              :description="t('generateNew.warning.description')"
            />
          </div>

          <template #footer>
            <div class="flex justify-end gap-2">
              <UButton
                color="neutral"
                variant="ghost"
                @click="isGenerateNewDialogOpen = false"
              >
                {{ t('actions.cancel') }}
              </UButton>
              <UButton
                color="primary"
                :loading="isGenerating"
                @click="confirmGenerateNewAsync"
              >
                {{ t('generateNew.confirm') }}
              </UButton>
            </div>
          </template>
        </UCard>
      </template>
    </UModal>

    <!-- Adopt Device Confirmation Dialog -->
    <UModal v-model:open="isAdoptDialogOpen">
      <template #content>
        <UCard>
          <template #header>
            <div class="flex items-center gap-2">
              <UIcon name="i-heroicons-exclamation-triangle" class="w-5 h-5 text-warning" />
              <h3 class="text-lg font-semibold">{{ t('adoptDialog.title') }}</h3>
            </div>
          </template>

          <div class="space-y-4">
            <p>{{ t('adoptDialog.description') }}</p>

            <div class="bg-muted rounded-lg p-3 space-y-2">
              <div class="flex justify-between text-sm">
                <span class="text-muted">{{ t('adoptDialog.deviceName') }}</span>
                <span class="font-medium">{{ deviceToAdopt?.name || t('allDevices.unnamed') }}</span>
              </div>
              <div class="flex justify-between text-sm">
                <span class="text-muted">{{ t('adoptDialog.deviceId') }}</span>
                <code class="text-xs font-mono">{{ deviceToAdopt?.deviceId }}</code>
              </div>
            </div>

            <UAlert
              color="warning"
              variant="subtle"
              icon="i-heroicons-information-circle"
              :title="t('adoptDialog.warning.title')"
              :description="t('adoptDialog.warning.description')"
            />
          </div>

          <template #footer>
            <div class="flex justify-end gap-2">
              <UButton
                color="neutral"
                variant="ghost"
                @click="isAdoptDialogOpen = false"
              >
                {{ t('actions.cancel') }}
              </UButton>
              <UButton
                color="primary"
                :loading="adoptingDeviceId !== null"
                @click="confirmAdoptAsync"
              >
                {{ t('actions.confirmAdopt') }}
              </UButton>
            </div>
          </template>
        </UCard>
      </template>
    </UModal>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { SelectHaexDevices } from '~/database/schemas'

const { t } = useI18n()
const { add } = useToast()
const localePath = useLocalePath()

const deviceStore = useDeviceStore()
const vaultStore = useVaultStore()
const { deviceId: currentDeviceId, deviceName: currentDeviceName } = storeToRefs(deviceStore)

// State
const devices = ref<SelectHaexDevices[]>([])
const isLoading = ref(false)
const isAdoptDialogOpen = ref(false)
const isGenerateNewDialogOpen = ref(false)
const deviceToAdopt = ref<SelectHaexDevices | null>(null)
const adoptingDeviceId = ref<string | null>(null)
const isGenerating = ref(false)

// Check if current device is registered in vault
const isCurrentDeviceRegistered = computed(() => {
  if (!currentDeviceId.value) return false
  return devices.value.some((d) => d.deviceId === currentDeviceId.value)
})

// Load all devices from vault
const loadDevicesAsync = async () => {
  isLoading.value = true
  try {
    const { currentVault } = useVaultStore()
    if (!currentVault?.drizzle) return

    const allDevices = await currentVault.drizzle.query.haexDevices.findMany()
    // Filter out invalid entries (Drizzle bug workaround)
    devices.value = allDevices.filter((d) => d.id)
  } catch (error) {
    console.error('Failed to load devices:', error)
    add({ color: 'error', description: t('errors.loadFailed') })
  } finally {
    isLoading.value = false
  }
}

// Open adopt confirmation dialog
const openAdoptDialog = (device: SelectHaexDevices) => {
  deviceToAdopt.value = device
  isAdoptDialogOpen.value = true
}

// Open generate new ID dialog
const openGenerateNewDialog = () => {
  isGenerateNewDialogOpen.value = true
}

// Confirm and generate new device ID
const confirmGenerateNewAsync = async () => {
  isGenerating.value = true

  try {
    // Generate new UUID and save to device.json
    const newId = await deviceStore.setDeviceIdAsync()

    // Update reactive state
    deviceStore.deviceId = newId

    isGenerateNewDialogOpen.value = false

    // Close vault and navigate to login
    await vaultStore.closeAsync()
    await navigateTo(localePath({ name: 'vaultOpen' }))
  } catch (error) {
    console.error('Failed to generate new device ID:', error)
    add({ color: 'error', description: t('errors.generateFailed') })
    isGenerating.value = false
  }
}

// Confirm and adopt the device
const confirmAdoptAsync = async () => {
  if (!deviceToAdopt.value?.deviceId) return

  adoptingDeviceId.value = deviceToAdopt.value.deviceId

  try {
    // Update local device.json with the new ID
    await deviceStore.setDeviceIdAsync(deviceToAdopt.value.deviceId)

    // Update reactive state
    deviceStore.deviceId = deviceToAdopt.value.deviceId
    deviceStore.deviceName = deviceToAdopt.value.name ?? undefined

    isAdoptDialogOpen.value = false

    // Close vault and navigate to login
    await vaultStore.closeAsync()
    await navigateTo(localePath({ name: 'vaultOpen' }))
  } catch (error) {
    console.error('Failed to adopt device:', error)
    add({ color: 'error', description: t('errors.adoptFailed') })
    adoptingDeviceId.value = null
    deviceToAdopt.value = null
  }
}

onMounted(async () => {
  await loadDevicesAsync()
  await deviceStore.readDeviceNameAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Geräte
  description: Verwalte die Geräte, die mit dieser Vault verbunden sind

  currentDevice:
    title: Aktuelles Gerät
    description: Informationen über dieses Gerät
    id: Geräte-ID
    name: Gerätename
    registered: In Vault registriert
    unknown: Unbekannt
    unnamed: Unbenannt
    yes: Ja
    no: Nein

  explanation:
    title: Wozu dient die Geräte-ID?
    description: Jede HaexVault-Installation erhält eine eindeutige Geräte-ID. Einstellungen und Erweiterungen können gerätespezifisch konfiguriert werden (z.B. Sync-Regeln). Nach einer Neuinstallation kannst du hier die alte ID übernehmen, damit alle gerätespezifischen Einstellungen wieder angewendet werden.

  generateNew:
    title: Neue Geräte-ID generieren
    description: Erstelle eine komplett neue Geräte-ID für dieses Gerät
    button: Neue ID generieren
    dialogTitle: Neue Geräte-ID generieren
    dialogDescription: Es wird eine neue, zufällige Geräte-ID erstellt. Dieses Gerät wird dann als neues Gerät in der Vault behandelt.
    confirm: Generieren
    warning:
      title: Wichtig
      description: Nach der Bestätigung wird die Vault geschlossen. Du musst deine Vault dann erneut auswählen und dich anmelden. Die alten gerätespezifischen Einstellungen werden nicht mehr angewendet.

  allDevices:
    title: Alle Geräte
    description: Alle Geräte, die in dieser Vault registriert sind
    empty: Keine Geräte in dieser Vault registriert
    thisDevice: Dieses Gerät
    active: Aktiv
    unnamed: Unbenannt

  adoptDialog:
    title: Geräte-ID übernehmen
    description: Du übernimmst die Identität eines anderen Geräts. Alle gerätespezifischen Einstellungen (Sync-Regeln, Erweiterungs-Konfigurationen) werden nach dem erneuten Öffnen der Vault angewendet.
    deviceName: Gerätename
    deviceId: Geräte-ID
    warning:
      title: Wichtig
      description: Nach der Bestätigung wird die Vault geschlossen. Du musst deine Vault dann erneut auswählen und dich anmelden. Erst dann werden die Einstellungen des gewählten Geräts aktiv.

  actions:
    adopt: Übernehmen
    cancel: Abbrechen
    confirmAdopt: Übernehmen bestätigen

  success:
    adopted:
      title: Gerät übernommen
      description: 'Du wirst jetzt als "{name}" erkannt'

  errors:
    loadFailed: Geräte konnten nicht geladen werden
    adoptFailed: Gerät konnte nicht übernommen werden
    generateFailed: Neue Geräte-ID konnte nicht generiert werden

en:
  title: Devices
  description: Manage devices connected to this vault

  currentDevice:
    title: Current Device
    description: Information about this device
    id: Device ID
    name: Device Name
    registered: Registered in Vault
    unknown: Unknown
    unnamed: Unnamed
    yes: Yes
    no: No

  explanation:
    title: What is the Device ID for?
    description: Each HaexVault installation receives a unique device ID. Settings and extensions can be configured per device (e.g., sync rules). After a reinstallation, you can adopt your old ID here so that all device-specific settings are applied again.

  generateNew:
    title: Generate New Device ID
    description: Create a completely new device ID for this device
    button: Generate New ID
    dialogTitle: Generate New Device ID
    dialogDescription: A new, random device ID will be created. This device will then be treated as a new device in the vault.
    confirm: Generate
    warning:
      title: Important
      description: After confirmation, the vault will be closed. You will need to select your vault again and log in. The old device-specific settings will no longer be applied.

  allDevices:
    title: All Devices
    description: All devices registered in this vault
    empty: No devices registered in this vault
    thisDevice: This Device
    active: Active
    unnamed: Unnamed

  adoptDialog:
    title: Adopt Device Identity
    description: You are adopting the identity of another device. All device-specific settings (sync rules, extension configurations) will be applied after reopening the vault.
    deviceName: Device Name
    deviceId: Device ID
    warning:
      title: Important
      description: After confirmation, the vault will be closed. You will need to select your vault again and log in. Only then will the selected device's settings become active.

  actions:
    adopt: Adopt
    cancel: Cancel
    confirmAdopt: Confirm Adoption

  success:
    adopted:
      title: Device Adopted
      description: 'You are now recognized as "{name}"'

  errors:
    loadFailed: Failed to load devices
    adoptFailed: Failed to adopt device
    generateFailed: Failed to generate new device ID
</i18n>
