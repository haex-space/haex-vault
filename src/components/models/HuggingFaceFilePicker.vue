<script setup lang="ts">
/**
 * File choice of one HuggingFace repository, the location `/ai/download/repo/:owner/:name`
 * (spec 005, spec 023 research R1/R10). `?files=a.gguf,b.gguf` limits the list to the files that
 * matched the search filters; without it (a deep link) every GGUF file is offered. Progress comes
 * from the download store, so it survives leaving the view.
 */
import type {
  HuggingFaceFileCandidate,
  HuggingFaceModelResult,
  InstallPreview,
} from '~/composables/useHuggingFace'
import type { InstalledModel } from '~/composables/useModels'
import { humanBytes } from '~/lib/models/format'

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const router = useTabRouter()
const { detailsAsync, previewInstallAsync } = useHuggingFace()
const downloadFromHf = useActionOrThrow('settings.models.downloadFromHf')
const downloads = useModelDownloadsStore()

const repoId = computed(
  () => `${router.route.params.owner ?? ''}/${router.route.params.name ?? ''}`,
)
const allowedFilenames = computed(() => {
  const files = router.route.query.files
  return files ? files.split(',').filter((name) => name.length > 0) : null
})

const details = ref<HuggingFaceModelResult | null>(null)
const detailsErrorKey = ref<string | null>(null)
const loadingDetails = ref(false)

const selectedFile = ref<HuggingFaceFileCandidate | null>(null)
const preview = ref<InstallPreview | null>(null)
const previewErrorKey = ref<string | null>(null)
const loadingPreview = ref(false)

const tokenizerRepoInput = ref('')
const tooBigConfirmed = ref(false)

const installing = ref(false)
const installErrorKey = ref<string | null>(null)
const installErrorDetail = ref<string | null>(null)

const visibleFiles = computed(() => {
  if (!details.value) return []
  if (!allowedFilenames.value) return details.value.files
  const allowed = new Set(allowedFilenames.value)
  return details.value.files.filter((file) => allowed.has(file.filename))
})

async function loadDetailsAsync() {
  loadingDetails.value = true
  detailsErrorKey.value = null
  try {
    details.value = await detailsAsync(repoId.value)
  } catch (e) {
    detailsErrorKey.value = hfErrorKey(e)
  } finally {
    loadingDetails.value = false
  }
}

/** The radio group reports the chosen file by its name. */
function selectFileByName(filename: unknown) {
  const file = visibleFiles.value.find((f) => f.filename === filename)
  if (file) void selectFileAsync(file)
}

async function selectFileAsync(file: HuggingFaceFileCandidate) {
  selectedFile.value = file
  preview.value = null
  previewErrorKey.value = null
  tokenizerRepoInput.value = ''
  tooBigConfirmed.value = false
  loadingPreview.value = true
  try {
    preview.value = await previewInstallAsync({
      repoId: file.repoId,
      filename: file.filename,
      // Details exposes the resolved source SHA. Pass only a real tracked
      // ref here; otherwise preview would turn the default `main` tracking
      // into an untracked direct SHA pin and update checks would disappear.
      revision: file.revisionRef ?? undefined,
    })
  } catch (e) {
    previewErrorKey.value = hfErrorKey(e)
  } finally {
    loadingPreview.value = false
  }
}

const effectiveTokenizerRepo = computed(
  () =>
    preview.value?.tokenizerRepo ?? (tokenizerRepoInput.value.trim() || null),
)
const needsTooBigConfirmation = computed(
  () => preview.value?.requiresExplicitTooBigConfirmation ?? false,
)
const canInstall = computed(
  () =>
    preview.value !== null &&
    effectiveTokenizerRepo.value !== null &&
    (!needsTooBigConfirmation.value || tooBigConfirmed.value) &&
    !installing.value,
)

const downloadModelId = computed(() => preview.value?.modelId ?? '')

/** The question before the download (spec 043 FR-028). */
const confirmOpen = ref(false)

async function installAsync() {
  if (!preview.value || !selectedFile.value || !canInstall.value) return
  installing.value = true
  installErrorKey.value = null
  installErrorDetail.value = null
  try {
    const model = (await downloadFromHf({
      repoId: selectedFile.value.repoId,
      filename: selectedFile.value.filename,
      // Preserve the preview's update semantics: tracked refs remain
      // tracked, while a deliberate direct SHA remains pinned.
      revision: preview.value.revisionRef ?? preview.value.revision,
      name: preview.value.name,
      tokenizerRepo: effectiveTokenizerRepo.value ?? undefined,
      contextWindow: preview.value.contextWindow ?? undefined,
      forceTooBig: tooBigConfirmed.value,
    })) as InstalledModel
    downloads.clearDownload(model.id)
    router.push('/ai/installed')
  } catch (e) {
    downloads.clearDownload(downloadModelId.value)
    installErrorKey.value = hfErrorKey(e)
    installErrorDetail.value = hfErrorDetail(e)
  } finally {
    installing.value = false
  }
}

function sizeLabel(bytes: number | null): string {
  return bytes === null ? t('models.filePicker.sizeUnknown') : humanBytes(bytes)
}

