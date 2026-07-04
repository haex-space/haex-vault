<template>
  <div class="space-y-4">
    <!-- Storage backend dropdown -->
    <div class="space-y-2">
      <label class="text-sm font-medium">{{ t('backendLabel') }}</label>
      <USelectMenu
        v-model="selectedBackend"
        :items="backendOptions"
        :placeholder="t('backendPlaceholder')"
        :loading="loading"
        by="value"
        class="w-full"
      />
    </div>

    <!-- Scope selection (only "whole bucket" for H2; tree comes in H3) -->
    <div
      v-if="selectedBackend"
      class="space-y-2"
    >
      <label class="text-sm font-medium">{{ t('scopeLabel') }}</label>
      <URadioGroup
        v-model="scopeMode"
        :items="scopeOptions"
      />
      <!-- Placeholder for the bucket tree browser (H3 will populate this). -->
      <div
        class="rounded-md border border-dashed border-muted p-4 text-xs text-muted"
      >
        {{ t('treePlaceholder') }}
      </div>
    </div>

    <!-- ARN preview -->
    <div
      v-if="selectedBackend"
      class="space-y-1"
    >
      <label class="text-xs uppercase tracking-wide text-muted">
        {{ t('arnPreviewLabel') }}
      </label>
      <code class="block text-xs break-all bg-gray-50 dark:bg-gray-800/50 px-2 py-1 rounded">
        {{ arnPreview }}
      </code>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { SelectHaexS3Backends } from '~/database/schemas'

type BackendOption = {
  label: string
  value: string
  bucket: string
}

const props = defineProps<{
  backends: SelectHaexS3Backends[]
  loading: boolean
}>()

const selectedBackendId = defineModel<string | null>('selectedBackendId', {
  required: true,
})

const { t } = useI18n()

// Scope for H2: only "whole bucket". H3 will add "prefix" and (maybe) "object".
const scopeMode = ref<'whole'>('whole')

const scopeOptions = computed(() => [
  { label: t('scopeWholeBucket'), value: 'whole' as const },
])

const backendOptions = computed<BackendOption[]>(() =>
  props.backends.map((b) => ({
    label: b.name,
    value: b.id,
    bucket: String((b.config as Record<string, unknown>)?.bucket ?? ''),
  })),
)

const selectedBackend = computed<BackendOption | undefined>({
  get: () =>
    selectedBackendId.value
      ? backendOptions.value.find((o) => o.value === selectedBackendId.value)
      : undefined,
  set: (opt) => {
    selectedBackendId.value = opt?.value ?? null
  },
})

const arnPreview = computed(() => {
  const bucket = selectedBackend.value?.bucket
  if (!bucket) return ''
  // For "whole bucket" scope. H3 will extend this with `/prefix/*` when the
  // tree browser adds prefix / object selection.
  return `arn:aws:s3:::${bucket}`
})
</script>

<i18n lang="yaml">
de:
  backendLabel: Storage-Backend auswählen
  backendPlaceholder: Backend wählen
  scopeLabel: Bereich
  scopeWholeBucket: Gesamter Bucket
  treePlaceholder: Bucket-Baum folgt in einem späteren Schritt.
  arnPreviewLabel: ARN Vorschau
en:
  backendLabel: Select storage backend
  backendPlaceholder: Choose backend
  scopeLabel: Scope
  scopeWholeBucket: Whole bucket
  treePlaceholder: Bucket tree browser coming in a later step.
  arnPreviewLabel: ARN preview
</i18n>
