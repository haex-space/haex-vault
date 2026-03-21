<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
  >
    <!-- Endpoint + Shared Folders (primary card) -->
    <UCard>
      <template #header>
        <div class="space-y-3">
          <div>
            <h3 class="text-lg font-semibold">{{ t('endpoint.title') }}</h3>
            <p class="text-sm text-muted mt-1">
              {{ t('endpoint.description') }}
            </p>
          </div>
          <UiButton
            :icon="store.running ? 'i-lucide-power-off' : 'i-lucide-power'"
            :color="store.running ? 'error' : 'primary'"
            :loading="isToggling"
            block
            @click="onToggleEndpointAsync"
          >
            {{ store.running ? t('endpoint.stop') : t('endpoint.start') }}
          </UiButton>
        </div>
      </template>

      <!-- Shared folders per space (always visible, independent of endpoint status) -->
        <!-- No Spaces -->
        <div
          v-if="!spacesStore.spaces.length"
          class="text-center py-6 text-muted"
        >
          <UIcon
            name="i-lucide-cloud-off"
            class="w-10 h-10 mx-auto mb-2 opacity-50"
          />
          <p class="text-sm">{{ t('shares.noSpaces') }}</p>
          <UiButton
            class="mt-3"
            variant="outline"
            icon="i-heroicons-user-group"
            @click="onNavigateToSpaces"
          >
            {{ t('shares.goToSpaces') }}
          </UiButton>
        </div>

        <!-- Spaces as accordions -->
        <div
          v-else
          class="space-y-2"
        >
          <div
            v-for="space in spacesStore.spaces"
            :key="space.id"
            class="border border-default rounded-lg overflow-hidden"
          >
            <UCollapsible
              :open="expandedSpaces.has(space.id)"
              :unmount-on-hide="false"
            >
              <!-- Space header (clickable toggle) -->
              <div class="flex items-center gap-2 px-4 py-2.5 bg-muted/30">
                <button
                  class="flex items-center gap-2 min-w-0 flex-1 cursor-pointer"
                  @click="toggleSpace(space.id)"
                >
                  <UIcon
                    name="i-lucide-chevron-right"
                    class="w-4 h-4 shrink-0 text-muted transition-transform duration-200"
                    :class="{ 'rotate-90': expandedSpaces.has(space.id) }"
                  />
                  <span class="font-medium truncate">{{ space.name }}</span>
                  <UBadge variant="subtle" size="sm">
                    {{ getSharesForSpace(space.id).length }}
                  </UBadge>
                </button>
                <UDropdownMenu
                  :items="[
                    [
                      { label: t('shares.addFolder'), icon: 'i-lucide-folder-plus', onSelect: () => onAddShareAsync(space.id, 'folder') },
                      { label: t('shares.addFile'), icon: 'i-lucide-file-plus', onSelect: () => onAddShareAsync(space.id, 'file') },
                    ],
                  ]"
                >
                  <UiButton
                    icon="i-lucide-plus"
                    variant="ghost"
                    size="xl"
                    :title="t('shares.add')"
                  />
                </UDropdownMenu>
              </div>

              <!-- Space content (collapsible) -->
              <template #content>
                <div class="divide-y divide-default">
                  <!-- This device's shares -->
                  <div
                    v-for="share in getSharesForDevice(space.id, store.nodeId)"
                    :key="share.id"
                    class="flex items-center gap-3 px-4 py-2.5 group"
                  >
                    <UIcon :name="getShareIcon(share)" class="w-4 h-4 text-primary shrink-0" />
                    <div
                      class="min-w-0 flex-1 cursor-pointer hover:text-primary transition-colors"
                      @click="onBrowseShare(share)"
                    >
                      <p class="text-sm font-medium">{{ share.name }}</p>
                      <p class="text-xs text-muted truncate">{{ formatPath(share.localPath) }}</p>
                    </div>
                    <UiButton
                      color="error"
                      variant="ghost"
                      icon="i-lucide-trash-2"
                      class="opacity-0 group-hover:opacity-100 transition-opacity"
                      @click="onRemoveShareAsync(share.id)"
                    />
                  </div>

                  <!-- Other devices' shares -->
                  <div
                    v-for="[deviceId, deviceShares] in getOtherDeviceShares(space.id)"
                    :key="deviceId"
                  >
                    <div class="px-4 py-2 bg-muted/10">
                      <span class="text-xs font-medium text-muted">
                        {{ getDeviceName(deviceId) || deviceId.slice(0, 12) + '…' }}
                      </span>
                    </div>
                    <div
                      v-for="share in deviceShares"
                      :key="share.id"
                      class="flex items-center gap-3 px-4 py-2.5 group cursor-pointer hover:bg-muted/20 transition-colors"
                      @click="onBrowseShare(share)"
                    >
                      <UIcon :name="getShareIcon(share)" class="w-4 h-4 text-muted shrink-0" />
                      <p class="text-sm flex-1 truncate">{{ share.name }}</p>
                      <UiButton
                        v-if="canDeleteShare(space.id, share)"
                        color="error"
                        variant="ghost"
                        icon="i-lucide-trash-2"
                        class="opacity-0 group-hover:opacity-100 transition-opacity"
                        @click.stop="onRemoveShareAsync(share.id)"
                      />
                      <UIcon v-else name="i-lucide-chevron-right" class="w-4 h-4 text-muted shrink-0" />
                    </div>
                  </div>

                  <!-- Empty space -->
                  <div
                    v-if="getSharesForSpace(space.id).length === 0"
                    class="px-4 py-4 text-center text-muted text-sm"
                  >
                    {{ t('shares.emptySpace') }}
                  </div>
                </div>
              </template>
            </UCollapsible>
          </div>
        </div>

      <template #footer>
        <UCheckbox
          v-model="autostart"
          :label="t('endpoint.autostart')"
          @update:model-value="onToggleAutostartAsync"
        />
      </template>
    </UCard>

    <!-- Relay Configuration -->
    <UCard>
      <template #header>
        <h3 class="text-lg font-semibold">{{ t('relay.title') }}</h3>
        <p class="text-sm text-muted mt-1">{{ t('relay.description') }}</p>
      </template>

      <div class="space-y-3">
        <UFormField :label="t('relay.urlLabel')" :hint="t('relay.urlHint')">
          <div class="flex gap-2">
            <UInput
              v-model="relayUrlInput"
              :placeholder="t('relay.urlPlaceholder')"
              class="flex-1 font-mono text-sm"
            />
            <UiButton
              icon="i-lucide-save"
              color="primary"
              variant="outline"
              @click="onSaveRelayUrlAsync"
            />
          </div>
        </UFormField>
        <p v-if="store.configuredRelayUrl" class="text-xs text-muted">
          {{ t('relay.active') }}: <code class="bg-muted/50 px-1 rounded">{{ store.configuredRelayUrl }}</code>
        </p>
        <p v-else class="text-xs text-muted">{{ t('relay.usingDefault') }}</p>
      </div>
    </UCard>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { SettingsCategory } from '~/config/settingsCategories'
