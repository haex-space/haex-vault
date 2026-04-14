<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
    show-back
    @back="$emit('back')"
  >
    <template #actions>
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-users"
        @click="onNavigateToSpaces"
      >
        <span class="hidden @sm:inline">{{ t('goToSpaces') }}</span>
      </UButton>
    </template>

    <!-- No Spaces -->
    <HaexSystemSettingsLayoutEmpty
      v-if="!spacesStore.visibleSpaces.length"
      :message="t('noSpaces')"
      icon="i-lucide-cloud-off"
    >
      <template #action>
        <UiButton
          variant="outline"
          icon="i-heroicons-user-group"
          @click="onNavigateToSpaces"
        >
          {{ t('goToSpaces') }}
        </UiButton>
      </template>
    </HaexSystemSettingsLayoutEmpty>

    <div
      v-else
      class="space-y-3"
    >
      <div class="grid gap-2 @md:grid-cols-[minmax(0,1fr)_16rem]">
        <UiInput
          v-model="spaceSearch"
          :placeholder="t('searchPlaceholder')"
          leading-icon="i-lucide-search"
        />
        <USelect
          v-model="selectedOwnerIdentityId"
          :items="ownerFilterOptions"
        />
      </div>

      <HaexSystemSettingsLayoutEmpty
        v-if="!filteredSpaces.length"
        :message="t('noSearchResults')"
        icon="i-lucide-search-x"
      />

      <!-- Spaces -->
      <UiListContainer v-else>
        <div
          v-for="space in filteredSpaces"
          :key="space.id"
          class="rounded-lg border border-default bg-default/40 px-3"
        >
        <UCollapsible
          :open="expandedSpaces.has(space.id)"
          :unmount-on-hide="false"
          @update:open="(val: boolean) => onToggleSpace(space.id, val)"
        >
          <!-- Space header -->
          <div class="flex items-center gap-2 py-2.5 cursor-pointer">
            <div class="flex items-center gap-2 min-w-0 flex-1">
              <UIcon
                name="i-lucide-chevron-right"
                class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
                :class="{
                  'rotate-90': expandedSpaces.has(space.id),
                }"
              />
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <span class="font-medium truncate">{{ space.name }}</span>
                  <UTooltip :text="`${t('spaceId')}: ${space.id}`">
                    <UIcon
                      name="i-lucide-info"
                      class="w-3.5 h-3.5 text-muted"
                    />
                  </UTooltip>
                  <UBadge
                    variant="subtle"
                    color="neutral"
                    size="sm"
                    :title="getOwnerDid(space.ownerIdentityId)"
                  >
                    {{ t('ownerBadge', { name: getOwnerName(space.ownerIdentityId) }) }}
                  </UBadge>
                </div>
              </div>
              <UBadge
                variant="subtle"
                size="sm"
              >
                {{ t('deviceCount', { count: getDevicesForSpace(space.id).length }) }}
              </UBadge>
              <UBadge
                variant="subtle"
                size="sm"
              >
                {{ t('shareCount', { count: getSharesForSpace(space.id).length }) }}
              </UBadge>
            </div>
            <div @click.stop>
              <UDropdownMenu
                :items="[
                  [
                    {
                      label: t('addFolder'),
                      icon: 'i-lucide-folder-plus',
                      onSelect: () => onAddShareAsync(space.id, 'folder'),
                    },
                    {
                      label: t('addFile'),
                      icon: 'i-lucide-file-plus',
                      onSelect: () => onAddShareAsync(space.id, 'file'),
                    },
                  ],
                ]"
              >
                <UiButton
                  icon="i-lucide-plus"
                  color="primary"
                  variant="solid"
                  size="xl"
                  :title="t('add')"
                />
              </UDropdownMenu>
            </div>
          </div>

          <!-- Space content -->
          <template #content>
            <div
              class="space-y-1"
              @click.stop
            >
              <!-- This device's shares -->
              <div
                v-if="getSharesForDevice(space.id, store.nodeId).length"
                class="rounded-lg overflow-hidden bg-primary/5 dark:bg-primary/10"
              >
                <div
                  class="flex items-center gap-2 px-3 py-1.5 bg-primary/10 dark:bg-primary/15"
                >
                  <UIcon
                    name="i-lucide-monitor"
                    class="w-3.5 h-3.5 text-primary shrink-0"
                  />
                  <span
                    class="text-xs font-semibold text-primary uppercase tracking-wide"
                  >
                    {{ t('thisDevice') }}
                  </span>
                  <code class="text-[11px] text-primary/80 truncate">
                    {{ store.nodeId }}
                  </code>
                </div>
                <UContextMenu
                  v-for="(share, idx) in getSharesForDevice(
                    space.id,
                    store.nodeId,
                  )"
                  :key="share.id"
                  :items="getShareContextMenuItems(share, space.id)"
                >
                  <div
                    class="group flex items-center justify-between gap-3 px-3 py-2 cursor-pointer hover:bg-primary/10 transition-colors"
                    :class="
                      idx % 2 === 1 ? 'bg-primary/2 dark:bg-primary/5' : ''
                    "
                    @click="onBrowseShare(share)"
                  >
                    <div class="flex items-center gap-3 min-w-0 flex-1">
                      <UIcon
                        :name="getShareIcon(share)"
                        class="w-4 h-4 text-primary shrink-0"
                      />
                      <div class="min-w-0 flex-1">
                        <p class="text-sm font-medium">{{ share.name }}</p>
                        <p class="text-xs text-muted truncate">
                          {{ formatPath(share.localPath) }}
                        </p>
                      </div>
                    </div>
                    <div class="shrink-0 flex items-center">
                      <UDropdownMenu
                        :items="getSyncDropdownItems(space.id, share)"
                      >
                        <UiButton
                          variant="ghost"
                          color="primary"
                          icon="i-lucide-refresh-cw"
                          class="opacity-0 group-hover:opacity-100 transition-opacity"
                          :class="{
                            'opacity-100!': getRulesForShare(share).length > 0,
                          }"
                          @click.stop
                        >
                          <UBadge
                            v-if="getRulesForShare(share).length > 0"
                            :label="String(getRulesForShare(share).length)"
                            color="primary"
                            variant="subtle"
                            size="sm"
                          />
                        </UiButton>
                      </UDropdownMenu>
                      <UiButton
                        color="error"
                        variant="ghost"
                        icon="i-lucide-trash-2"
                        class="opacity-0 group-hover:opacity-100 transition-opacity"
                        @click.stop="onRemoveShareAsync(share.id)"
                      />
                    </div>
                  </div>
                </UContextMenu>
              </div>

              <!-- Other devices' shares -->
              <div
                v-for="(
                  [deviceId, deviceShares], groupIdx
                ) in getOtherDeviceShares(space.id)"
                :key="deviceId"
                class="rounded-lg overflow-hidden"
                :class="
                  groupIdx % 2 === 0
                    ? 'bg-muted/5 dark:bg-muted/10'
                    : 'bg-muted/10 dark:bg-muted/15'
                "
              >
                <div
                  class="flex items-center gap-2 px-3 py-1.5 bg-muted/10 dark:bg-muted/15"
                >
                  <UIcon
                    name="i-lucide-smartphone"
                    class="w-3.5 h-3.5 text-muted shrink-0"
                  />
                  <span
                    class="text-xs font-semibold text-muted uppercase tracking-wide"
                  >
                    {{ getDeviceName(deviceId) || deviceId.slice(0, 12) + '…' }}
                  </span>
                  <code class="text-[11px] text-muted truncate">
                    {{ deviceId }}
                  </code>
                </div>
                <UContextMenu
                  v-for="(share, idx) in deviceShares"
                  :key="share.id"
                  :items="getShareContextMenuItems(share, space.id)"
                >
                  <div
                    class="group flex items-center justify-between gap-3 px-3 py-2 cursor-pointer hover:bg-muted/15 transition-colors"
                    :class="idx % 2 === 1 ? 'bg-muted/3 dark:bg-muted/5' : ''"
                    @click="onBrowseShare(share)"
                  >
                    <div class="flex items-center gap-3 min-w-0 flex-1">
                      <UIcon
                        :name="getShareIcon(share)"
                        class="w-4 h-4 text-muted shrink-0"
                      />
                      <p class="text-sm flex-1 truncate">{{ share.name }}</p>
                    </div>
                    <div class="shrink-0 flex items-center">
                      <UDropdownMenu
                        :items="getSyncDropdownItems(space.id, share)"
                      >
                        <UiButton
                          variant="ghost"
                          color="primary"
                          icon="i-lucide-refresh-cw"
                          class="opacity-0 group-hover:opacity-100 transition-opacity"
                          :class="{
                            'opacity-100!': getRulesForShare(share).length > 0,
                          }"
                          @click.stop
                        >
                          <UBadge
                            v-if="getRulesForShare(share).length > 0"
                            :label="String(getRulesForShare(share).length)"
                            color="primary"
                            variant="subtle"
                            size="sm"
                          />
                        </UiButton>
                      </UDropdownMenu>
                      <UiButton
                        v-if="canDeleteShare(space.id, share)"
                        color="error"
                        variant="ghost"
                        icon="i-lucide-trash-2"
                        class="opacity-0 group-hover:opacity-100 transition-opacity"
                        @click.stop="onRemoveShareAsync(share.id)"
                      />
                      <UIcon
                        v-else
                        name="i-lucide-chevron-right"
                        class="w-4 h-4 text-muted"
                      />
                    </div>
                  </div>
                </UContextMenu>
              </div>

              <!-- Empty space -->
              <div
                v-if="getSharesForSpace(space.id).length === 0"
                class="py-4 text-center text-muted text-sm"
              >
                {{ t('emptySpace') }}
              </div>
            </div>
          </template>
        </UCollapsible>
        </div>
      </UiListContainer>
    </div>

    <HaexSystemSettingsPeerStorageCreateSyncRuleDialog
      v-model:open="showSyncDialog"
      :prefill="syncPrefill"
      :edit-rule="syncEditRule"
      @created="onSyncDialogDone"
      @updated="onSyncDialogDone"
      @deleted="onSyncDialogDone"
    />
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui'
import { invoke } from '@tauri-apps/api/core'

