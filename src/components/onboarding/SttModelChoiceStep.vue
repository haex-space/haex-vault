<script setup lang="ts">
import type { TierRecommendation, Tier } from '~/composables/useCatalog'
import type { SttCatalogEntry } from '~/composables/useSttCatalog'
import { humanBytes } from '~/lib/models/format'

const { t } = useI18n()

defineProps<{
  tiers: TierRecommendation<SttCatalogEntry>[]
  downloadingId: string | null
  downloadError: string | null
}>()

const emit = defineEmits<{
  choose: [tier: TierRecommendation<SttCatalogEntry>]
  skip: []
  back: []
}>()

function tierLabelKey(tier: Tier): string {
  return `onboarding.model.tier.${tier}`
}

function fitLabelKey(fit: string): string {
  return `onboarding.model.fit.${fit}`
}

/** The choice whose download waits for the confirmation (spec 043 FR-028). */
const confirming = ref<TierRecommendation<SttCatalogEntry> | null>(null)
const confirmOpen = computed({
  get: () => confirming.value !== null,
  set: (value: boolean) => {
    if (!value) confirming.value = null
  },
})

function confirmed() {
  const rec = confirming.value
  confirming.value = null
  if (rec) emit('choose', rec)
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <h2 class="text-xl font-semibold">
      {{ t('onboarding.sttModel.title') }}
    </h2>
    <p class="text-sm text-muted-foreground">
      {{ t('onboarding.sttModel.description') }}
    </p>

    <div v-if="tiers.length === 0" class="text-sm text-muted-foreground">
      {{ t('onboarding.sttModel.empty') }}
    </div>

    <div v-else class="grid grid-cols-1 sm:grid-cols-3 gap-3">
      <button
        v-for="rec in tiers"
        :key="`${rec.tier}-${rec.entry.id}`"
        type="button"
        class="flex flex-col gap-2 rounded-md border border-input p-3 text-left hover:border-primary focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-50 disabled:cursor-progress"
        :disabled="downloadingId !== null"
        @click="confirming = rec"
      >
        <span class="text-sm font-semibold uppercase tracking-wide">
          {{ t(tierLabelKey(rec.tier)) }}
        </span>
        <span class="text-base font-medium">{{ rec.entry.name }}</span>
        <span class="text-xs text-muted-foreground">
          {{
            t(`settings.sttModel.engine.${rec.entry.engine}`, {
              count: rec.entry.languages.length,
            })
          }}
          · {{ humanBytes(rec.entry.approxSizeBytes) }}
        </span>
        <span class="text-xs">
          {{ t(fitLabelKey(rec.fit)) }}
        </span>
      </button>
    </div>

    <p v-if="downloadingId" class="text-sm text-muted-foreground" role="status">
      {{ t('onboarding.sttModel.downloading') }} ({{ downloadingId }})
    </p>
    <p v-if="downloadError" class="text-sm text-destructive" role="alert">
      {{ t('onboarding.sttModel.downloadFailed') }}: {{ downloadError }}
    </p>

    <ModelsDownloadConfirmStep
      v-model:open="confirmOpen"
      :target="confirming && { kind: 'stt', id: confirming.entry.id }"
      :name="confirming?.entry.name ?? ''"
      @confirm="confirmed"
    />

    <div class="flex items-center justify-between">
      <UiButton
        variant="ghost"
        :disabled="downloadingId !== null"
        @click="emit('back')"
      >
        {{ t('onboarding.wizard.back') }}
      </UiButton>
      <UiButton
        variant="outline"
        :disabled="downloadingId !== null"
        @click="emit('skip')"
      >
        {{ t('onboarding.sttModel.skip') }}
      </UiButton>
    </div>
  </div>
</template>
