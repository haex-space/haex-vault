<template>
  <div
    v-if="spaceShares.length === 0"
    class="text-xs text-muted text-center py-3"
  >
    {{ t('empty') }}
  </div>

  <div
    v-else
    class="rounded-md overflow-hidden bg-gray-100/50 dark:bg-gray-700/30"
  >
    <UCollapsible :unmount-on-hide="false">
      <!-- Collapsible trigger: group header -->
      <div
        class="flex items-center gap-2 px-2.5 py-2.5 text-xs font-semibold text-muted uppercase tracking-wide cursor-pointer hover:text-foreground transition-colors"
      >
        <UIcon
          name="i-lucide-chevron-right"
          class="w-3 h-3 shrink-0 transition-transform duration-200 [[data-state=open]>&]:rotate-90"
        />
        <UIcon
          name="i-lucide-hard-drive"
          class="w-3.5 h-3.5 shrink-0"
        />
        <span class="truncate">{{ t('title') }}</span>
        <UBadge
          variant="subtle"
          size="sm"
          color="neutral"
        >
          {{ spaceShares.length }}
        </UBadge>
      </div>

      <template #content>
        <div class="space-y-2 p-2">
    <!-- This device's shares -->
    <div
      v-if="ownShares.length"
      class="rounded-lg overflow-hidden bg-primary/5 dark:bg-primary/10"
    >
      <UCollapsible :unmount-on-hide="false">
      <div
        class="flex items-center gap-2 px-3 py-1.5 bg-primary/10 dark:bg-primary/15 cursor-pointer hover:bg-primary/15 dark:hover:bg-primary/20 transition-colors"
      >
        <UIcon
          name="i-lucide-chevron-right"
          class="w-3 h-3 text-primary shrink-0 transition-transform duration-200 [[data-state=open]>&]:rotate-90"
        />
        <UIcon
          name="i-lucide-monitor"
          class="w-3.5 h-3.5 text-primary shrink-0"
        />
        <span
          class="text-xs font-semibold text-primary uppercase tracking-wide"
        >
          {{ t('thisDevice') }}
        </span>
        <code
          v-if="peerStore.nodeId"
          class="text-[11px] text-primary/80 truncate"
        >
          {{ peerStore.nodeId }}
        </code>
      </div>
      <template #content>
      <UContextMenu
        v-for="(share, idx) in ownShares"
        :key="share.id"
        :items="shareContextMenuItems(share)"
      >
        <div
          class="group flex items-center justify-between gap-3 px-3 py-2 cursor-pointer hover:bg-primary/10 transition-colors"
          :class="idx % 2 === 1 ? 'bg-primary/2 dark:bg-primary/5' : ''"
          @click="onBrowseShare(share)"
        >
          <div class="flex items-center gap-3 min-w-0 flex-1">
            <UIcon
              :name="getShareIcon(share)"
              class="w-4 h-4 text-primary shrink-0"
            />
            <div class="min-w-0 flex-1">
              <p class="text-sm font-medium truncate">{{ share.name }}</p>
              <p class="text-xs text-muted truncate">
                {{ formatPath(share.localPath) }}
              </p>
            </div>
          </div>
          <div class="shrink-0 flex items-center">
            <UDropdownMenu :items="syncDropdownItems(share)">
              <UiButton
                variant="ghost"
                color="primary"
                icon="i-lucide-refresh-cw"
                class="opacity-0 group-hover:opacity-100 transition-opacity"
                :class="{
                  'opacity-100!': rulesForShare(share).length > 0,
                }"
                @click.stop
              >
                <UBadge
                  v-if="rulesForShare(share).length > 0"
                  :label="String(rulesForShare(share).length)"
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
      </template>
      </UCollapsible>
    </div>

    <!-- Other devices' shares -->
    <div
      v-for="([deviceId, deviceShares], groupIdx) in otherDeviceShares"
      :key="deviceId"
      class="rounded-lg overflow-hidden"
      :class="
        groupIdx % 2 === 0
          ? 'bg-muted/5 dark:bg-muted/10'
          : 'bg-muted/10 dark:bg-muted/15'
      "
    >
      <UCollapsible :unmount-on-hide="false">
      <div
        class="flex items-center gap-2 px-3 py-1.5 bg-muted/10 dark:bg-muted/15 cursor-pointer hover:bg-muted/20 dark:hover:bg-muted/25 transition-colors"
      >
        <UIcon
          name="i-lucide-chevron-right"
          class="w-3 h-3 text-muted shrink-0 transition-transform duration-200 [[data-state=open]>&]:rotate-90"
        />
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
      <template #content>
      <UContextMenu
        v-for="(share, idx) in deviceShares"
        :key="share.id"
        :items="shareContextMenuItems(share)"
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
            <UDropdownMenu :items="syncDropdownItems(share)">
              <UiButton
                variant="ghost"
                color="primary"
                icon="i-lucide-refresh-cw"
                class="opacity-0 group-hover:opacity-100 transition-opacity"
                :class="{
                  'opacity-100!': rulesForShare(share).length > 0,
                }"
                @click.stop
              >
                <UBadge
                  v-if="rulesForShare(share).length > 0"
                  :label="String(rulesForShare(share).length)"
                  color="primary"
                  variant="subtle"
                  size="sm"
                />
              </UiButton>
            </UDropdownMenu>
            <UiButton
              v-if="canDeleteShare(share)"
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
      </template>
      </UCollapsible>
    </div>
        </div>
      </template>
    </UCollapsible>
  </div>

  <HaexSystemSettingsPeerStorageCreateSyncRuleDialog
    v-model:open="showSyncDialog"
    :prefill="syncPrefill"
    :edit-rule="syncEditRule"
    @created="onSyncDialogDone"
    @updated="onSyncDialogDone"
    @deleted="onSyncDialogDone"
  />
