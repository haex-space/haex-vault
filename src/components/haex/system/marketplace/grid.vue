<template>
  <div class="contents">
    <!-- Source errors: shown when one or more marketplaces fail to load -->
    <div
      v-if="Object.keys(sourceErrors).length"
      class="px-6 py-2 flex flex-wrap gap-2"
    >
      <UBadge
        v-for="(entry, id) in sourceErrors"
        :key="id"
        color="warning"
        variant="soft"
        class="text-xs"
      >
        {{ entry.name }}: {{ entry.message }}
      </UBadge>
    </div>

    <!-- Loading State -->
    <div
      v-if="isLoading"
      class="flex-1 flex flex-col items-center justify-center gap-3"
    >
      <UIcon
        name="i-heroicons-arrow-path"
        class="w-8 h-8 animate-spin text-gray-400"
      />
      <p class="text-sm text-gray-500">{{ t('loading') }}</p>
    </div>

    <!-- Extensions Grid -->
    <div
      v-else-if="extensions.length"
      class="flex-1 overflow-auto p-6"
    >
      <div class="grid grid-cols-1 @xl:grid-cols-2 gap-6">
        <HaexExtensionMarketplaceCard
          v-for="ext in extensions"
          :key="ext.id"
          :extension="ext"
          @install="emit('install', ext)"
          @update="emit('update', ext)"
          @details="emit('details', ext)"
          @remove="emit('remove', ext)"
        />
      </div>

      <!-- Pagination -->
      <div
        v-if="extensionsTotal > 20"
        class="flex justify-center mt-6"
      >
        <UPagination
          v-model="pageModel"
          :total="extensionsTotal"
          :items-per-page="20"
        />
      </div>
    </div>

    <!-- Empty State -->
    <div
      v-else
      class="flex flex-col items-center justify-center flex-1 text-center p-6"
    >
      <UIcon
        name="i-heroicons-puzzle-piece"
        class="w-16 h-16 text-gray-400 mb-4"
      />
      <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
        {{ t('empty.title') }}
      </h3>
      <p class="text-gray-500 dark:text-gray-400 mt-2">
        {{ t('empty.description') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { MarketplaceExtensionViewModel } from '~/types/haexspace'

interface SourceErrorEntry {
  name: string
  message: string
}

const props = defineProps<{
  extensions: MarketplaceExtensionViewModel[]
  extensionsTotal: number
  isLoading: boolean
  sourceErrors: Record<string, SourceErrorEntry>
  currentPage: number
}>()

const emit = defineEmits<{
  install: [ext: MarketplaceExtensionViewModel]
  update: [ext: MarketplaceExtensionViewModel]
  details: [ext: MarketplaceExtensionViewModel]
  remove: [ext: MarketplaceExtensionViewModel]
  'update:currentPage': [value: number]
}>()

const { t } = useI18n()

const pageModel = computed({
  get: () => props.currentPage,
  set: (value: number) => emit('update:currentPage', value),
})
</script>

<i18n lang="yaml">
de:
  loading: Erweiterungen werden geladen...
  empty:
    title: Keine Erweiterungen gefunden
    description: Versuche einen anderen Suchbegriff oder eine andere Kategorie
en:
  loading: Loading extensions...
  empty:
    title: No extensions found
    description: Try a different search term or category
</i18n>
