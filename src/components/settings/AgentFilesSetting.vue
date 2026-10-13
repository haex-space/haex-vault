<script setup lang="ts">
/**
 * What each agent may do with files (spec 044 FR-031 to FR-031b): the files of this device and
 * each cloud storage, changed or revoked on selection (spec 023 FR-021). The permissions sync with
 * the vault; a storage nobody decided on yet asks on the agent's first access. Remembered answers
 * for encrypted folders (spec 048 FR-033) list with storage, status and reach; a revoke removes
 * the answer at once, so the agent is asked again. Writes go through the catalog actions, which no
 * agent may call (guardrails).
 */
import { invoke } from '@tauri-apps/api/core'
import type { AgentFilePermissionView } from '@bindings/AgentFilePermissionView'
import {
  agentFiles,
  BUILTIN_AGENT,
  type StorageChoice,
} from '~/lib/files/agentPermissions'

const { t } = useI18n()
const { errString } = useErrorString()
const { overview } = useStorageOverview()
const setPermission = useActionOrThrow('settings.agents.files.set')
const removePermission = useActionOrThrow('settings.agents.files.remove')

const rows = ref<AgentFilePermissionView[] | null>(null)
const busy = ref(false)
const loadError = ref<string | null>(null)
const opError = ref<string | null>(null)

async function reloadAsync() {
  try {
    rows.value = await invoke<AgentFilePermissionView[]>(
      'files_agent_permissions',
    )
    loadError.value = null
  } catch (error: unknown) {
    loadError.value = errString(error)
  }
}

onMounted(reloadAsync)
onVaultTablesChanged(['agent_file_permissions'], () => {
  if (!busy.value) return reloadAsync()
})

const agents = computed(() =>
  rows.value && overview.value
    ? agentFiles(rows.value, overview.value.storages)
    : [],
)

const agentName = (agentId: string) =>
  agentId === BUILTIN_AGENT ? t('settings.agentFiles.builtin') : agentId

const choices = computed(() =>
  (['ask', 'read', 'readWrite', 'denied'] as const).map((value) => ({
    value,
    label: t(`settings.agentFiles.status.${value}`),
    // No row is the only way to ask: a decision can be changed, not taken back to a question.
    disabled: value === 'ask',
  })),
)

async function saveAsync(
  agentId: string,
  kind: 'device' | 'storage',
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

async function revokeAsync(agentId: string, target: string) {
  busy.value = true
  opError.value = null
  try {
    await removePermission({ agentId, kind: 'encryptedFolder', target })
  } catch (error: unknown) {
    opError.value = errString(error)
  } finally {
    busy.value = false
    await reloadAsync()
  }
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
        :description="t('settings.agentFiles.storageDescription')"
        icon="lucide:cloud"
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
