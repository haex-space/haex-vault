<script setup lang="ts">
/**
 * Default chat model (spec 002 US4) of this device: models are installed per device, so it is one
 * of the few settings that do not apply to the whole vault (spec 023 FR-024). One select saved on
 * selection (FR-021); "Keins" clears the stored value.
 */
import type { InstalledModel } from '~/composables/useModels'
import type { Provider, ProviderModel } from '~/composables/useProviders'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync } = usePreferences()
const setDefault = useActionOrThrow('settings.models.setDefault')
const clearDefault = useActionOrThrow('settings.models.clearDefault')
const { listInstalledAsync } = useModels()
const { listAsync: listProvidersAsync, listModelsAsync } = useProviders()
const device = useSettingsDevice()

const PREF_KEY = 'chat.default_model_id'

const installedModels = ref<InstalledModel[]>([])
const providerList = ref<Provider[]>([])
const providerModels = ref<Record<string, ProviderModel[]>>({})
const stored = ref<string | null>(null)

const loading = ref(true)
const busy = ref(false)
const savedFlash = ref(false)
const opError = ref<string | null>(null)
const loadError = ref<string | null>(null)
const modelListError = ref(false)

type ModelGroup = {
  providerId: string
  providerName: string
  models: { id: string; name: string }[]
}

const modelGroups = computed<ModelGroup[]>(() => {
  const groups: ModelGroup[] = []
  if (installedModels.value.length > 0) {
    groups.push({
      providerId: 'local',
      providerName: t('settings.default.modelGroup.local'),
      models: installedModels.value.map((m) => ({ id: m.id, name: m.name })),
    })
  }
  for (const p of providerList.value) {
    if (p.kind !== 'api_key') continue
    const list = providerModels.value[p.id] ?? []
    if (list.length === 0) continue
    groups.push({
      providerId: p.id,
      providerName: p.name,
      models: list.map((m) => ({ id: m.id, name: m.name })),
    })
  }
  return groups
})

const hasAnyModel = computed(() =>
  modelGroups.value.some((g) => g.models.length > 0),
)

/** A stored id that is no longer offered (a deleted model) stays visible and selected. */
const extraOptions = computed<SettingsSelectOption[]>(() => {
  const id = stored.value
  const offered =
    id === null ||
    modelGroups.value.some((g) => g.models.some((m) => m.id === id))
  return [
    { value: '', label: t('settings.default.none') },
    ...(offered
      ? []
      : [{ value: id, label: t('settings.default.unknownModel', { id }) }]),
  ]
})

const selectGroups = computed(() =>
  modelGroups.value.map((group) => ({
    key: group.providerId,
    label: group.providerName,
    options: group.models.map((m) => ({ value: m.id, label: m.name })),
  })),
)

/** A `quiet` reload (after a change from elsewhere) keeps the shown lists until the new ones are read. */
async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadError.value = null
  modelListError.value = false
  try {
    const [installed, providers] = await Promise.all([
      listInstalledAsync(),
      listProvidersAsync(),
    ])
    installedModels.value = installed
    providerList.value = providers
    const nextModels: Record<string, ProviderModel[]> = {}
    await Promise.all(
      providers
        .filter((p) => p.kind === 'api_key')
        .map(async (p) => {
          try {
            nextModels[p.id] = await listModelsAsync(p.id)
          } catch {
            modelListError.value = true
          }
        }),
    )
    providerModels.value = nextModels

    stored.value = await getPrefAsync(
      { kind: 'device', uuid: device.info.value.vaultDeviceUuid },
      PREF_KEY,
    )
  } catch (e) {
    loadError.value = errString(e)
  } finally {
    loading.value = false
  }
}

async function chooseAsync(value: string) {
  const modelId = value || null
  busy.value = true
  savedFlash.value = false
  opError.value = null
  try {
    if (modelId) await setDefault({ modelId })
    else await clearDefault({})
    stored.value = modelId
    savedFlash.value = true
  } catch (e) {
    opError.value = errString(e)
  } finally {
    busy.value = false
  }
}

onMounted(() => reloadAsync())
onVaultTablesChanged(['preferences', 'providers'], () => {
  if (!busy.value) return reloadAsync(true)
})
</script>

<template>
  <section class="flex flex-col gap-3">
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>

    <p v-if="loadError" class="text-sm text-destructive" role="alert">
      {{ t('errors.prefLoadFailed') }}: {{ loadError }}
    </p>

    <template v-if="!loading && !loadError">
      <p v-if="modelListError" class="text-sm text-destructive" role="alert">
        {{ t('errors.modelListFailed') }}
      </p>
      <SettingsGroup
        v-if="!hasAnyModel && !modelListError"
        :label="t('settings.default.empty')"
        data-testid="settings-default-empty"
      >
        <SettingsRow
          to="/ai/download"
          icon="lucide:download"
          :title="t('settings.locations.ai.download.title')"
          :description="t('settings.locations.ai.download.description')"
        />
        <SettingsRow
          to="/ai/providers"
          icon="lucide:plug"
          :title="t('settings.locations.ai.providers.title')"
          :description="t('settings.locations.ai.providers.description')"
        />
      </SettingsGroup>

      <SettingsGroup>
        <SettingsRow
          :title="t('settings.default.label')"
          :description="t('settings.deviceOnly')"
          label-for="settings-default-model"
        >
          <SettingsSelect
            id="settings-default-model"
            :model-value="stored ?? ''"
            :options="extraOptions"
            :groups="selectGroups"
            :disabled="busy"
            data-testid="settings-default-model"
            @update:model-value="chooseAsync"
          />
        </SettingsRow>
      </SettingsGroup>

      <span v-if="savedFlash" class="text-xs text-success" role="status">
        {{ t('settings.default.saved') }}
      </span>
      <span v-if="opError" class="text-xs text-destructive" role="alert">
        {{ t('errors.prefSaveFailed') }}: {{ opError }}
      </span>
    </template>
  </section>
</template>