</template>

<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui'
import type {
  SelectHaexPeerShares,
  SelectHaexSyncRules,
} from '~/database/schemas'
import { useSpaceShares } from '@/composables/useSpaceShares'

const props = defineProps<{
  spaceId: string
  /** Capability strings the current user holds for this space. */
  capabilities: string[]
}>()

const { t } = useI18n()
const peerStore = usePeerStorageStore()
const syncStore = useFileSyncStore()
const windowManager = useWindowManagerStore()
const { removeShareAsync } = useSpaceShares()

// Derived: all shares for this space
const spaceShares = computed<SelectHaexPeerShares[]>(() =>
  peerStore.shares.filter((s) => s.spaceId === props.spaceId),
)

const ownShares = computed<SelectHaexPeerShares[]>(() =>
  spaceShares.value.filter((s) => s.deviceEndpointId === peerStore.nodeId),
)

const otherDeviceShares = computed<[string, SelectHaexPeerShares[]][]>(() => {
  const shares = spaceShares.value.filter(
    (s) => s.deviceEndpointId !== peerStore.nodeId,
  )

  const grouped = new Map<string, SelectHaexPeerShares[]>()
  for (const share of shares) {
    const existing = grouped.get(share.deviceEndpointId) || []
    existing.push(share)
    grouped.set(share.deviceEndpointId, existing)
  }
  return [...grouped.entries()]
})

const getDeviceName = (deviceEndpointId: string): string | undefined => {
  return peerStore.spaceDevices.find(
    (d) => d.deviceEndpointId === deviceEndpointId,
  )?.deviceName
}

// Share icon & path helpers — local to this component; the add-share flow
// uses the same semantics via useSpaceShares.
const isFileShare = (share: SelectHaexPeerShares): boolean => {
  return share.name.includes('.') && !share.name.endsWith('/')
}