import { SettingsCategory } from '~/config/settingsCategories'
import type {
  SelectHaexPeerShares,
  SelectHaexSyncRules,
} from '~/database/schemas'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const store = usePeerStorageStore()
const spacesStore = useSpacesStore()
const identityStore = useIdentityStore()
const syncStore = useFileSyncStore()
const windowManager = useWindowManagerStore()

const expandedSpaces = ref(new Set<string>())
const spaceCapabilities = ref(new Map<string, string[]>())
const spaceSearch = ref('')
const selectedOwnerIdentityId = ref('__all__')

const normalizedSearch = computed(() => spaceSearch.value.trim().toLowerCase())

const ownerFilterOptions = computed(() => {
  const ownerIds = [...new Set(
    spacesStore.visibleSpaces.map((space) => space.ownerIdentityId),
  )]

  return [
    {
      label: t('allOwners'),
      value: '__all__',
      icon: 'i-lucide-users',
    },
    ...ownerIds.map((identityId) => ({
      label: getOwnerName(identityId),
      value: identityId,
      icon: 'i-lucide-user-round',
    })),
  ]
})

const filteredSpaces = computed(() => {
  const query = normalizedSearch.value
  const ownerIdentityId = selectedOwnerIdentityId.value
  const ownerFilteredSpaces = ownerIdentityId === '__all__'
    ? spacesStore.visibleSpaces
    : spacesStore.visibleSpaces.filter((space) => space.ownerIdentityId === ownerIdentityId)

  if (!query) return ownerFilteredSpaces

  return ownerFilteredSpaces.filter((space) => {
    const owner = getOwnerIdentity(space.ownerIdentityId)
    const devices = getDevicesForSpace(space.id)
    const haystack = [
      space.name,
      space.id,
      owner?.name,
      owner?.did,
      ...devices.map((device) => device.deviceName),
      ...devices.map((device) => device.deviceEndpointId),
    ]
      .filter(Boolean)
      .join(' ')
      .toLowerCase()

    return haystack.includes(query)
  })
})

