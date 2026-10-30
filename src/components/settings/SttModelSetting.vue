<script setup lang="ts">
/**
 * Speech-to-text model (spec 010). The select lists only installed models and saves on selection
 * (spec 023 FR-021); a model that is not installed is fetched and activated with its own button,
 * because a download is an action, not a value (clarification 2026-09-26).
 */
import type { SttCatalogEntry } from '~/composables/useSttCatalog'
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import { useSettingsDevice } from '~/components/settings/deviceContext'

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync } = usePreferences()
const setStt = useActionOrThrow('settings.models.setStt')
const { listAsync: listSttCatalogAsync } = useSttCatalog()
const { listInstalledAsync } = useSttModels()
const device = useSettingsDevice()

const PREF_KEY = 'voice.stt_model_id'
const DEFAULT_ID = 'whisper-tiny'

const catalog = ref<SttCatalogEntry[]>([])
const installedIds = ref<Set<string>>(new Set())
const activeId = ref(DEFAULT_ID)

const loading = ref(true)
const busyId = ref<string | null>(null)
const savedFlash = ref(false)
const opError = ref<string | null>(null)
const loadError = ref<string | null>(null)

const installed = computed(() =>
  catalog.value.filter((entry) => installedIds.value.has(entry.id)),
)
const sttOptions = computed<SettingsSelectOption[]>(() => [
  ...(installedIds.value.has(activeId.value)
    ? []
    : [
        {
          value: activeId.value,
          label: t('settings.sttModel.notInstalled'),
          disabled: true,
        },
      ]),
  ...installed.value.map((entry) => ({ value: entry.id, label: entry.name })),
])
const available = computed(() =>
  catalog.value.filter((entry) => !installedIds.value.has(entry.id)),
)

/** Engine and languages in one line (spec 066 T136). */
function engineLabel(entry: SttCatalogEntry): string {
  return t(`settings.sttModel.engine.${entry.engine}`, {
    count: entry.languages.length,
  })
}

/** A `quiet` reload (after a change from elsewhere) keeps the shown values until the new ones are read. */
async function reloadAsync(quiet = false) {
  if (!quiet) loading.value = true
  loadError.value = null
  try {
    const [entries, installedModels, pref] = await Promise.all([
      listSttCatalogAsync(),
      listInstalledAsync(),
      getPrefAsync(
        { kind: 'device', uuid: device.info.value.vaultDeviceUuid },
        PREF_KEY,
      ),
    ])
    catalog.value = entries
    installedIds.value = new Set(installedModels.map((m) => m.id))
    activeId.value =
      pref && entries.some((entry) => entry.id === pref) ? pref : DEFAULT_ID
  } catch (e) {
    loadError.value = errString(e)
  } finally {
    loading.value = false
  }
}

/** Activates a model; `setStt` downloads it first when it is not installed. */
async function activateAsync(catalogId: string) {
  // The default counts as active before it is installed; downloading it still has to run.
  const alreadyActive =
    catalogId === activeId.value && installedIds.value.has(catalogId)
  if (alreadyActive || busyId.value) return
  busyId.value = catalogId
  savedFlash.value = false
  opError.value = null
  try {
    const { modelId } = (await setStt({ catalogId })) as { modelId: string }
    activeId.value = modelId
    installedIds.value = new Set([...installedIds.value, modelId])
    savedFlash.value = true
  } catch (e) {
    opError.value = errString(e)
  } finally {
    busyId.value = null
  }
}

/** The model whose download waits for the confirmation (spec 043 FR-028). */
const confirming = ref<SttCatalogEntry | null>(null)
const confirmOpen = computed({
  get: () => confirming.value !== null,
  set: (value: boolean) => {
    if (!value) confirming.value = null
  },
})

function confirmed() {
  const entry = confirming.value
  confirming.value = null
  if (entry) void activateAsync(entry.id)
}

onMounted(() => reloadAsync())
onVaultTablesChanged(['preferences'], () => {
  if (!busyId.value) return reloadAsync(true)
})
</script>

<template>
  <section class="flex flex-col gap-6">
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('onboarding.wizard.loadingDeviceInfo') }}
    </div>

    <p v-if="loadError" class="text-sm text-destructive" role="alert">
      {{ t('errors.prefLoadFailed') }}: {{ loadError }}
    </p>

    <template v-if="!loading && !loadError">
      <SettingsGroup v-if="installed.length > 0">
        <SettingsRow
          :title="t('settings.sttModel.modelLabel')"
          :description="t('settings.deviceOnly')"
          label-for="settings-stt-model-select"
        >
          <SettingsSelect
            id="settings-stt-model-select"
            :model-value="activeId"
            :options="sttOptions"
            :disabled="busyId !== null"
            data-testid="settings-stt-model"
            @update:model-value="activateAsync"
          />
        </SettingsRow>
      </SettingsGroup>
      <p v-else class="text-sm text-muted-foreground">
        {{ t('settings.sttModel.noneInstalled') }}
      </p>

      <SettingsGroup
        v-if="available.length > 0"
        :label="t('settings.sttModel.availableLabel')"
      >
        <SettingsRow
          v-for="entry in available"
          :key="entry.id"
          :title="entry.name"
          :description="engineLabel(entry)"
        >
          <UiButton
            type="button"
            variant="outline"
            size="sm"
            :loading="busyId === entry.id"
            :disabled="busyId !== null"
            @click="confirming = entry"
          >
            {{
              busyId === entry.id
                ? t('settings.sttModel.downloading')
                : t('settings.sttModel.downloadAndUse')
            }}
          </UiButton>
        </SettingsRow>
      </SettingsGroup>

      <ModelsDownloadConfirmStep
        v-model:open="confirmOpen"
        :target="confirming && { kind: 'stt', id: confirming.id }"
        :name="confirming?.name ?? ''"
        @confirm="confirmed"
      />

      <p v-if="savedFlash" class="px-1 text-xs text-success" role="status">
        {{ t('settings.sttModel.saved') }}
      </p>
      <p v-if="opError" class="px-1 text-xs text-destructive" role="alert">
        {{ t('settings.sttModel.downloadFailed') }}: {{ opError }}
      </p>
    </template>
  </section>
</template>
