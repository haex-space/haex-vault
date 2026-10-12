<script setup lang="ts">
/**
 * "Agents → Dateien" (spec 044 FR-031b): what the agent of the chat may do with the files of this
 * device and with each storage, changed through `settings.agentFiles.set` (a guardrail, never
 * callable by an agent). Without a stored answer the device is allowed and a storage is asked for
 * on first access; choosing that again removes the stored answer.
 */
import { invoke } from '@tauri-apps/api/core'
import type { AgentFilePermission } from '@bindings/AgentFilePermission'
import type { Sources } from '@bindings/Sources'
import type { StorageSource } from '@bindings/StorageSource'

/** The agent of the holzi chat (`files::permissions::BUILTIN_AGENT`). */
const BUILTIN = 'builtin'

const { t } = useI18n()
const { errString } = useErrorString()
const setPermission = useActionOrThrow('settings.agentFiles.set')

const rows = ref<AgentFilePermission[]>([])
const storages = ref<StorageSource[]>([])
const loading = ref(true)
const loadError = ref<string | null>(null)
const busy = ref(false)
const savedFlash = ref(false)
const opError = ref<string | null>(null)

const deviceOptions = computed(() => [
  { value: 'granted', label: t('settings.agentFiles.granted') },
  { value: 'denied', label: t('settings.agentFiles.denied') },
])
const storageOptions = computed(() => [
  { value: '', label: t('settings.agentFiles.ask') },
  { value: 'read', label: t('settings.agentFiles.read') },
  { value: 'readWrite', label: t('settings.agentFiles.readWrite') },
  { value: 'denied', label: t('settings.agentFiles.denied') },
])

function statusOf(
  kind: 'device' | 'storage',
  target: string,
): string | undefined {
  return rows.value.find(
    (row) =>
      row.agentId === BUILTIN && row.kind === kind && row.target === target,
  )?.status
}

/** The built-in agent reaches the device unless it is revoked. */
const device = computed(() => statusOf('device', '') ?? 'granted')

async function loadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadError.value = null
  try {
    const [stored, sources] = await Promise.all([
      invoke<AgentFilePermission[]>('files_agent_permissions'),
      invoke<Sources>('files_sources'),
    ])
    rows.value = stored
    storages.value = sources.storages
  } catch (e) {
    loadError.value = errString(e)
  } finally {
    loading.value = false
  }
}

async function chooseAsync(
  kind: 'device' | 'storage',
  target: string,
  status: string,
) {
  busy.value = true
  savedFlash.value = false
  opError.value = null
  try {
    await setPermission({
      agentId: BUILTIN,
      kind,
      target,
      ...(status ? { status } : {}),
    })
    savedFlash.value = true
    await loadAsync(true)
  } catch (e) {
    opError.value = errString(e)
  } finally {
    busy.value = false
  }
}

onMounted(() => loadAsync())
onVaultTablesChanged(['agent_file_permissions', 'haex_storages'], () => {
  if (!busy.value) return loadAsync(true)
})
</script>

<template>
  <section class="flex flex-col gap-2">
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>
    <p v-if="loadError" class="text-sm text-destructive" role="alert">
      {{ t('settings.agentFiles.loadFailed') }}: {{ loadError }}
    </p>

    <template v-if="!loading && !loadError">
      <h3 class="px-1 text-xs font-medium text-muted-foreground">
        {{ t('settings.agentFiles.builtin') }}
      </h3>
      <SettingsGroup>
        <SettingsRow
          icon="lucide:hard-drive"
          :title="t('settings.agentFiles.device')"
          :description="t('settings.agentFiles.deviceHint')"
          label-for="settings-agent-files-device"
        >
          <SettingsSelect
            id="settings-agent-files-device"
            :model-value="device"
            :options="deviceOptions"
            :disabled="busy"
            data-testid="settings-agent-files-device"
            @update:model-value="chooseAsync('device', '', $event)"
          />
        </SettingsRow>
        <SettingsRow
          v-for="storage in storages"
          :key="storage.id"
          icon="lucide:cloud"
          :title="storage.name"
          :label-for="`settings-agent-files-${storage.id}`"
        >
          <SettingsSelect
            :id="`settings-agent-files-${storage.id}`"
            :model-value="statusOf('storage', storage.id) ?? ''"
            :options="storageOptions"
            :disabled="busy"
            :data-testid="`settings-agent-files-storage-${storage.name}`"
            @update:model-value="chooseAsync('storage', storage.id, $event)"
          />
        </SettingsRow>
      </SettingsGroup>
      <p
        v-if="storages.length === 0"
        class="px-1 text-xs text-muted-foreground"
      >
        {{ t('settings.agentFiles.noStorages') }}
      </p>
    </template>

    <p v-if="savedFlash" class="px-1 text-xs text-success" role="status">
      {{ t('settings.agentFiles.saved') }}
    </p>
    <p v-if="opError" class="px-1 text-xs text-destructive" role="alert">
      {{ t('settings.agentFiles.failed') }}: {{ opError }}
    </p>
  </section>
</template>