// -- Sync Rules per Share --
const getRulesForShare = (
  share: SelectHaexPeerShares,
): SelectHaexSyncRules[] => {
  return syncStore.syncRules.filter((rule) => {
    const cfg = rule.sourceConfig as Record<string, unknown>
    if (share.deviceEndpointId === store.nodeId) {
      // Own device: match local path
      return rule.sourceType === 'local' && cfg?.path === share.localPath
    }
    // Remote device: match peer endpointId + share name in path
    return (
      rule.sourceType === 'peer' &&
      cfg?.endpointId === share.deviceEndpointId &&
      typeof cfg?.path === 'string' &&
      (cfg.path as string).startsWith(share.name)
    )
  })
}

// Sync dialog state
const showSyncDialog = ref(false)
const syncPrefill = ref<{
  sourceType: 'local' | 'peer'
  spaceId: string
  deviceEndpointId: string
  shareName: string
  localPath?: string
} | null>(null)
const syncEditRule = ref<SelectHaexSyncRules | null>(null)

const onToggleSpace = (spaceId: string, open: boolean) => {
  const next = new Set(expandedSpaces.value)
  if (open) next.add(spaceId)
  else next.delete(spaceId)
  expandedSpaces.value = next
}

onMounted(async () => {
  await store.refreshStatusAsync()
  await store.loadSharesAsync()
  await store.loadSpaceDevicesAsync()
  await syncStore.loadRulesAsync()
  await identityStore.loadIdentitiesAsync()
  await spacesStore.loadSpacesFromDbAsync()

  // Pre-load capabilities for all visible spaces
  for (const space of spacesStore.visibleSpaces) {
    const capabilities = await spacesStore.getCapabilitiesForSpaceAsync(
      space.id,
    )
    spaceCapabilities.value.set(space.id, capabilities)
  }
})