const getShareIcon = (share: SelectHaexPeerShares): string => {
  return isFileShare(share) ? 'i-lucide-file' : 'i-lucide-folder'
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

// Capability-based delete gating. Matches the old connection.vue semantics:
// own-device shares are always deletable by the owner; for remote shares the
// user needs space/admin or space/write, or must share the same identity as
// the share's device.
const canDeleteShare = (share: SelectHaexPeerShares): boolean => {
  if (share.deviceEndpointId === peerStore.nodeId) return true

  if (
    props.capabilities.includes('space/admin') ||
    props.capabilities.includes('space/write')
  ) {
    return true
  }

  const shareDevice = peerStore.spaceDevices.find(
    (d) =>
      d.deviceEndpointId === share.deviceEndpointId &&
      d.spaceId === props.spaceId,
  )
  const ownDevice = peerStore.spaceDevices.find(
    (d) =>
      d.deviceEndpointId === peerStore.nodeId && d.spaceId === props.spaceId,
  )
  if (
    shareDevice?.identityId &&
    ownDevice?.identityId &&
    shareDevice.identityId === ownDevice.identityId
  ) {
    return true
  }

  return false
}

// Sync rule matching: a rule applies to a share when source is the same
// local path (own device) or peer endpoint + share-name prefix (remote).
const rulesForShare = (share: SelectHaexPeerShares): SelectHaexSyncRules[] => {
  return syncStore.syncRules.filter((rule) => {
    const cfg = rule.sourceConfig as Record<string, unknown>
    if (share.deviceEndpointId === peerStore.nodeId) {
      return rule.sourceType === 'local' && cfg?.path === share.localPath
    }
    return (
      rule.sourceType === 'peer' &&
      cfg?.endpointId === share.deviceEndpointId &&
      typeof cfg?.path === 'string' &&
      (cfg.path as string).startsWith(share.name)
    )
  })
}

// Sync rule dialog state
const showSyncDialog = ref(false)
const syncPrefill = ref<{
  sourceType: 'local' | 'peer'
  spaceId: string
  deviceEndpointId: string
  shareName: string
  localPath?: string
} | null>(null)
const syncEditRule = ref<SelectHaexSyncRules | null>(null)

const onCreateSyncRule = (share: SelectHaexPeerShares) => {
  const isOwn = share.deviceEndpointId === peerStore.nodeId
  syncEditRule.value = null
  syncPrefill.value = {
    sourceType: isOwn ? 'local' : 'peer',
    spaceId: props.spaceId,
    deviceEndpointId: share.deviceEndpointId,
    shareName: share.name,
    localPath: isOwn ? share.localPath : undefined,
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

const syncDropdownItems = (share: SelectHaexPeerShares) => {
  const rules = rulesForShare(share)
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
      onSelect: () => onCreateSyncRule(share),
    },
  ])

  return items
}

const shareContextMenuItems = (share: SelectHaexPeerShares) => {
  const items: ContextMenuItem[] = [
    {
      label: t('contextMenu.createSync'),
      icon: 'i-lucide-refresh-cw',
      onSelect: () => onCreateSyncRule(share),
    },
  ]
  if (canDeleteShare(share)) {
    items.push({
      label: t('contextMenu.delete'),
      icon: 'i-lucide-trash-2',
      color: 'error' as const,
      onSelect: () => onRemoveShareAsync(share.id),
    })
  }
  return items
}

const onRemoveShareAsync = async (shareId: string) => {
  await removeShareAsync(shareId)
}

const onBrowseShare = (share: SelectHaexPeerShares) => {
  const isOwnDevice = share.deviceEndpointId === peerStore.nodeId
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

onMounted(async () => {
  await peerStore.refreshStatusAsync()
  await peerStore.loadSharesAsync()
  await peerStore.loadSpaceDevicesAsync()
  await syncStore.loadRulesAsync()
})
</script>

<i18n lang="yaml">
de:
  title: Geteilte Dateien
  empty: Noch keine Ordner oder Dateien in diesem Space geteilt
  thisDevice: Dieses Gerät
  syncDropdown:
    createNew: Neue Sync-Regel erstellen
  contextMenu:
    createSync: Sync-Regel erstellen
    delete: Freigabe entfernen
en:
  title: Shared files
  empty: No folders or files shared in this space yet
  thisDevice: This device
  syncDropdown:
    createNew: Create new sync rule
  contextMenu:
    createSync: Create sync rule
    delete: Remove share
</i18n>