import { eq } from 'drizzle-orm'
import { invoke } from '@tauri-apps/api/core'
import type { SelectHaexPeerShares } from '~/database/schemas'
import { haexVaultSettings } from '~/database/schemas'
import {
  VaultSettingsKeyEnum,
  VaultSettingsTypeEnum,
} from '~/config/vault-settings'

const { t } = useI18n()
const { add } = useToast()
const store = usePeerStorageStore()
const spacesStore = useSpacesStore()
const windowManager = useWindowManagerStore()
const { currentVault } = storeToRefs(useVaultStore())

const isToggling = ref(false)
const autostart = ref(false)
const relayUrlInput = ref('')
const expandedSpaces = ref(new Set<string>())

const toggleSpace = (spaceId: string) => {
  const next = new Set(expandedSpaces.value)
  if (next.has(spaceId)) next.delete(spaceId)
  else next.add(spaceId)
  expandedSpaces.value = next
}

const onToggleAutostartAsync = async (value: boolean | 'indeterminate') => {
  if (value === 'indeterminate') return
  if (!currentVault.value?.drizzle) return

  try {
    const existing =
      await currentVault.value.drizzle.query.haexVaultSettings.findFirst({
        where: eq(
          haexVaultSettings.key,
          VaultSettingsKeyEnum.peerStorageAutostart,
        ),
      })

    if (existing) {
      await currentVault.value.drizzle
        .update(haexVaultSettings)
        .set({ value: value ? 'true' : 'false' })
        .where(
          eq(haexVaultSettings.key, VaultSettingsKeyEnum.peerStorageAutostart),
        )
    } else {
      await currentVault.value.drizzle.insert(haexVaultSettings).values({
        id: crypto.randomUUID(),
        key: VaultSettingsKeyEnum.peerStorageAutostart,
        type: VaultSettingsTypeEnum.settings,
        value: value ? 'true' : 'false',
      })
    }
  } catch (error) {
    console.error('Failed to save autostart setting:', error)
    add({ description: t('toast.error'), color: 'error' })
  }
}