const getSharesForSpace = (spaceId: string): SelectHaexPeerShares[] => {
  return store.shares.filter((s) => s.spaceId === spaceId)
}

const getDevicesForSpace = (spaceId: string) => {
  return store.spaceDevices.filter((d) => d.spaceId === spaceId)
}

const getOwnerIdentity = (identityId: string) => {
  return identityStore.identities.find((identity) => identity.id === identityId)
}

const getOwnerName = (identityId: string): string => {
  return getOwnerIdentity(identityId)?.name || t('unknownOwner')
}

const getOwnerDid = (identityId: string): string => {
  return getOwnerIdentity(identityId)?.did || identityId
}

const getSharesForDevice = (
  spaceId: string,
  deviceEndpointId: string,
): SelectHaexPeerShares[] => {
  return store.shares.filter(
    (s) => s.spaceId === spaceId && s.deviceEndpointId === deviceEndpointId,
  )
}

const getOtherDeviceShares = (
  spaceId: string,
): [string, SelectHaexPeerShares[]][] => {
  const spaceShares = getSharesForSpace(spaceId).filter(
    (s) => s.deviceEndpointId !== store.nodeId,
  )

  const grouped = new Map<string, SelectHaexPeerShares[]>()
  for (const share of spaceShares) {
    const existing = grouped.get(share.deviceEndpointId) || []
    existing.push(share)
    grouped.set(share.deviceEndpointId, existing)
  }

  return [...grouped.entries()]
}

const getDeviceName = (deviceEndpointId: string): string | undefined => {
  return store.spaceDevices.find((d) => d.deviceEndpointId === deviceEndpointId)
    ?.deviceName
}

const canDeleteShare = (
  spaceId: string,
  share: SelectHaexPeerShares,
): boolean => {
  if (share.deviceEndpointId === store.nodeId) return true

  const capabilities = spaceCapabilities.value.get(spaceId) ?? []
  if (
    capabilities.includes('space/admin') ||
    capabilities.includes('space/write')
  )
    return true

  const shareDevice = store.spaceDevices.find(
    (d) =>
      d.deviceEndpointId === share.deviceEndpointId && d.spaceId === spaceId,
  )
  const ownDevice = store.spaceDevices.find(
    (d) => d.deviceEndpointId === store.nodeId && d.spaceId === spaceId,
  )
  if (
    shareDevice?.identityId &&
    ownDevice?.identityId &&
    shareDevice.identityId === ownDevice.identityId
  )
    return true

  return false
}

