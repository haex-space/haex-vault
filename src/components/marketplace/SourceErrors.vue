<script setup lang="ts">
/**
 * The marketplaces that did not answer (spec 069, FR-031), above the catalog: the entries of the
 * others stay. Each failing source is a toast that names the cause from a fixed list, never what
 * the marketplace answered; it toasts when the source starts failing and again after a retry
 * that fails. "Erneut versuchen" stays here and asks again.
 */
import { toast } from 'vue-sonner'
import type { SourceError } from '@bindings/SourceError'
import { sourceErrorKey } from '~/lib/marketplace/errors'

const props = defineProps<{ errors: readonly SourceError[] }>()
const emit = defineEmits<{ retry: [] }>()
const { t } = useI18n()

/** The errors already toasted, so a quiet reload with the same answer does not repeat them. */
let toasted = new Set<string>()
watch(
  () => props.errors,
  (errors) => {
    const current = new Set<string>()
    for (const error of errors) {
      const key = `${error.sourceId}|${error.kind}`
      current.add(key)
      if (!toasted.has(key))
        toast.error(t(sourceErrorKey(error.kind), { name: error.name }))
    }
    toasted = current
  },
  { immediate: true },
)

function retry(): void {
  // The answer to the retry is new: a source that still fails says so again.
  toasted = new Set()
  emit('retry')
}
</script>

<template>
  <div v-if="errors.length > 0" class="flex justify-center">
    <UiButton
      variant="outline"
      size="sm"
      data-testid="marketplace-source-retry"
      @click="retry"
    >
      <Icon name="lucide:refresh-cw" class="size-4" />
      {{ t('marketplace.retry') }}
    </UiButton>
  </div>
</template>
