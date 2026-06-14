<template>
  <HaexSystemSettingsLayout
    :title="t('otherDevices.title')"
    :description="t('otherDevices.description')"
    show-back
    @back="$emit('back')"
  >
    <UAccordion
      v-if="otherDevices.length > 0"
      :items="accordionItems"
      :ui="{ header: 'w-full', trigger: 'w-full flex-1', label: 'w-full flex-1' }"
    >
      <template #default="{ item }">
        <div class="flex items-center gap-3 flex-1 py-1">
          <div class="relative shrink-0">
            <UiAvatar
              :src="item.device.avatar"
              :seed="item.device.endpointId"
              :badge-src="getIdentityAvatar(item.device.identityId)"
              :badge-seed="item.device.identityId || undefined"
              size="sm"
            />
            <span
              class="absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full border border-default"
              :class="peerOnlineStatus[item.device.endpointId] ? 'bg-success' : 'bg-warning'"
            />
          </div>
          <p class="text-sm font-medium truncate">{{ item.device.name }}</p>
        </div>
      </template>
      <template #content="{ item }">
        <div class="px-4 pb-4 space-y-3">
          <component
            :is="getIdentity(item.device.identityId)?.source === 'contact' ? 'button' : 'div'"
            v-if="getIdentity(item.device.identityId)"
            class="flex items-center gap-2 text-left"
            :class="getIdentity(item.device.identityId)?.source === 'contact' ? 'hover:text-primary cursor-pointer transition-colors' : ''"
            @click="getIdentity(item.device.identityId)?.source === 'contact' && navigateToSettings(SettingsCategory.Contacts, { contactId: item.device.identityId })"
          >
            <span class="text-xs text-muted w-14 shrink-0">{{ t('identity.label') }}</span>
            <UiAvatar
              :src="getIdentity(item.device.identityId)?.avatar ?? null"
              :seed="item.device.identityId || undefined"
              size="xs"
            />
            <span class="text-sm">{{ getIdentity(item.device.identityId)?.name }}</span>
            <UIcon
              v-if="getIdentity(item.device.identityId)?.source === 'contact'"
              name="i-lucide-chevron-right"
              class="w-3 h-3 ml-auto text-muted"
            />
          </component>
          <div class="flex items-center gap-2 flex-wrap">
            <UBadge
              v-if="getSpaceName(item.device.spaceId)"
              variant="subtle"
            >
              {{ getSpaceName(item.device.spaceId) }}
            </UBadge>
            <span class="text-xs text-muted">
              {{ peerOnlineStatus[item.device.endpointId] ? t('status.online') : t('status.offline') }}
            </span>
          </div>
          <div
            v-if="editingDeviceId === item.device.id"
            class="flex items-center gap-2"
          >
            <UiInput
              v-model="editingName"
              class="flex-1"
              @keyup="onEditDeviceNameKeyup"
            />
            <UiButton
              icon="i-lucide-check"
              color="primary"
              @click="onSaveOtherDeviceNameAsync(item.device.id)"
            />
            <UiButton
              icon="i-lucide-x"
              variant="ghost"
              color="neutral"
              @click="editingDeviceId = null"
            />
          </div>
          <div
            v-else
            class="flex items-center gap-1"
          >
            <UiButton
              icon="i-lucide-pencil"
              variant="ghost"
              color="neutral"
              @click="startEditing(item.device)"
            />
            <UiButton
              icon="i-lucide-trash-2"
              variant="ghost"
              color="error"
              @click="deviceToRemove = item.device"
            />
          </div>
        </div>
      </template>
    </UAccordion>

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
      :description="t('removeDevice.description', { name: deviceToRemove?.name })"
      :confirm-label="t('removeDevice.confirm')"
      confirm-icon="i-lucide-trash-2"
      @confirm="onRemoveDeviceAsync"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { eq } from 'drizzle-orm'
import { haexSpaceDevices, haexPeerShares } from '~/database/schemas'
import { SettingsCategory } from '~/config/settingsCategories'

defineEmits<{ back: [] }>()

const tabId = inject<string>('haex-tab-id', '')
const { navigateTo: navigateToSettings } = useDrillDownNavigation<SettingsCategory>(SettingsCategory.General, 'settings-categories', tabId)

const { t } = useI18n()
const { add } = useToast()

const deviceStore = useDeviceStore()
const peerStore = usePeerStorageStore()
const spacesStore = useSpacesStore()
const identityStore = useIdentityStore()
const { identities } = storeToRefs(identityStore)
const { deviceId } = storeToRefs(deviceStore)
const { currentVault } = storeToRefs(useVaultStore())

const getIdentity = (identityId: string | null) => {
  if (!identityId) return null
  return identities.value.find(i => i.id === identityId) ?? null
}

const getIdentityAvatar = (identityId: string | null) => {
  return getIdentity(identityId)?.avatar ?? null
}

const otherDevices = computed(() => {
  const seen = new Set<string>()
  return peerStore.spaceDevices.filter(d => {
    if (d.endpointId === deviceId.value) return false
    if (seen.has(d.endpointId)) return false
    seen.add(d.endpointId)
    return true
  })
})

const getSpaceName = (spaceId: string) => {
  return spacesStore.visibleSpaces.find(s => s.id === spaceId)?.name
}

const accordionItems = computed(() =>
  otherDevices.value.map(device => ({ label: device.name, value: device.endpointId, device }))
)

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
    await db.delete(haexPeerShares).where(eq(haexPeerShares.endpointId, device.endpointId))
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
  editingName.value = device.name
}

const onEditDeviceNameKeyup = (e: KeyboardEvent) => {
  if (e.key === 'Enter' && editingDeviceId.value) onSaveOtherDeviceNameAsync(editingDeviceId.value)
  else if (e.key === 'Escape') editingDeviceId.value = null
}

const onSaveOtherDeviceNameAsync = async (id: string) => {
  const name = editingName.value.trim()
  if (!name || !currentVault.value?.drizzle) return

  try {
    await currentVault.value.drizzle
      .update(haexSpaceDevices)
      .set({ name })
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
    peerStore.checkPeerOnlineAsync(device.endpointId).then((online) => {
      peerOnlineStatus.value = { ...peerOnlineStatus.value, [device.endpointId]: online }
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
  identity:
    label: Kontakt
  status:
    online: Online
    offline: Offline
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
  identity:
    label: Contact
  status:
    online: Online
    offline: Offline
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
