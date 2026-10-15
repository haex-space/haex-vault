<script setup lang="ts">
/**
 * Download models (spec 005, moved to `/ai/download` by spec 023, research R10): the
 * recommended catalog models with their download progress, and the way into the HuggingFace
 * search as a sub-view.
 */
import type { CatalogEntryWithFit } from '~/composables/useCatalog'

const { t } = useI18n()
const downloadCatalog = useActionOrThrow('settings.models.downloadCatalog')
const downloads = useModelDownloadsStore()
const modelStore = useModelsStore()
const { installedModels: installed, catalogEntries } = storeToRefs(modelStore)

const loading = ref(true)
const busyModelId = ref<string | null>(null)
const errorKey = ref<string | null>(null)
const errorDetail = ref<string | null>(null)

async function reloadAsync() {
  loading.value = true
  errorKey.value = null
  errorDetail.value = null
  try {
    await modelStore.refreshInstalledAndCatalog()
  } catch (e) {
    errorKey.value = hfErrorKey(e)
    errorDetail.value = hfErrorDetail(e)
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
  return installed.value.some((model) => model.id === entry.id)
}

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
  errorKey.value = null
  errorDetail.value = null
  try {
    await downloadCatalog({ entryId: entry.id })
    await reloadAsync()
  } catch (e) {
    errorKey.value = hfErrorKey(e)
    errorDetail.value = hfErrorDetail(e)
  } finally {
    busyModelId.value = null
    downloads.clearDownload(entry.id)
  }
}

onMounted(reloadAsync)
</script>

<template>
  <section class="flex flex-col gap-6">
    <SettingsGroup>
      <SettingsRow
        to="/ai/download/search"
        icon="lucide:search"
        :title="t('settings.locations.ai.download.search.title')"
        :description="t('settings.locations.ai.download.search.description')"
        data-testid="settings-row-ai.download.search"
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

    <p v-if="errorKey" class="text-sm text-destructive" role="alert">
      {{ t(errorKey) }}
      <span v-if="errorDetail" class="block text-xs">{{ errorDetail }}</span>
    </p>
    <div v-if="loading" class="text-sm text-muted-foreground">
      {{ t('models.search.loading') }}
    </div>

    <SettingsGroup
      v-if="catalogEntries.length > 0"
      :label="
        otherDownloads.length > 0
          ? t('settings.downloadModels.recommended')
          : undefined
      "
    >
      <SettingsRow v-for="entry in catalogEntries" :key="entry.id">
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
              {{ t(`models.filePicker.fit.${entry.fit}`) }}</span
            >
            <ModelsDownloadStatus :model-id="entry.id" />
          </span>
        </template>
        <UiButton
          type="button"
          size="sm"
          :disabled="isInstalled(entry)"
          :loading="busyModelId === entry.id"
          :data-testid="`models-download-${entry.id}`"
          @click="confirming = entry"
        >
          {{
            isInstalled(entry)
              ? t('models.installed.active')
              : t('models.filePicker.install')
          }}
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
