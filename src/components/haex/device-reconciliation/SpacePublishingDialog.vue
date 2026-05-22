<template>
  <UiDrawerModal
    :open="publishingStore.isOpen"
    :title="mode === 'new-device' ? t('newDevice.title') : t('newSpace.title')"
    :description="
      mode === 'new-device' ? t('newDevice.description') : t('newSpace.description')
    "
    :ui="{ content: 'max-w-2xl' }"
    @update:open="(v: boolean) => !v && publishingStore.close()"
  >
    <template #body>
      <div class="space-y-3">
        <!-- New-device mode: list of own spaces, user picks which to publish into -->
        <template v-if="mode === 'new-device'">
          <p
            v-if="rowsForSpaces.length === 0"
            class="text-sm text-muted"
          >
            {{ t('newDevice.empty') }}
          </p>
          <UiListContainer v-else>
            <UiListItem
              v-for="space in rowsForSpaces"
              :key="space.id"
              :data-testid="`publishing-space-${space.id}`"
              class="cursor-pointer"
              @click="toggle(space.id, selectedSpaces)"
            >
              <div class="flex items-center gap-3">
                <UCheckbox
                  :model-value="selectedSpaces.has(space.id)"
                  @click.stop="toggle(space.id, selectedSpaces)"
                />
                <div class="min-w-0">
                  <div class="text-sm font-medium truncate">
                    {{ space.name }}
                  </div>
                  <div class="text-xs text-muted">
                    {{ space.type }}
                  </div>
                </div>
              </div>
            </UiListItem>
          </UiListContainer>
        </template>

        <!-- New-space mode: list of own devices, user picks which to publish -->
        <template v-else-if="mode === 'new-space'">
          <p
            v-if="rowsForDevices.length === 0"
            class="text-sm text-muted"
          >
            {{ t('newSpace.empty') }}
          </p>
          <UiListContainer v-else>
            <UiListItem
              v-for="device in rowsForDevices"
              :key="device.id"
              :data-testid="`publishing-device-${device.id}`"
              class="cursor-pointer"
              @click="toggle(device.id, selectedDevices)"
            >
              <div class="flex items-center gap-3">
                <UCheckbox
                  :model-value="selectedDevices.has(device.id)"
                  @click.stop="toggle(device.id, selectedDevices)"
                />
                <UiAvatar
                  :seed="device.endpointId"
                  size="sm"
                />
                <div class="min-w-0">
                  <div class="text-sm font-medium truncate">
                    {{ device.name }}
                  </div>
                  <div class="flex items-center gap-2 text-xs text-muted">
                    <UIcon
                      :name="platformIcon(device.platform)"
                      class="w-3.5 h-3.5"
                    />
                    <span>{{ device.platform }}</span>
                  </div>
                </div>
              </div>
            </UiListItem>
          </UiListContainer>
        </template>
      </div>
    </template>

    <template #footer>
      <div class="flex justify-between gap-2">
        <UiButton
          variant="ghost"
          color="neutral"
          data-testid="publishing-skip"
          @click="publishingStore.close"
        >
          {{ t('skip') }}
        </UiButton>
        <UiButton
          :disabled="!canSubmit"
          :loading="submitting"
          data-testid="publishing-submit"
          @click="onSubmit"
        >
          {{ t('publish') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import { haexDevices, haexSpaceDevices } from '~/database/schemas'
import { and, eq } from 'drizzle-orm'

interface DeviceRow {
  id: string
  endpointId: string
  name: string
  platform: string
}

const publishingStore = useSpacePublishingStore()
const deviceStore = useDeviceStore()
const spacesStore = useSpacesStore()
const peerStorageStore = usePeerStorageStore()
const vaultStore = useVaultStore()
const { t } = useI18n()
const log = createLogger('PUBLISHING')

const mode = computed(() => publishingStore.mode)
const submitting = ref(false)

// `rowsForSpaces` only includes spaces the user actually owns or has joined
// (visibleSpaces already filters phantom rows).
const rowsForSpaces = computed(() => spacesStore.visibleSpaces)
const selectedSpaces = ref<Set<string>>(new Set())

const rowsForDevices = ref<DeviceRow[]>([])
const selectedDevices = ref<Set<string>>(new Set())

const canSubmit = computed(() => {
  if (submitting.value) return false
  if (mode.value === 'new-device') return selectedSpaces.value.size > 0
  if (mode.value === 'new-space') return selectedDevices.value.size > 0
  return false
})

const platformIcon = (platform: string) => {
  switch (platform) {
    case 'desktop':
      return 'i-lucide-monitor'
    case 'android':
    case 'ios':
      return 'i-lucide-smartphone'
    default:
      return 'i-lucide-cpu'
  }
}

const toggle = (id: string, set: Set<string>) => {
  if (set.has(id)) set.delete(id)
  else set.add(id)
  // trigger reactivity
  if (set === selectedSpaces.value) selectedSpaces.value = new Set(set)
  else selectedDevices.value = new Set(set)
}

const loadOwnDevicesAsync = async () => {
  const db = vaultStore.currentVault?.drizzle
  if (!db) {
    rowsForDevices.value = []
    return
  }
  const rows = await db.select().from(haexDevices)
  rowsForDevices.value = rows.map(r => ({
    id: r.id,
    endpointId: r.endpointId,
    name: r.name,
    platform: r.platform,
  }))
}

// When the dialog opens, prime defaults (everything checked) for the right mode.
watch(
  () => publishingStore.isOpen,
  async (open) => {
    if (!open) return
    if (publishingStore.mode === 'new-device') {
      selectedSpaces.value = new Set(rowsForSpaces.value.map(s => s.id))
    } else if (publishingStore.mode === 'new-space') {
      await loadOwnDevicesAsync()
      selectedDevices.value = new Set(rowsForDevices.value.map(d => d.id))
    }
  },
)

const alreadyPublishedAsync = async (spaceId: string, deviceRowId: string) => {
  const db = vaultStore.currentVault?.drizzle
  if (!db) return false
  const rows = await db
    .select({ id: haexSpaceDevices.id })
    .from(haexSpaceDevices)
    .where(and(
      eq(haexSpaceDevices.spaceId, spaceId),
      eq(haexSpaceDevices.deviceId, deviceRowId),
    ))
    .limit(1)
  return rows.length > 0
}

const onSubmit = async () => {
  if (!canSubmit.value) return
  submitting.value = true
  try {
    if (publishingStore.mode === 'new-device') {
      // Publish the (just-created) current device into each selected space.
      for (const spaceId of selectedSpaces.value) {
        if (await alreadyPublishedAsync(spaceId, deviceStore.deviceRowId)) continue
        try {
          await peerStorageStore.registerDeviceInSpaceAsync(spaceId)
        } catch (e) {
          log.warn(`publish device into space ${spaceId} failed:`, e)
        }
      }
    } else if (publishingStore.mode === 'new-space' && publishingStore.targetSpaceId) {
      const spaceId = publishingStore.targetSpaceId
      // Publish each selected device into the new space. Only the current
      // device can be published from this vault — the other rows are still
      // visible in the list so the user knows what else exists, but the
      // actual INSERT requires that device's secret key, which only its
      // owning vault has. We therefore only call registerDeviceInSpaceAsync
      // when the current device is in the selection.
      if (selectedDevices.value.has(deviceStore.deviceRowId)) {
        if (!(await alreadyPublishedAsync(spaceId, deviceStore.deviceRowId))) {
          await peerStorageStore.registerDeviceInSpaceAsync(spaceId)
        }
      }
      // For other selected devices the user would need to confirm publishing
      // on each of them in turn (or via Personal Sync replicating the
      // haex_devices row). The Geräte-&-Spaces settings page surfaces those
      // outstanding rows.
    }
    publishingStore.close()
  } finally {
    submitting.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  publish: Veröffentlichen
  skip: Überspringen
  newDevice:
    title: Gerät in Spaces veröffentlichen
    description: In welchen Spaces soll dieses Gerät für andere erreichbar sein?
    empty: Du hast noch keine Spaces. Du kannst dieses Gerät später in den Einstellungen freigeben.
  newSpace:
    title: Geräte in diesem Space veröffentlichen
    description: Welche deiner Geräte sollen in diesem Space erreichbar sein?
    empty: Du hast noch keine Geräte registriert.
en:
  publish: Publish
  skip: Skip
  newDevice:
    title: Publish device in spaces
    description: In which spaces should this device be reachable for others?
    empty: You have no spaces yet. You can publish this device later from settings.
  newSpace:
    title: Publish devices in this space
    description: Which of your devices should be reachable in this space?
    empty: You have no devices registered yet.
</i18n>
