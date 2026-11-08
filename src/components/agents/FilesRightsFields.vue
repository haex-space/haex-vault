<script setup lang="ts">
/**
 * The rights group "Dateien" of an agent profile (spec 083 FR-033; specs 044, 048, 056, 061): the
 * files of this device (always this device's), each cloud storage, remembered answers for encrypted
 * folders, and managing file shares and sync rules. A storage or an encrypted folder without an
 * entry asks at the first access; "Immer erlauben" is written here. Each choice emits the whole
 * group; the editor saves on selection.
 */
import { invoke } from '@tauri-apps/api/core'
import type { Sources } from '@bindings/Sources'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { isPeerStorage, isRemoteStorage } from '~/lib/files/registry'
import type { AccessLevel, FilesRights, FolderRight } from '~/lib/agents/rights'

const props = defineProps<{
  modelValue: FilesRights
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [FilesRights] }>()

const { t } = useI18n()
const { overview } = useStorageOverview()
const remoteFailed = ref(false)

/** Other own devices and received shares, as storages (spec 056). */
const remote = ref<{ id: string; name: string }[]>([])
async function loadRemoteAsync() {
  try {
    const sources = await invoke<Sources>('files_sources')
    remote.value = [
      ...sources.devices.map((device) => ({
        id: device.id,
        name: device.name ?? t('files.peer.unnamed'),
      })),
      ...sources.contacts.flatMap((contact) =>
        contact.shares.map((share) => ({
          id: share.storageId,
          name: t('mcpAgents.files.receivedShare', {
            name: share.name,
            contact: contact.contactName,
          }),
        })),
      ),
    ]
    remoteFailed.value = false
  } catch {
    remoteFailed.value = true
  }
}
onMounted(loadRemoteAsync)
onVaultTablesChanged(
  [
    'device_presence_no_sync',
    'device_lists',
    'haex_file_shares_received',
    'haex_contacts*',
  ],
  loadRemoteAsync,
)

const storages = computed(() => {
  const known = [...(overview.value?.storages ?? []), ...remote.value]
  const knownIds = new Set(known.map((storage) => storage.id))
  const grantedIds = new Set([
    ...Object.keys(props.modelValue.storages ?? {}),
    ...Object.keys(props.modelValue.folders ?? {}).map((target) =>
      target.startsWith('device:')
        ? ''
        : target.slice(0, target.lastIndexOf('/')),
    ),
  ])
  return [
    ...known,
    ...[...grantedIds]
      .filter((id) => id && !knownIds.has(id))
      .map((id) => ({ id, name: t('mcpAgents.files.storageUnknown') })),
  ]
})

function storageIcon(id: string): string {
  if (isPeerStorage(id)) return 'lucide:monitor-smartphone'
  return isRemoteStorage(id) ? 'lucide:folder-symlink' : 'lucide:cloud'
}

function storageDescription(id: string): string {
  if (isPeerStorage(id)) return t('mcpAgents.files.peerDescription')
  return isRemoteStorage(id)
    ? t('mcpAgents.files.receivedDescription')
    : t('mcpAgents.files.storageDescription')
}

const NONE = 'none'
const levels: AccessLevel[] = ['read', 'readWrite', 'denied']
const options = computed(() => [
  { value: NONE, label: t('mcpAgents.files.ask') },
  ...levels.map((level) => ({
    value: level,
    label: t(`mcpAgents.files.level.${level}`),
  })),
])

function set(next: Partial<FilesRights>) {
  emit('update:modelValue', { ...props.modelValue, ...next })
}

function chooseStorage(id: string, choice: string) {
  const { [id]: _removed, ...others } = props.modelValue.storages ?? {}
  set({
    storages:
      choice === NONE ? others : { ...others, [id]: choice as AccessLevel },
  })
}

/** The remembered answers for encrypted folders, with the names this device has read. */
const folderNames = ref<Record<string, string>>({})
const folderTargets = computed(() =>
  Object.keys(props.modelValue.folders ?? {}),
)
watch(
  folderTargets,
  async (targets) => {
    if (targets.length === 0) return
    try {
      folderNames.value = await invoke<Record<string, string>>(
        'files_folder_names',
        { targets },
      )
    } catch {
      // Without the names the folders show as encrypted folders.
    }
  },
  { immediate: true },
)

