<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
    show-back
    @back="$emit('back')"
  >
    <div class="space-y-4">
      <!-- Filter bar -->
      <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:gap-3">
        <UiInput
          v-model="search"
          :placeholder="t('filters.searchPlaceholder')"
          icon="i-lucide-search"
          class="flex-1"
        />
        <label class="flex items-center gap-2 text-sm">
          <UCheckbox v-model="onlyWithGaps" />
          {{ t('filters.onlyWithGaps') }}
        </label>
      </div>

      <p
        v-if="ownDevices.length === 0"
        class="text-sm text-muted"
      >
        {{ t('emptyDevices') }}
      </p>
      <p
        v-else-if="filteredSpaces.length === 0"
        class="text-sm text-muted"
      >
        {{ t('emptySpaces') }}
      </p>

      <!-- Matrix table -->
      <div
        v-else
        class="overflow-x-auto border border-default rounded-lg"
      >
        <table class="min-w-full divide-y divide-default">
          <thead class="bg-muted/30">
            <tr>
              <th class="sticky left-0 z-10 bg-muted/30 px-3 py-2 text-left text-xs font-semibold uppercase tracking-wide">
                {{ t('table.device') }}
              </th>
              <th
                v-for="space in filteredSpaces"
                :key="space.id"
                class="px-3 py-2 text-center text-xs font-semibold whitespace-nowrap"
                :title="space.name"
              >
                {{ space.name }}
              </th>
            </tr>
          </thead>
          <tbody class="divide-y divide-default">
            <tr
              v-for="device in ownDevices"
              :key="device.id"
              :data-testid="`matrix-row-${device.id}`"
            >
              <td class="sticky left-0 z-10 bg-default px-3 py-2 whitespace-nowrap">
                <div class="flex items-center gap-2">
                  <UiAvatar :seed="device.endpointId" size="sm" />
                  <div class="min-w-0">
                    <div class="text-sm font-medium truncate flex items-center gap-1.5">
                      {{ device.name }}
                      <UBadge
                        v-if="device.isCurrent"
                        color="primary"
                        variant="subtle"
                        size="xs"
                      >
                        {{ t('currentBadge') }}
                      </UBadge>
                    </div>
                    <div class="flex items-center gap-1.5 text-xs text-muted">
                      <UIcon
                        :name="platformIcon(device.platform)"
                        class="w-3.5 h-3.5"
                      />
                      <span>{{ device.platform }}</span>
                    </div>
                  </div>
                </div>
              </td>
              <td
                v-for="space in filteredSpaces"
                :key="space.id"
                class="px-3 py-2 text-center"
              >
                <UCheckbox
                  :model-value="isPublished(device.id, space.id)"
                  :disabled="
                    !device.isCurrent
                      || togglingKey === cellKey(device.id, space.id)
                  "
                  :data-testid="`matrix-cell-${device.id}-${space.id}`"
                  @update:model-value="onToggle(device, space)"
                />
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <p class="text-xs text-muted">
        {{ t('onlyCurrentDeviceHint') }}
      </p>
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { and, eq } from 'drizzle-orm'
import { haexDevices, haexIdentities, haexSpaceDevices } from '~/database/schemas'

defineEmits<{ back: [] }>()

interface DeviceRow {
  id: string
  endpointId: string
  name: string
  platform: string
  isCurrent: boolean
}

const { t } = useI18n()
const { add } = useToast()
const vaultStore = useVaultStore()
const deviceStore = useDeviceStore()
const spacesStore = useSpacesStore()
const peerStorageStore = usePeerStorageStore()
const log = createLogger('DEVICE-MATRIX')

const search = ref('')
const onlyWithGaps = ref(false)

const ownDevices = ref<DeviceRow[]>([])
const publishedKeys = ref<Set<string>>(new Set())
const togglingKey = ref<string | null>(null)

const cellKey = (deviceRowId: string, spaceId: string) =>
  `${deviceRowId}|${spaceId}`
const isPublished = (deviceRowId: string, spaceId: string) =>
  publishedKeys.value.has(cellKey(deviceRowId, spaceId))

