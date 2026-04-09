<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    show-back
    @back="$emit('back')"
  >
    <template #description>
      <span v-if="store.nodeId" class="flex items-center gap-1.5">
        {{ t('endpointId') }}: <code class="font-mono truncate">{{ store.nodeId }}</code>
        <UButton
          icon="i-lucide-copy"
          color="neutral"
          variant="ghost"
          size="xs"
          class="shrink-0"
          @click="copyEndpointId"
        />
      </span>
      <span v-else>{{ t('description') }}</span>
    </template>
    <template #actions>
      <UButton
        color="neutral"
        variant="outline"
        icon="i-lucide-users"
        @click="onNavigateToSpaces"
      >
        <span class="hidden @sm:inline">{{ t('goToSpaces') }}</span>
      </UButton>
      <UiButton
        :icon="store.running ? 'i-lucide-power-off' : 'i-lucide-power'"
        :color="store.running ? 'error' : 'primary'"
        :loading="isToggling"
        @click="onToggleEndpointAsync"
      >
        {{ store.running ? t('actions.stop') : t('actions.start') }}
      </UiButton>
      <div class="basis-full">
        <UCheckbox
          v-model="autostart"
          :label="t('autostart')"
          @update:model-value="onToggleAutostartAsync"
        />
      </div>
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

    <!-- Spaces -->
    <UiListContainer v-else>
      <div
        v-for="space in spacesStore.visibleSpaces"
        :key="space.id"
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
                :class="{ 'rotate-90': expandedSpaces.has(space.id) }"
              />
              <span class="font-medium truncate">{{ space.name }}</span>
              <UBadge variant="subtle" size="sm">
                {{ getSharesForSpace(space.id).length }}
              </UBadge>
            </div>
            <div @click.stop>
              <UDropdownMenu
                :items="[
                  [
                    { label: t('addFolder'), icon: 'i-lucide-folder-plus', onSelect: () => onAddShareAsync(space.id, 'folder') },
                    { label: t('addFile'), icon: 'i-lucide-file-plus', onSelect: () => onAddShareAsync(space.id, 'file') },
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
            <div class="space-y-1" @click.stop>
              <!-- This device's shares -->
              <div
                v-if="getSharesForDevice(space.id, store.nodeId).length"
                class="rounded-lg overflow-hidden bg-primary/5 dark:bg-primary/10"
              >
                <div class="flex items-center gap-2 px-3 py-1.5 bg-primary/10 dark:bg-primary/15">
                  <UIcon name="i-lucide-monitor" class="w-3.5 h-3.5 text-primary shrink-0" />
                  <span class="text-xs font-semibold text-primary uppercase tracking-wide">
                    {{ t('thisDevice') }}
                  </span>
                </div>
                <UContextMenu
                  v-for="(share, idx) in getSharesForDevice(space.id, store.nodeId)"
                  :key="share.id"
                  :items="getShareContextMenuItems(share, space.id)"
                >
                  <div
                    class="group flex items-center justify-between gap-3 px-3 py-2 cursor-pointer hover:bg-primary/10 transition-colors"
                    :class="idx % 2 === 1 ? 'bg-primary/2 dark:bg-primary/5' : ''"
                    @click="onBrowseShare(share)"
                  >
                    <div class="flex items-center gap-3 min-w-0 flex-1">
                      <UIcon :name="getShareIcon(share)" class="w-4 h-4 text-primary shrink-0" />
                      <div class="min-w-0 flex-1">
                        <p class="text-sm font-medium">{{ share.name }}</p>
                        <p class="text-xs text-muted truncate">{{ formatPath(share.localPath) }}</p>
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
                          @click.stop
                          class="opacity-0 group-hover:opacity-100 transition-opacity"
                          :class="{ 'opacity-100!': getRulesForShare(share).length > 0 }"
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
                v-for="([deviceId, deviceShares], groupIdx) in getOtherDeviceShares(space.id)"
                :key="deviceId"
                class="rounded-lg overflow-hidden"
                :class="groupIdx % 2 === 0 ? 'bg-muted/5 dark:bg-muted/10' : 'bg-muted/10 dark:bg-muted/15'"
              >
                <div class="flex items-center gap-2 px-3 py-1.5 bg-muted/10 dark:bg-muted/15">
                  <UIcon name="i-lucide-smartphone" class="w-3.5 h-3.5 text-muted shrink-0" />
                  <span class="text-xs font-semibold text-muted uppercase tracking-wide">
                    {{ getDeviceName(deviceId) || deviceId.slice(0, 12) + '…' }}
                  </span>
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
                      <UIcon :name="getShareIcon(share)" class="w-4 h-4 text-muted shrink-0" />
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
                          @click.stop
                          class="opacity-0 group-hover:opacity-100 transition-opacity"
                          :class="{ 'opacity-100!': getRulesForShare(share).length > 0 }"
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
                      <UIcon v-else name="i-lucide-chevron-right" class="w-4 h-4 text-muted" />
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
import { SettingsCategory } from '~/config/settingsCategories'
import { and, eq, isNull } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import type { SelectHaexPeerShares, SelectHaexSyncRules } from '~/database/schemas'
import { haexVaultSettings } from '~/database/schemas'
import { VaultSettingsKeyEnum } from '~/config/vault-settings'

defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const { copy } = useClipboard()
const store = usePeerStorageStore()
const spacesStore = useSpacesStore()
const syncStore = useFileSyncStore()
const windowManager = useWindowManagerStore()
const { currentVault } = storeToRefs(useVaultStore())

const isToggling = ref(false)
const autostart = ref(false)
const expandedSpaces = ref(new Set<string>())
const spaceCapabilities = ref(new Map<string, string[]>())

// -- Sync Rules per Share --
const getRulesForShare = (share: SelectHaexPeerShares): SelectHaexSyncRules[] => {
  return syncStore.syncRules.filter((rule) => {
    const cfg = rule.sourceConfig as Record<string, unknown>
    if (share.deviceEndpointId === store.nodeId) {
      // Own device: match local path
      return rule.sourceType === 'local' && cfg?.path === share.localPath
    }
    // Remote device: match peer endpointId + share name in path
    return rule.sourceType === 'peer'
      && cfg?.endpointId === share.deviceEndpointId
      && typeof cfg?.path === 'string'
      && (cfg.path as string).startsWith(share.name)
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

const deviceStore = useDeviceStore()

const onToggleAutostartAsync = async (value: boolean | 'indeterminate') => {
  if (value === 'indeterminate') return
  if (!currentVault.value?.drizzle) return
  if (!deviceStore.deviceId) return

  try {
    const existing =
      await currentVault.value.drizzle.query.haexVaultSettings.findFirst({
        where: and(
          eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
          eq(haexVaultSettings.deviceId, deviceStore.deviceId),
        ),
      })

    if (existing) {
      await currentVault.value.drizzle
        .update(haexVaultSettings)
        .set({ value: value ? 'true' : 'false' })
        .where(eq(haexVaultSettings.id, existing.id))
    } else {
      await currentVault.value.drizzle.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.peerStorageAutostart,
        deviceId: deviceStore.deviceId,
        value: value ? 'true' : 'false',
      })
    }
  } catch (error) {
    console.error('Failed to save autostart setting:', error)
    add({ description: t('error'), color: 'error' })
  }
}

onMounted(async () => {
  await store.refreshStatusAsync()
  await store.loadSharesAsync()
  await store.loadSpaceDevicesAsync()
  await syncStore.loadRulesAsync()

  // Pre-load capabilities for all visible spaces
  for (const space of spacesStore.visibleSpaces) {
    const capabilities = await spacesStore.getCapabilitiesForSpaceAsync(space.id)
    spaceCapabilities.value.set(space.id, capabilities)
  }

  if (currentVault.value?.drizzle && deviceStore.deviceId) {
    const row =
      await currentVault.value.drizzle.query.haexVaultSettings.findFirst({
        where: and(
          eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
          eq(haexVaultSettings.deviceId, deviceStore.deviceId),
        ),
      })
    autostart.value = row?.value === 'true'
  }
})

const getSharesForSpace = (spaceId: string): SelectHaexPeerShares[] => {
  return store.shares.filter((s) => s.spaceId === spaceId)
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

const canDeleteShare = (spaceId: string, share: SelectHaexPeerShares): boolean => {
  if (share.deviceEndpointId === store.nodeId) return true

  const capabilities = spaceCapabilities.value.get(spaceId) ?? []
  if (capabilities.includes('space/admin') || capabilities.includes('space/write')) return true

  const shareDevice = store.spaceDevices.find(
    (d) => d.deviceEndpointId === share.deviceEndpointId && d.spaceId === spaceId,
  )
  const ownDevice = store.spaceDevices.find(
    (d) => d.deviceEndpointId === store.nodeId && d.spaceId === spaceId,
  )
  if (shareDevice?.identityId && ownDevice?.identityId
    && shareDevice.identityId === ownDevice.identityId) return true

  return false
}

const onNavigateToSpaces = () => {
  windowManager.openWindowAsync({
    type: 'system',
    sourceId: 'settings',
    params: { category: SettingsCategory.Spaces },
  })
}

const onToggleEndpointAsync = async () => {
  isToggling.value = true
  try {
    if (store.running) {
      await store.stopAsync()
      add({ title: t('toast.stopped'), color: 'neutral' })
    } else {
      await store.startAsync()
      add({ title: t('toast.started'), color: 'success' })
    }
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  } finally {
    isToggling.value = false
  }
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

const onAddShareAsync = async (spaceId: string, type: 'folder' | 'file' = 'folder') => {
  const selected = type === 'folder'
    ? await invoke<string | null>('filesystem_select_folder', {})
    : await invoke<string | null>('filesystem_select_file', {})
  if (!selected) return

  const name = type === 'folder' ? extractFolderName(selected) : extractFileName(selected)

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

const copyEndpointId = async () => {
  await copy(store.nodeId)
  add({ title: t('toast.copied'), color: 'success' })
}

const getShareContextMenuItems = (share: SelectHaexPeerShares, spaceId?: string) => {
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
    items.push(rules.map((rule) => {
      const tgtCfg = rule.targetConfig as Record<string, unknown>
      const targetLabel = rule.targetType === 'local'
        ? ((tgtCfg?.path as string) || '').split(/[/\\]/).pop() || 'Local'
        : rule.targetType === 'cloud'
          ? `S3:${(tgtCfg?.prefix as string) || '/'}`
          : 'Peer'
      return {
        label: `→ ${targetLabel}`,
        icon: rule.enabled ? 'i-lucide-circle-check' : 'i-lucide-circle-pause',
        onSelect: () => onEditRule(rule),
      }
    }))
  }

  items.push([{
    label: t('syncDropdown.createNew'),
    icon: 'i-lucide-plus',
    onSelect: () => onCreateSyncRule(spaceId, share),
  }])

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
  title: Verbindung
  description: P2P-Endpoint und geteilte Ordner verwalten
  actions:
    start: Starten
    stop: Stoppen
  endpointId: Endpoint-ID
  autostart: Automatisch starten wenn die Vault geöffnet wird
  noSpaces: Keine Spaces vorhanden
  goToSpaces: Spaces verwalten
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
    copied: Endpoint-ID kopiert
    started: P2P-Endpoint gestartet
    stopped: P2P-Endpoint gestoppt
    shareAdded: Ordner hinzugefügt
    shareRemoved: Ordner entfernt
en:
  title: Connection
  description: Manage P2P endpoint and shared folders
  actions:
    start: Start
    stop: Stop
  endpointId: Endpoint ID
  autostart: Automatically start when the vault is opened
  noSpaces: No spaces available
  goToSpaces: Manage Spaces
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
    copied: Endpoint ID copied
    started: P2P endpoint started
    stopped: P2P endpoint stopped
    shareAdded: Folder added
    shareRemoved: Folder removed
</i18n>