const onNavigateToSpaces = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.Spaces },
  })
}

const formatPath = (path: string): string => {
  try {
    const parsed = JSON.parse(path)
    if (parsed.uri) {
      const decoded = decodeURIComponent(parsed.uri)
      const treeMatch = decoded.match(/tree\/[^:]+:(.+)/)
      if (treeMatch?.[1]) return treeMatch[1]
      return decoded.replace('content://', '').split('/tree/').pop() ?? decoded
    }
  } catch {
    // Not JSON — regular path
  }
  return path
}

const extractFolderName = (path: string): string => {
  try {
    const parsed = JSON.parse(path)
    if (parsed.uri) {
      const decoded = decodeURIComponent(parsed.uri)
      const treeMatch = decoded.match(/tree\/[^:]+:(.+)/)
      if (treeMatch?.[1])
        return treeMatch[1].split('/').pop() ?? 'Shared Folder'
      const lastSegment = decoded.split('/').pop() || decoded.split(':').pop()
      return lastSegment || 'Shared Folder'
    }
  } catch {
    // Not JSON — regular path
  }
  return path.split(/[/\\]/).pop() || 'Shared Folder'
}

const isFileShare = (share: SelectHaexPeerShares): boolean => {
  return share.name.includes('.') && !share.name.endsWith('/')
}

const getShareIcon = (share: SelectHaexPeerShares): string => {
  return isFileShare(share) ? 'i-lucide-file' : 'i-lucide-folder'
}

const extractFileName = (path: string): string => {
  try {
    const parsed = JSON.parse(path)
    if (parsed.uri) {
      const decoded = decodeURIComponent(parsed.uri)
      const lastSegment = decoded.split('/').pop() || decoded.split(':').pop()
      return lastSegment || 'Shared File'
    }
  } catch {
    // Not JSON — regular path
  }
  return path.split(/[/\\]/).pop() || 'Shared File'
}

const onBrowseShare = (share: SelectHaexPeerShares) => {
  const isOwnDevice = share.deviceEndpointId === store.nodeId
  const deviceName = isOwnDevice
    ? t('thisDevice')
    : getDeviceName(share.deviceEndpointId) ||
      share.deviceEndpointId.slice(0, 12) + '…'

  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'files',
    params: {
      endpointId: share.deviceEndpointId,
      peerName: deviceName,
      shareName: share.name,
      ...(isOwnDevice && { localPath: share.localPath }),
    },
  })
}

