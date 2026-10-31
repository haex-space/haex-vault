<script setup lang="ts">
/**
 * What each agent may do with files (spec 044 FR-031 to FR-031b): the files of this device and
 * each cloud storage, changed or revoked on selection (spec 023 FR-021). The permissions sync with
 * the vault; a storage nobody decided on yet asks on the agent's first access. Remembered answers
 * for encrypted folders (spec 048 FR-033) list with storage, status and reach; a revoke removes
 * the answer at once, so the agent is asked again. The contacts (spec 051 FR-030) follow: all of
 * them by selection, where "Fragt nach" revokes the answer, and remembered answers for single groups
 * with a revoke. Other own devices and the shares contacts gave this vault list as storages (spec
 * 056 FR-040), and "Dateifreigaben verwalten" lets an agent manage file shares (FR-045). Writes go
 * through the catalog actions, which no agent may call (guardrails).
 *
 * With `agentId` it shows only that agent, for the page of an external agent (spec 021 US5), which
 * starts without any permission, not even the device. File shares and sync rules stay with the
 * built-in agent; their actions serve only it.
 */
import { invoke } from '@tauri-apps/api/core'
import type { AgentFilePermissionView } from '@bindings/AgentFilePermissionView'
import type { AgentView } from '@bindings/AgentView'
import type { ContactGroup } from '@bindings/ContactGroup'
import type { Sources } from '@bindings/Sources'
import { isPeerStorage, isRemoteStorage } from '~/lib/files/registry'
import {
  agentFiles,
  BUILTIN_AGENT,
  type StorageChoice,
} from '~/lib/files/agentPermissions'

const props = defineProps<{ agentId?: string }>()

const { t } = useI18n()
const { errString } = useErrorString()
const { overview } = useStorageOverview()
const setPermission = useActionOrThrow('settings.agents.files.set')
const removePermission = useActionOrThrow('settings.agents.files.remove')

const rows = ref<AgentFilePermissionView[] | null>(null)
const groups = ref<ContactGroup[]>([])
/** Other own devices and received shares, as storages (spec 056). */
const remote = ref<{ id: string; name: string }[]>([])
const busy = ref(false)
const loadError = ref<string | null>(null)
const opError = ref<string | null>(null)

async function reloadAsync() {
  try {
    rows.value = await invoke<AgentFilePermissionView[]>(
      'files_agent_permissions',
    )
    groups.value = await invoke<ContactGroup[]>('contacts_groups')
    // Without the other devices the settings still show the rest.
    const sources = await invoke<Sources>('files_sources').catch(() => null)
    remote.value = sources ? remoteStorages(sources) : []
    loadError.value = null
  } catch (error: unknown) {
    loadError.value = errString(error)
  }
}

function remoteStorages(sources: Sources) {
  return [
    ...sources.devices.map((device) => ({
      id: device.id,
      name: device.name ?? t('files.peer.unnamed'),
    })),
    ...sources.contacts.flatMap((contact) =>
      contact.shares.map((share) => ({
        id: share.storageId,
        name: t('settings.agentFiles.receivedShare', {
          name: share.name,
          contact: contact.contactName,
        }),
      })),
    ),
  ]
}

onMounted(reloadAsync)
onVaultTablesChanged(['agent_file_permissions'], () => {
  if (!busy.value) return reloadAsync()
})

const agents = computed(() =>
  rows.value && overview.value
    ? agentFiles(
        rows.value,
        [...overview.value.storages, ...remote.value],
        props.agentId,
      )
    : [],
)

function storageIcon(id: string): string {
  if (isPeerStorage(id)) return 'lucide:monitor-smartphone'
  return isRemoteStorage(id) ? 'lucide:folder-symlink' : 'lucide:cloud'
}

function storageDescription(id: string): string {
  if (isPeerStorage(id)) return t('settings.agentFiles.peerDescription')
  return isRemoteStorage(id)
    ? t('settings.agentFiles.receivedDescription')
    : t('settings.agentFiles.storageDescription')
}

/** The names external agents report, by id (spec 021). */
const externalNames = ref<Record<string, string>>({})

