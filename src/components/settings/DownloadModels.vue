<script setup lang="ts">
/**
 * Download models (spec 005; the lower part of the location `/ai/local`, spec 023 research R10):
 * the recommended catalog models not installed yet with their download progress, and the way
 * into the HuggingFace search as a sub-view.
 */
import { toast } from 'vue-sonner'
import type { CatalogEntryWithFit } from '~/composables/useCatalog'

const { t } = useI18n()
const downloadCatalog = useActionOrThrow('settings.models.downloadCatalog')
const cancelDownload = useActionOrThrow('settings.models.cancelDownload')
const downloads = useModelDownloadsStore()
const modelStore = useModelsStore()
const { installedModels: installed, catalogEntries } = storeToRefs(modelStore)

const loading = ref(true)
const busyModelId = ref<string | null>(null)

/** A HuggingFace failure: its i18n message, with the raw reason underneath when there is one. */
function toastHfError(e: unknown) {
  toast.error(t(hfErrorKey(e)), { description: hfErrorDetail(e) ?? undefined })
}

async function reloadAsync() {
  loading.value = true
  try {
    await modelStore.refreshInstalledAndCatalog()
  } catch (e) {
    toastHfError(e)
  } finally {
    loading.value = false
  }
}

/** Downloads that are no catalog row, e.g. a HuggingFace file started from its repository. */
const otherDownloads = computed(() => {
  const catalogIds = new Set(catalogEntries.value.map((entry) => entry.id))
  return Object.keys(downloads.downloads).filter((id) => !catalogIds.has(id))
})

function isInstalled(entry: CatalogEntryWithFit): boolean {
  return installed.value.some(
    (model) =>
      model.id === entry.id ||
      (model.hfRepo === entry.hf_repo &&
        model.hfFilename === entry.hf_filename),
  )
}

/** An installed model is listed above, with the models on this device. */
const notInstalled = computed(() =>
  catalogEntries.value.filter((entry) => !isInstalled(entry)),
)

/** The entry whose download waits for the confirmation (spec 043 FR-028). */
const confirming = ref<CatalogEntryWithFit | null>(null)
const confirmOpen = computed({
  get: () => confirming.value !== null,
  set: (value: boolean) => {
    if (!value) confirming.value = null
  },
})

function confirmed() {
  const entry = confirming.value
  confirming.value = null
  if (entry) void downloadAsync(entry)
}

async function downloadAsync(entry: CatalogEntryWithFit) {
  busyModelId.value = entry.id
  try {
    await downloadCatalog({ entryId: entry.id })
    await reloadAsync()
  } catch (e) {
    // A cancelled download is what the person asked for, not an error.
    if (!isDownloadCancelled(e)) toastHfError(e)
  } finally {
    busyModelId.value = null
    downloads.clearDownload(entry.id)
  }
}

async function cancelAsync(entry: CatalogEntryWithFit) {
  try {
    await cancelDownload({ modelId: entry.id })
  } catch (e) {
    toastHfError(e)
  }
}

onMounted(reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-6">
    <SettingsGroup>
      <SettingsRow
        to="/ai/local/search"
        icon="lucide:search"
        :title="t('settings.locations.ai.local.search.title')"
        :description="t('settings.locations.ai.local.search.description')"
        data-testid="settings-row-ai.local.search"
      />
    </SettingsGroup>

    <SettingsGroup
      v-if="otherDownloads.length > 0"
      :label="t('settings.downloadModels.running')"
    >
      <SettingsRow v-for="id in otherDownloads" :key="id">
        <template #backdrop>
          <ModelsDownloadBar :model-id="id" />
        </template>
        <template #title>
          <span class="font-mono text-sm">{{ id }}</span>
        </template>
        <template #description>
          <ModelsDownloadStatus :model-id="id" />
        </template>
      </SettingsRow>
    </SettingsGroup>

    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('models.search.loading') }}
    </div>

    <SettingsGroup
      v-if="notInstalled.length > 0"
      :label="
        otherDownloads.length > 0
          ? t('settings.downloadModels.recommended')
          : undefined
      "
    >
      <SettingsRow v-for="entry in notInstalled" :key="entry.id">
        <template #backdrop>
          <ModelsDownloadBar :model-id="entry.id" />
        </template>
        <template #title>
          <span class="font-medium">{{ entry.name }}</span>
        </template>
        <template #description>
          <span class="flex flex-col text-xs">
            <span
              >{{ entry.parameters }} · {{ entry.quantization }} ·
              {{ t(`models.filePicker.fit.${entry.fit}`)
              }}{{ entry.trial ? ` · ${t('chat.catalog.trial')}` : '' }}</span
            >
            <ModelsDownloadStatus :model-id="entry.id" />
          </span>
        </template>
        <UiButton
          v-if="busyModelId === entry.id"
          type="button"
          size="sm"
          variant="outline"
          :data-testid="`models-cancel-${entry.id}`"
          @click="cancelAsync(entry)"
        >
          {{ t('models.filePicker.cancel') }}
        </UiButton>
        <UiButton
          v-else
          type="button"
          size="sm"
          :data-testid="`models-download-${entry.id}`"
          @click="confirming = entry"
        >
          {{ t('models.filePicker.install') }}
        </UiButton>
      </SettingsRow>
    </SettingsGroup>
    <ModelsDownloadConfirmStep
      v-model:open="confirmOpen"
      :target="confirming && { kind: 'catalog', id: confirming.id }"
      :name="confirming?.name ?? ''"
      @confirm="confirmed"
    />
  </section>
</template>
