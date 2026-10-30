<script setup lang="ts">
/**
 * The row above the catalog (spec 069, FR-012, FR-017): search, category, marketplace and
 * "Aktualisieren". The marketplace selector appears when more than one is switched on.
 */
import { Search } from '@lucide/vue'
import type { CatalogCategoryCount } from '@bindings/CatalogCategoryCount'
import type { MarketplaceView } from '@bindings/MarketplaceView'

const props = defineProps<{
  categories: readonly CatalogCategoryCount[]
  markets: readonly MarketplaceView[]
  category: string
  source: string
  loading: boolean
}>()
const search = defineModel<string>('search', { required: true })
const emit = defineEmits<{
  category: [slug: string]
  source: [id: string]
  refresh: []
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()

const categoryOptions = computed(() => [
  { value: '', label: t('marketplace.category.all') },
  ...props.categories.map((category) => ({
    value: category.slug,
    label: `${category.name} (${category.count})`,
  })),
])
const enabledMarkets = computed(() =>
  props.markets.filter((market) => market.enabled),
)
const sourceOptions = computed(() => [
  { value: '', label: t('marketplace.source.all') },
  ...enabledMarkets.value.map((market) => ({
    value: market.id,
    label: market.name,
  })),
])
</script>

<template>
  <div class="flex flex-wrap items-center gap-2">
    <UiInput
      v-model="search"
      type="search"
      :placeholder="t('marketplace.search.placeholder')"
      :aria-label="t('marketplace.search.label')"
      :labels="fieldLabels.input.value"
      :prepend-icon="Search"
      clearable
      class="min-w-48 flex-1 [&_input::-webkit-search-cancel-button]:hidden"
      data-testid="marketplace-search"
      @keydown.esc.prevent.stop="search = ''"
    />
    <SettingsSelect
      :model-value="category"
      :options="categoryOptions"
      :aria-label="t('marketplace.category.label')"
      class="w-44"
      data-testid="marketplace-category"
      @update:model-value="emit('category', $event)"
    />
    <SettingsSelect
      v-if="enabledMarkets.length > 1"
      :model-value="source"
      :options="sourceOptions"
      :aria-label="t('marketplace.source.label')"
      class="w-44"
      data-testid="marketplace-source-filter"
      @update:model-value="emit('source', $event)"
    />
    <UiButton
      variant="outline"
      size="icon"
      :aria-label="t('marketplace.refresh')"
      :tooltip="t('marketplace.refresh')"
      :disabled="loading"
      data-testid="marketplace-refresh"
      @click="emit('refresh')"
    >
      <Icon name="lucide:refresh-cw" class="size-4" />
    </UiButton>
  </div>
</template>