async function loadNamesAsync() {
  try {
    const agents = await invoke<AgentView[]>('mcp_agents_list')
    externalNames.value = Object.fromEntries(
      agents.map((agent) => [agent.id, agent.clientName]),
    )
  } catch {
    // Without the names the ids stand in.
  }
}
onMounted(loadNamesAsync)

const agentName = (agentId: string) => {
  if (agentId === BUILTIN_AGENT) return t('settings.agentFiles.builtin')
  if (props.agentId) return t('settings.agentFiles.thisAgent')
  return externalNames.value[agentId] ?? agentId
}

const choices = computed(() =>
  (['ask', 'read', 'readWrite', 'denied'] as const).map((value) => ({
    value,
    label: t(`settings.agentFiles.status.${value}`),
    // No row is the only way to ask: a decision can be changed, not taken back to a question.
    disabled: value === 'ask',
  })),
)

const contactChoices = computed(() =>
  (['ask', 'read', 'readWrite', 'denied'] as const).map((value) => ({
    value,
    label: t(`settings.agentFiles.status.${value}`),
  })),
)

const groupName = (groupId: string) => {
  const name = groups.value.find((group) => group.id === groupId)?.name
  return name
    ? t('settings.agentFiles.contactsGroup', { name })
    : t('settings.agentFiles.contactsGroupUnknown')
}

async function saveAsync(
  agentId: string,
  kind: 'device' | 'storage' | 'contacts' | 'fileShares' | 'syncRules',
  target: string,
  status: string,
) {
  busy.value = true
  opError.value = null
  try {
    await setPermission({ agentId, kind, target, status })
  } catch (error: unknown) {
    opError.value = errString(error)
  } finally {
    busy.value = false
    await reloadAsync()
  }
}

async function revokeAsync(
  agentId: string,
  target: string,
  kind: 'encryptedFolder' | 'contacts' = 'encryptedFolder',
) {
  busy.value = true
  opError.value = null
  try {
    await removePermission({ agentId, kind, target })
  } catch (error: unknown) {
    opError.value = errString(error)
  } finally {
    busy.value = false
    await reloadAsync()
  }
}

function onContacts(agentId: string, next: string) {
  if ((next as StorageChoice) === 'ask')
    return revokeAsync(agentId, '*', 'contacts')
  return saveAsync(agentId, 'contacts', '*', next)
}

function onStorage(agentId: string, storageId: string, next: string) {
  if ((next as StorageChoice) === 'ask') return
  return saveAsync(agentId, 'storage', storageId, next)
}
</script>