/** One line under the file name: size, quantization, fit and a catalog match. */
function fileSummary(file: HuggingFaceFileCandidate): string {
  return [
    sizeLabel(file.sizeBytes),
    file.quantization ?? t('models.filePicker.quantizationUnknown'),
    t(`models.filePicker.fit.${file.fit}`),
    ...(file.catalogMatch ? [t('models.result.catalogMatch')] : []),
  ].join(' · ')
}

onMounted(loadDetailsAsync)
</script>

<template>
  <section class="flex flex-col gap-3">
    <p
      v-if="loadingDetails"
      class="text-sm text-muted-foreground"
      role="status"
    >
      {{ t('models.search.loading') }}
    </p>
    <SettingsGroup
      v-if="detailsErrorKey || (details && details.files.length === 0)"
      data-testid="settings-repo-missing"
    >
      <SettingsRow
        to="/ai/download/search"
        icon="lucide:search"
        :title="t('settings.locations.ai.download.search.title')"
      >
        <template #description>
          <span
            :class="detailsErrorKey ? 'text-destructive' : ''"
            :role="detailsErrorKey ? 'alert' : undefined"
          >
            {{
              detailsErrorKey
                ? t(detailsErrorKey)
                : t('models.result.noGgufFiles')
            }}
          </span>
        </template>
      </SettingsRow>
    </SettingsGroup>
    <p
      v-else-if="details && visibleFiles.length === 0"
      class="text-sm text-muted-foreground"
    >
      {{ t('models.search.filters.empty') }}
    </p>

    <SettingsGroup
      v-if="details && visibleFiles.length > 0"
      radio
      :model-value="selectedFile?.filename ?? null"
      @update:model-value="selectFileByName"
    >
      <SettingsOptionRow
        v-for="file in visibleFiles"
        :key="file.filename"
        type="radio"
        :value="file.filename"
        :title="file.filename"
        :description="fileSummary(file)"
      />
    </SettingsGroup>

    <div
      v-if="selectedFile"
      class="relative flex flex-col gap-3 overflow-hidden rounded-xl bg-muted p-4"
    >
      <ModelsDownloadBar v-if="preview" :model-id="downloadModelId" />
      <div class="relative z-10 flex flex-col gap-3">
        <p
          v-if="loadingPreview"
          class="text-sm text-muted-foreground"
          role="status"
        >
          {{ t('models.search.loading') }}
        </p>
        <p v-if="previewErrorKey" class="text-sm text-destructive" role="alert">
          {{ t(previewErrorKey) }}
        </p>

        <template v-if="preview">
          <dl class="grid grid-cols-2 gap-x-3 gap-y-1 text-xs">
            <dt class="text-muted-foreground">
              {{ t('models.filePicker.size') }}
            </dt>
            <dd>{{ sizeLabel(preview.sizeBytes) }}</dd>
            <dt class="text-muted-foreground">
              {{ t('models.filePicker.quantization') }}
            </dt>
            <dd>
              {{
                preview.quantization ??
                t('models.filePicker.quantizationUnknown')
              }}
            </dd>
            <dt class="text-muted-foreground">
              {{ t('models.filePicker.contextWindow') }}
            </dt>
            <dd>
              {{
                preview.contextWindow ??
                t('models.filePicker.contextWindowUnknown')
              }}
            </dd>
            <dt class="text-muted-foreground">
              {{ t('models.filePicker.revision') }}
            </dt>
            <dd class="truncate font-mono">
              {{ preview.revision }}
            </dd>
            <template v-if="preview.revisionRef">
              <dt class="text-muted-foreground">
                {{ t('models.filePicker.revisionRef') }}
              </dt>
              <dd>{{ preview.revisionRef }}</dd>
            </template>
          </dl>

          <div v-if="preview.tokenizerRequired" class="flex flex-col gap-1">
            <p class="text-xs text-warning">
              {{ t('models.filePicker.tokenizerHint') }}
            </p>
            <UiInput
              v-model="tokenizerRepoInput"
              :label="t('models.filePicker.tokenizerRepoLabel')"
              :placeholder="t('models.filePicker.tokenizerRepoPlaceholder')"
              :labels="fieldLabels.input.value"
              label-bg="var(--muted)"
            />
          </div>

          <div v-if="needsTooBigConfirmation" class="flex flex-col gap-1">
            <p class="text-sm text-destructive" role="alert">
              {{ t('models.filePicker.tooBigWarning') }}
            </p>
            <label class="flex items-center gap-2 text-sm">
              <ShadcnCheckbox v-model="tooBigConfirmed" />
              {{ t('models.filePicker.tooBigConfirm') }}
            </label>
          </div>

          <p
            v-if="installErrorKey"
            class="text-sm text-destructive"
            role="alert"
          >
            {{ t(installErrorKey) }}
            <span v-if="installErrorDetail" class="block text-xs">{{
              installErrorDetail
            }}</span>
          </p>
          <ModelsDownloadStatus :model-id="downloadModelId" />

          <UiButton
            type="button"
            :disabled="!canInstall"
            :loading="installing"
            @click="confirmOpen = true"
          >
            {{ t('models.filePicker.install') }}
          </UiButton>
          <ModelsDownloadConfirmStep
            v-model:open="confirmOpen"
            :target="{ kind: 'file', sizeBytes: preview.sizeBytes ?? null }"
            :name="preview.name"
            @confirm="installAsync"
          />
        </template>
      </div>
    </div>
  </section>
</template>