const onAddShareAsync = async (
  spaceId: string,
  type: 'folder' | 'file' = 'folder',
) => {
  const selected =
    type === 'folder'
      ? await invoke<string | null>('filesystem_select_folder', {})
      : await invoke<string | null>('filesystem_select_file', {})
  if (!selected) return

  const name =
    type === 'folder' ? extractFolderName(selected) : extractFileName(selected)

  try {
    await store.addShareAsync(spaceId, name, selected)
    add({ title: t('toast.shareAdded'), color: 'success' })
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const onSyncShare = (_spaceId: string, _share: SelectHaexPeerShares) => {
  // TODO: Implement file sync trigger (feature/file-sync)
}

const getShareContextMenuItems = (
  share: SelectHaexPeerShares,
  spaceId?: string,
) => {
  const items: ContextMenuItem[] = []
  if (spaceId) {
    items.push({
      label: t('contextMenu.sync'),
      icon: 'i-lucide-refresh-cw',
      onSelect: () => onSyncShare(spaceId, share),
    })
  }
  if (!spaceId || canDeleteShare(spaceId, share)) {
    items.push({
      label: t('contextMenu.delete'),
      icon: 'i-lucide-trash-2',
      color: 'error' as const,
      onSelect: () => onRemoveShareAsync(share.id),
    })
  }
  return items
}

const getSyncDropdownItems = (spaceId: string, share: SelectHaexPeerShares) => {
  const rules = getRulesForShare(share)
  const items: ContextMenuItem[][] = []

  if (rules.length > 0) {
    items.push(
      rules.map((rule) => {
        const tgtCfg = rule.targetConfig as Record<string, unknown>
        const targetLabel =
          rule.targetType === 'local'
            ? ((tgtCfg?.path as string) || '').split(/[/\\]/).pop() || 'Local'
            : rule.targetType === 'cloud'
              ? `S3:${(tgtCfg?.prefix as string) || '/'}`
              : 'Peer'
        return {
          label: `→ ${targetLabel}`,
          icon: rule.enabled
            ? 'i-lucide-circle-check'
            : 'i-lucide-circle-pause',
          onSelect: () => onEditRule(rule),
        }
      }),
    )
  }

  items.push([
    {
      label: t('syncDropdown.createNew'),
      icon: 'i-lucide-plus',
      onSelect: () => onCreateSyncRule(spaceId, share),
    },
  ])

  return items
}

const onCreateSyncRule = (spaceId: string, share: SelectHaexPeerShares) => {
  const isOwnDevice = share.deviceEndpointId === store.nodeId
  syncEditRule.value = null
  syncPrefill.value = {
    sourceType: isOwnDevice ? 'local' : 'peer',
    spaceId,
    deviceEndpointId: share.deviceEndpointId,
    shareName: share.name,
    localPath: isOwnDevice ? share.localPath : undefined,
  }
  showSyncDialog.value = true
}

const onEditRule = (rule: SelectHaexSyncRules) => {
  syncPrefill.value = null
  syncEditRule.value = rule
  showSyncDialog.value = true
}

const onSyncDialogDone = async () => {
  showSyncDialog.value = false
  await syncStore.loadRulesAsync()
}

const onRemoveShareAsync = async (shareId: string) => {
  try {
    await store.removeShareAsync(shareId)
    add({ title: t('toast.shareRemoved'), color: 'neutral' })
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Spaces
  description: Geteilte Dateien nach Space, Owner und Geräten durchsuchen
  endpointId: Endpoint-ID
  noSpaces: Keine Spaces vorhanden
  noSearchResults: Keine Spaces für diesen Filter gefunden
  searchPlaceholder: Nach Space-Name oder Owner suchen
  allOwners: Alle Owner
  goToSpaces: Spaces verwalten
  spaceId: Space-ID
  unknownOwner: Unbekannter Owner
  ownerBadge: 'Owner: {name}'
  deviceCount: '{count} Geräte'
  shareCount: '{count} Freigaben'
  add: Hinzufügen
  addFolder: Ordner freigeben
  addFile: Datei freigeben
  emptySpace: Noch keine Ordner oder Dateien geteilt
  thisDevice: Dieses Gerät
  error: Fehler
  syncDropdown:
    createNew: Neue Sync-Regel erstellen
  contextMenu:
    sync: Ordner syncen
    delete: Freigabe entfernen
  toast:
    shareAdded: Ordner hinzugefügt
    shareRemoved: Ordner entfernt
en:
  title: Spaces
  description: Browse shared files by space, owner, and device
  endpointId: Endpoint ID
  noSpaces: No spaces available
  noSearchResults: No spaces match this filter
  searchPlaceholder: Search by space name or owner
  allOwners: All owners
  goToSpaces: Manage Spaces
  spaceId: Space ID
  unknownOwner: Unknown owner
  ownerBadge: 'Owner: {name}'
  deviceCount: '{count} devices'
  shareCount: '{count} shares'
  add: Add
  addFolder: Share folder
  addFile: Share file
  emptySpace: No folders or files shared yet
  thisDevice: This device
  error: Error
  syncDropdown:
    createNew: Create new sync rule
  contextMenu:
    sync: Sync folder
    delete: Remove share
  toast:
    shareAdded: Folder added
    shareRemoved: Folder removed
</i18n>