const onSaveRelayUrlAsync = async () => {
  try {
    await store.saveConfiguredRelayUrlAsync(relayUrlInput.value.trim() || null)
    add({ title: t('relay.saved'), color: 'success' })
  } catch {
    add({ title: t('toast.error'), color: 'error' })
  }
}

onMounted(async () => {
  await store.refreshStatusAsync()
  await store.loadSharesAsync()
  await store.loadSpaceDevicesAsync()
  await store.loadConfiguredRelayUrlAsync()
  relayUrlInput.value = store.configuredRelayUrl ?? ''

  if (currentVault.value?.drizzle) {
    const row =
      await currentVault.value.drizzle.query.haexVaultSettings.findFirst({
        where: eq(
          haexVaultSettings.key,
          VaultSettingsKeyEnum.peerStorageAutostart,
        ),
      })
    autostart.value = row?.value === 'true'
  }
})

// =========================================================================
// Computed helpers for grouping shares
// =========================================================================

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

/** Check if the current user can delete a share in this space.
 *  Allowed when:
 *  - The share belongs to the same identity (own device, possibly different device)
 *  - OR the user is admin/owner of the space
 */
const canDeleteShare = (spaceId: string, share: SelectHaexPeerShares): boolean => {
  // Own device — always allowed
  if (share.deviceEndpointId === store.nodeId) return true

  // Check space role — admin/owner can delete any share
  const space = spacesStore.spaces.find((s) => s.id === spaceId)
  if (space && (space.role === 'admin' || space.role === 'owner')) return true

  // Check if share belongs to same identity (same user, different device)
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

// =========================================================================
// Actions
// =========================================================================

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
      title: t('toast.error'),
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
  const name = share.name
  return name.includes('.') && !name.endsWith('/')
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
    ? t('shares.thisDevice')
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
      title: t('toast.error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}

const onRemoveShareAsync = async (shareId: string) => {
  try {
    await store.removeShareAsync(shareId)
    add({ title: t('toast.shareRemoved'), color: 'neutral' })
  } catch (error) {
    add({
      title: t('toast.error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  title: P2P Storage
  description: Teile lokale Ordner direkt mit anderen Peers über eine verschlüsselte P2P-Verbindung
  endpoint:
    title: Verbindung
    description: Starte den P2P-Endpoint, um Dateien zu teilen und zu empfangen
    start: Starten
    stop: Stoppen
    stopped: Endpoint ist nicht aktiv. Starte ihn, um Dateien zu teilen.
    autostart: Automatisch starten wenn die Vault geöffnet wird
  shares:
    add: Hinzufügen
    addFolder: Ordner freigeben
    addFile: Datei freigeben
    noSpaces: Keine Spaces vorhanden
    emptySpace: Noch keine Ordner oder Dateien geteilt
    thisDevice: Dieses Gerät
    goToSpaces: Spaces verwalten
  relay:
    title: Relay-Server
    description: Relay-Server für P2P-Verbindungen durch NAT konfigurieren
    urlLabel: Relay-URL
    urlHint: "Leer lassen um den Standard-Relay zu verwenden: relay.sync.haex.space"
    urlPlaceholder: "https://relay.sync.haex.space"
    active: "Aktiver Relay"
    usingDefault: "Standard: relay.sync.haex.space"
    saved: Relay-URL gespeichert
  toast:
    started: P2P-Endpoint gestartet
    stopped: P2P-Endpoint gestoppt
    shareAdded: Ordner hinzugefügt
    shareRemoved: Ordner entfernt
    error: Fehler
en:
  title: P2P Storage
  description: Share local folders directly with other peers over an encrypted P2P connection
  endpoint:
    title: Connection
    description: Start the P2P endpoint to share and receive files
    start: Start
    stop: Stop
    stopped: Endpoint is not active. Start it to share files.
    autostart: Automatically start when the vault is opened
  shares:
    add: Add
    addFolder: Share folder
    addFile: Share file
    noSpaces: No spaces available
    emptySpace: No folders or files shared yet
    thisDevice: This device
    goToSpaces: Manage Spaces
  relay:
    title: Relay Server
    description: Configure the relay server for P2P connections through NAT
    urlLabel: Relay URL
    urlHint: "Leave empty to use the default: relay.sync.haex.space"
    urlPlaceholder: "https://relay.sync.haex.space"
    active: "Active relay"
    usingDefault: "Default: relay.sync.haex.space"
    saved: Relay URL saved
  toast:
    started: P2P endpoint started
    stopped: P2P endpoint stopped
    shareAdded: Folder added
    shareRemoved: Folder removed
    error: Error
</i18n>
