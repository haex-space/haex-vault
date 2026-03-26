<template>
  <HaexSystemSettingsLayout
    :title="t('otherDevices.title')"
    :description="t('otherDevices.description')"
    show-back
    @back="$emit('back')"
  >
    <UiListContainer v-if="otherDevices.length > 0">
      <UiListItem
        v-for="device in otherDevices"
        :key="device.deviceEndpointId"
      >
        <div class="flex items-center gap-3">
          <div class="relative shrink-0">
            <UiAvatar
              :src="device.avatar"
              :seed="device.deviceEndpointId"
              :badge-src="getIdentityAvatar(device.identityId)"
              :badge-seed="device.identityId || undefined"
              size="sm"
            />
            <span
              class="absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full border border-default"
              :class="peerOnlineStatus[device.deviceEndpointId] ? 'bg-success' : 'bg-warning'"
            />
          </div>
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
        </div>
        <template
          v-if="editingDeviceId !== device.id"
          #actions
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
        </template>
      </UiListItem>
    </UiListContainer>

    <div
      v-else
      class="text-center py-8 text-muted"
    >
      {{ t('otherDevices.empty') }}
    </div>

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

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()

const deviceStore = useDeviceStore()
const peerStore = usePeerStorageStore()
const spacesStore = useSpacesStore()
const identityStore = useIdentityStore()
const { identities } = storeToRefs(identityStore)
const { deviceId } = storeToRefs(deviceStore)
const { currentVault } = storeToRefs(useVaultStore())

const getIdentityAvatar = (identityId: string | null) => {
  if (!identityId) return null
  return identities.value.find(i => i.publicKey === identityId)?.avatar ?? null
}

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

const editingDeviceId = ref<string | null>(null)
const editingName = ref('')
const peerOnlineStatus = ref<Record<string, boolean>>({})
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

const checkPeersOnlineAsync = async () => {
  if (!peerStore.running) return
  for (const device of otherDevices.value) {
    peerStore.checkPeerOnlineAsync(device.deviceEndpointId).then((online) => {
      peerOnlineStatus.value = { ...peerOnlineStatus.value, [device.deviceEndpointId]: online }
    })
  }
}

onMounted(async () => {
  await peerStore.loadSpaceDevicesAsync()
  checkPeersOnlineAsync()
})
</script>

<i18n lang="yaml">
de:
  otherDevices:
    title: Andere Geräte
    description: Geräte, die diese Vault ebenfalls geöffnet haben
    empty: Keine anderen Geräte gefunden
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
  otherDevices:
    title: Other Devices
    description: Devices that have also opened this vault
    empty: No other devices found
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