const folders = computed(() =>
  Object.entries(props.modelValue.folders ?? {})
    .map(([target, right]: [string, FolderRight]) => {
      const onDevice = target.startsWith('device:')
      const storage = storages.value.find((candidate) =>
        target.startsWith(`${candidate.id}/`),
      )
      return {
        target,
        right,
        onDevice,
        storageName: storage?.name ?? '',
        known: onDevice || !!storage,
        name: folderNames.value[target] ?? null,
      }
    })
    // A folder of a storage that is gone is not listed.
    .filter((folder) => folder.known)
    .sort(
      (a, b) =>
        a.storageName.localeCompare(b.storageName) ||
        (a.name ?? '').localeCompare(b.name ?? ''),
    ),
)

function revokeFolder(target: string) {
  const { [target]: _removed, ...others } = props.modelValue.folders ?? {}
  set({ folders: others })
}
</script>

<template>
  <SettingsGroup
    :label="t('mcpAgents.files.title')"
    data-testid="mcp-files-rights"
  >
    <SettingsRow
      :title="t('mcpAgents.files.device')"
      :description="t('mcpAgents.files.deviceDescription')"
      icon="lucide:hard-drive"
      label-for="mcp-files-device"
    >
      <ShadcnSwitch
        id="mcp-files-device"
        :model-value="!!modelValue.device"
        :disabled="disabled"
        data-testid="mcp-files-device"
        @update:model-value="set({ device: $event === true })"
      />
    </SettingsRow>
    <div v-if="remoteFailed" class="px-3 py-2" role="status">
      <p class="mb-2 text-sm text-muted-foreground">
        {{ t('mcpAgents.files.sourcesUnavailable') }}
      </p>
      <UiButton
        type="button"
        variant="outline"
        size="sm"
        :disabled="disabled"
        @click="loadRemoteAsync"
      >
        {{ t('mcpAgents.files.retrySources') }}
      </UiButton>
    </div>
    <SettingsRow
      v-for="storage in storages"
      :key="storage.id"
      :title="storage.name"
      :description="storageDescription(storage.id)"
      :icon="storageIcon(storage.id)"
      :label-for="`mcp-files-storage-${storage.id}`"
    >
      <SettingsSelect
        :id="`mcp-files-storage-${storage.id}`"
        :model-value="modelValue.storages?.[storage.id] ?? NONE"
        :options="options"
        :disabled="disabled"
        :data-testid="`mcp-files-storage-${storage.id}`"
        @update:model-value="chooseStorage(storage.id, $event)"
      />
    </SettingsRow>
    <SettingsRow
      v-for="folder in folders"
      :key="folder.target"
      :title="folder.name ?? t('mcpAgents.files.folderUnknown')"
      :description="
        t(
          folder.onDevice
            ? 'mcpAgents.files.folderDescriptionDevice'
            : 'mcpAgents.files.folderDescription',
          {
            storage: folder.storageName,
            status: t(`mcpAgents.files.level.${folder.right.level}`),
            reach: t(
              folder.right.cloud
                ? 'mcpAgents.files.reach.cloud'
                : 'mcpAgents.files.reach.local',
            ),
          },
        )
      "
      icon="lucide:folder-lock"
    >
      <UiButton
        type="button"
        variant="outline"
        size="sm"
        :disabled="disabled"
        :data-testid="`mcp-files-folder-revoke-${folder.target}`"
        @click="revokeFolder(folder.target)"
      >
        {{ t('mcpAgents.files.revoke') }}
      </UiButton>
    </SettingsRow>
    <SettingsRow
      :title="t('mcpAgents.files.fileShares')"
      :description="t('mcpAgents.files.fileSharesDescription')"
      icon="lucide:share-2"
      label-for="mcp-files-shares"
    >
      <ShadcnSwitch
        id="mcp-files-shares"
        :model-value="!!modelValue.fileShares"
        :disabled="disabled"
        data-testid="mcp-files-shares"
        @update:model-value="set({ fileShares: $event === true })"
      />
    </SettingsRow>
    <SettingsRow
      :title="t('mcpAgents.files.syncRules')"
      :description="t('mcpAgents.files.syncRulesDescription')"
      icon="lucide:refresh-cw"
      label-for="mcp-files-sync-rules"
    >
      <ShadcnSwitch
        id="mcp-files-sync-rules"
        :model-value="!!modelValue.syncRules"
        :disabled="disabled"
        data-testid="mcp-files-sync-rules"
        @update:model-value="set({ syncRules: $event === true })"
      />
    </SettingsRow>
  </SettingsGroup>
</template>