const filteredSpaces = computed(() => {
  const q = search.value.trim().toLowerCase()
  return spacesStore.visibleSpaces.filter((space) => {
    if (q && !space.name.toLowerCase().includes(q)) return false
    if (onlyWithGaps.value) {
      const hasGap = ownDevices.value.some(
        d => !publishedKeys.value.has(cellKey(d.id, space.id)),
      )
      if (!hasGap) return false
    }
    return true
  })
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

const loadAsync = async () => {
  const db = vaultStore.currentVault?.drizzle
  if (!db) return
  // Foreign device stubs (created by the haex_space_devices_ensure_refs
  // trigger) share the haex_devices table — filter them out by joining the
  // owner identity and only keeping `source='own'` rows.
  const devs = await db
    .select({
      id: haexDevices.id,
      endpointId: haexDevices.endpointId,
      name: haexDevices.name,
      platform: haexDevices.platform,
    })
    .from(haexDevices)
    .innerJoin(haexIdentities, eq(haexIdentities.did, haexDevices.ownerDid))
    .where(eq(haexIdentities.source, 'own'))
  ownDevices.value = devs.map(d => ({
    id: d.id,
    endpointId: d.endpointId,
    name: d.name,
    platform: d.platform,
    isCurrent: d.id === deviceStore.deviceRowId,
  }))

  const publishRows = await db
    .select({ deviceId: haexSpaceDevices.deviceId, spaceId: haexSpaceDevices.spaceId })
    .from(haexSpaceDevices)
  publishedKeys.value = new Set(
    publishRows.map(r => cellKey(r.deviceId, r.spaceId)),
  )
}

onMounted(loadAsync)

const onToggle = async (device: DeviceRow, space: { id: string; name: string }) => {
  if (!device.isCurrent) {
    add({ title: t('errors.notCurrent'), color: 'warning' })
    return
  }
  const db = vaultStore.currentVault?.drizzle
  if (!db) return
  const key = cellKey(device.id, space.id)
  togglingKey.value = key
  try {
    if (publishedKeys.value.has(key)) {
      // Unpublish: DELETE the haex_space_devices row. CRDT tombstone
      // propagates to the leader, which removes us from allowed_peers.
      await db
        .delete(haexSpaceDevices)
        .where(and(
          eq(haexSpaceDevices.deviceId, device.id),
          eq(haexSpaceDevices.spaceId, space.id),
        ))
      const next = new Set(publishedKeys.value)
      next.delete(key)
      publishedKeys.value = next
    } else {
      // Publish via peer-storage helper so the row gets the right identity
      // resolution and snapshot fields.
      await peerStorageStore.registerDeviceInSpaceAsync(space.id)
      const next = new Set(publishedKeys.value)
      next.add(key)
      publishedKeys.value = next
    }
  } catch (e) {
    log.warn(`Toggle publish failed for device ${device.id} space ${space.id}:`, e)
    add({
      title: t('errors.toggleFailed', { space: space.name }),
      color: 'error',
    })
  } finally {
    togglingKey.value = null
  }
}
</script>

<i18n lang="yaml">
de:
  title: Geräte & Spaces
  description: Welche deiner Geräte sind in welchen Spaces erreichbar?
  currentBadge: dieses Gerät
  emptyDevices: Du hast noch keine Geräte registriert.
  emptySpaces: Keine Spaces für die gewählten Filter.
  onlyCurrentDeviceHint: Nur das aktuelle Gerät kann ein Häkchen setzen — andere Geräte müssen ihre eigene Vault öffnen und dort veröffentlichen.
  filters:
    searchPlaceholder: Space-Name suchen…
    onlyWithGaps: nur Spaces mit fehlenden Geräten
  table:
    device: Gerät
  errors:
    notCurrent: Nur das aktuelle Gerät kann seine eigene Veröffentlichung ändern.
    toggleFailed: 'Konnte Status in „{space}" nicht ändern.'
en:
  title: Devices & Spaces
  description: Which of your devices are reachable in which spaces?
  currentBadge: this device
  emptyDevices: You have no devices registered yet.
  emptySpaces: No spaces match the active filters.
  onlyCurrentDeviceHint: Only the current device can tick a cell — other devices must open their own vault and publish from there.
  filters:
    searchPlaceholder: Search space name…
    onlyWithGaps: only spaces with missing devices
  table:
    device: Device
  errors:
    notCurrent: Only the current device can change its own publication status.
    toggleFailed: 'Could not toggle publication in "{space}".'
</i18n>
