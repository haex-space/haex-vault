<script setup lang="ts">
/**
 * One error line per marketplace that did not answer (spec 069, FR-031), above the catalog: the
 * entries of the others stay. The text names the cause from a fixed list, never what the
 * marketplace answered. "Erneut versuchen" asks again.
 */
import type { SourceError } from '@bindings/SourceError'
import { sourceErrorKey } from '~/lib/marketplace/errors'

defineProps<{ errors: readonly SourceError[] }>()
const emit = defineEmits<{ retry: [] }>()
const { t } = useI18n()
</script>

<template>
  <ul v-if="errors.length > 0" class="flex flex-col gap-2">
    <li
      v-for="error in errors"
      :key="error.sourceId"
      class="flex flex-wrap items-center gap-2 rounded-xl bg-destructive/10 px-4 py-2 text-sm text-destructive"
      role="alert"
      :data-testid="`marketplace-source-error-${error.sourceId}`"
    >
      <Icon name="lucide:triangle-alert" class="size-4 shrink-0" />
      <span class="min-w-0 flex-1">{{
        t(sourceErrorKey(error.kind), { name: error.name })
      }}</span>
      <UiButton
        variant="outline"
        size="sm"
        :data-testid="`marketplace-source-retry-${error.sourceId}`"
        @click="emit('retry')"
      >
        {{ t('marketplace.retry') }}
      </UiButton>
    </li>
  </ul>
</template>