<template>
  <section class="flex flex-col gap-6">
    <p v-if="loadError" class="text-sm text-destructive" role="alert">
      {{ t('settings.agentFiles.loadFailed') }}: {{ loadError }}
    </p>

    <SettingsGroup
      v-for="agent in agents"
      :key="agent.agentId"
      :label="agentName(agent.agentId)"
      :data-testid="`agent-files-${agent.agentId}`"
    >
      <SettingsRow
        :title="t('settings.agentFiles.device')"
        :description="t('settings.agentFiles.deviceDescription')"
        icon="lucide:hard-drive"
        :label-for="`agent-files-device-${agent.agentId}`"
      >
        <ShadcnSwitch
          :id="`agent-files-device-${agent.agentId}`"
          :model-value="agent.device"
          :disabled="busy"
          :data-testid="`agent-files-device-${agent.agentId}`"
          @update:model-value="
            saveAsync(
              agent.agentId,
              'device',
              '',
              $event === true ? 'granted' : 'denied',
            )
          "
        />
      </SettingsRow>
      <SettingsRow
        v-for="storage in agent.storages"
        :key="storage.id"
        :title="storage.name"
        :description="storageDescription(storage.id)"
        :icon="storageIcon(storage.id)"
        :label-for="`agent-files-storage-${agent.agentId}-${storage.id}`"
      >
        <SettingsSelect
          :id="`agent-files-storage-${agent.agentId}-${storage.id}`"
          :model-value="storage.status"
          :options="choices"
          :disabled="busy"
          :data-testid="`agent-files-storage-${agent.agentId}-${storage.id}`"
          @update:model-value="onStorage(agent.agentId, storage.id, $event)"
        />
      </SettingsRow>
      <SettingsRow
        :title="t('settings.agentFiles.contacts')"
        :description="t('settings.agentFiles.contactsDescription')"
        icon="lucide:contact"
        :label-for="`agent-files-contacts-${agent.agentId}`"
      >
        <SettingsSelect
          :id="`agent-files-contacts-${agent.agentId}`"
          :model-value="agent.contacts"
          :options="contactChoices"
          :disabled="busy"
          :data-testid="`agent-files-contacts-${agent.agentId}`"
          @update:model-value="onContacts(agent.agentId, $event)"
        />
      </SettingsRow>
      <SettingsRow
        v-if="agent.agentId === BUILTIN_AGENT"
        :title="t('settings.agentFiles.fileShares')"
        :description="t('settings.agentFiles.fileSharesDescription')"
        icon="lucide:share-2"
        :label-for="`agent-files-shares-${agent.agentId}`"
      >
        <ShadcnSwitch
          :id="`agent-files-shares-${agent.agentId}`"
          :model-value="agent.fileShares"
          :disabled="busy"
          :data-testid="`agent-files-shares-${agent.agentId}`"
          @update:model-value="
            saveAsync(
              agent.agentId,
              'fileShares',
              '',
              $event === true ? 'granted' : 'denied',
            )
          "
        />
      </SettingsRow>
      <SettingsRow
        v-if="agent.agentId === BUILTIN_AGENT"
        :title="t('settings.agentFiles.syncRules')"
        :description="t('settings.agentFiles.syncRulesDescription')"
        icon="lucide:refresh-cw"
        :label-for="`agent-files-sync-rules-${agent.agentId}`"
      >
        <ShadcnSwitch
          :id="`agent-files-sync-rules-${agent.agentId}`"
          :model-value="agent.syncRules"
          :disabled="busy"
          :data-testid="`agent-files-sync-rules-${agent.agentId}`"
          @update:model-value="
            saveAsync(
              agent.agentId,
              'syncRules',
              '',
              $event === true ? 'granted' : 'denied',
            )
          "
        />
      </SettingsRow>
      <SettingsRow
        v-for="grant in agent.contactGroups"
        :key="grant.target"
        :title="groupName(grant.groupId)"
        :description="t(`settings.agentFiles.status.${grant.status}`)"
        icon="lucide:users"
      >
        <UiButton
          type="button"
          variant="outline"
          size="sm"
          :disabled="busy"
          :data-testid="`agent-files-contacts-revoke-${agent.agentId}-${grant.groupId}`"
          @click="revokeAsync(agent.agentId, grant.target, 'contacts')"
        >
          {{ t('settings.agentFiles.revoke') }}
        </UiButton>
      </SettingsRow>
      <SettingsRow
        v-for="folder in agent.folders"
        :key="folder.target"
        :title="folder.name ?? t('settings.agentFiles.folderUnknown')"
        :description="
          t(
            folder.onDevice
              ? 'settings.agentFiles.folderDescriptionDevice'
              : 'settings.agentFiles.folderDescription',
            {
              storage: folder.storageName,
              status: t(`settings.agentFiles.status.${folder.status}`),
              reach: t(`settings.agentFiles.reach.${folder.reach}`),
            },
          )
        "
        icon="lucide:folder-lock"
      >
        <UiButton
          type="button"
          variant="outline"
          size="sm"
          :disabled="busy"
          :data-testid="`agent-files-folder-revoke-${agent.agentId}-${folder.target}`"
          @click="revokeAsync(agent.agentId, folder.target)"
        >
          {{ t('settings.agentFiles.revoke') }}
        </UiButton>
      </SettingsRow>
    </SettingsGroup>

    <p v-if="opError" class="px-1 text-xs text-destructive" role="alert">
      {{ t('settings.agentFiles.failed') }}: {{ opError }}
    </p>
  </section>
</template>
