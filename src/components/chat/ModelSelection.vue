<script setup lang="ts">
/**
 * The chat page's "nothing installed/configured yet" state, rendered
 * inline inside the message area — never blocking the composer or thread
 * sidebar, so the chat window itself is reachable the instant the page
 * opens. Only shown when literally no model exists anywhere (parent gates
 * on `noModelsInstalled`); picking among models that DO exist happens
 * exclusively through the composer's own model control
 * (`ChatComposerSettingsPopover`) — see `[instance].vue`.
 *
 * Reads `useModelsStore` directly for its state (`catalogEntries`,
 * `downloadingId`, `downloadProgressBytes`/`downloadTotalBytes`,
 * `modelGroups`); downloads run the `chat.model.downloadRecommended` action
 * (spec 020) once the person confirmed the size (spec 043 FR-028).
 */
const { t } = useI18n()
const modelStore = useModelsStore()
const {
  catalogEntries,
  downloadingId,
  downloadProgressBytes,
  downloadTotalBytes,
  modelGroups,
} = storeToRefs(modelStore)
const downloadRecommended = useAction('chat.model.downloadRecommended')

/** The entry whose download waits for the confirmation (spec 043 FR-028). */
const confirming = ref<(typeof catalogEntries.value)[number] | null>(null)
const confirmOpen = computed({
  get: () => confirming.value !== null,
  set: (value: boolean) => {
    if (!value) confirming.value = null
  },
})

function confirmed() {
  const entry = confirming.value
  confirming.value = null
  if (entry) void downloadRecommended({ entryId: entry.id })
}

/** Maps a hardware-fit verdict to its localized display label. */
function fitLabel(f: (typeof catalogEntries.value)[number]['fit']): string {
  return t(`chat.fit.${f}`)
}

/** Formats a byte count for the model download UI. */
function humanBytes(n: number | null): string {
  if (n === null) return '?'
  const kb = 1024
  const mb = kb * 1024
  const gb = mb * 1024
  if (n >= gb) return (n / gb).toFixed(1) + ' GB'
  if (n >= mb) return (n / mb).toFixed(0) + ' MB'
  return (n / kb).toFixed(0) + ' KB'
}

function downloadProgressPercent(
  modelId: string,
  downloadingId: string | null,
  downloadTotalBytes: number | null,
  downloadProgressBytes: number,
): number | null {
  if (
    downloadingId !== modelId ||
    downloadTotalBytes === null ||
    downloadTotalBytes <= 0
  )
    return null
  return Math.min(
    100,
    Math.max(0, Math.round((downloadProgressBytes / downloadTotalBytes) * 100)),
  )
}
</script>

<template>
  <div class="mx-auto w-full max-w-2xl py-6">
    <h2 class="text-lg font-semibold mb-4">
      {{ t('chat.empty.noModelsTitle') }}
    </h2>
    <p class="text-sm text-muted-foreground mb-6">
      {{ t('chat.empty.noModelsDescription') }}
    </p>
    <div class="space-y-2">
      <div
        v-for="e in catalogEntries"
        :key="e.id"
        class="relative overflow-hidden border border-border rounded p-3 flex items-center justify-between gap-4"
      >
        <UiProgress
          v-if="downloadingId === e.id"
          :value="
            downloadProgressPercent(
              e.id,
              downloadingId,
              downloadTotalBytes,
              downloadProgressBytes,
            )
          "
          :label="`${humanBytes(downloadProgressBytes)} / ${humanBytes(downloadTotalBytes)}`"
          variant="backdrop"
        />
        <div class="relative z-10 flex-1 min-w-0">
          <div class="font-medium text-sm">
            {{ e.name }}
          </div>
          <div class="text-xs text-muted-foreground truncate">
            {{ e.hf_repo }}/{{ e.hf_filename }}
          </div>
          <div class="text-xs text-muted-foreground">
            {{
              t('chat.catalog.meta', {
                size: humanBytes(e.approx_size_bytes),
                context: e.context_window.toLocaleString(),
                license: e.license,
                fit: fitLabel(e.fit),
              })
            }}
          </div>
        </div>
        <div class="relative z-10">
          <UiButton
            size="sm"
            :disabled="downloadingId !== null"
            @click="confirming = e"
          >
            <template v-if="downloadingId === e.id">
              {{ humanBytes(downloadProgressBytes) }} /
              {{ humanBytes(downloadTotalBytes) }}
            </template>
            <template v-else>
              {{ t('chat.download') }}
            </template>
          </UiButton>
        </div>
      </div>
    </div>
    <ModelsDownloadConfirmStep
      v-model:open="confirmOpen"
      :target="confirming && { kind: 'catalog', id: confirming.id }"
      :name="confirming?.name ?? ''"
      @confirm="confirmed"
    />
    <div v-if="modelGroups.length > 0" class="mt-6 space-y-2">
      <div
        v-for="group in modelGroups"
        :key="group.providerId"
        class="border border-border rounded p-3"
        aria-disabled="true"
      >
        <div class="font-medium text-sm">{{ group.providerName }}</div>
        <div
          v-for="model in group.models"
          :key="model.id"
          class="text-sm text-muted-foreground"
        >
          {{ model.name }}
        </div>
      </div>
    </div>
  </div>
</template>
