<template>
  <div
    class="flex flex-col @lg:flex-row items-stretch @lg:items-center gap-4 p-6 border-b border-gray-200 dark:border-gray-800"
  >
    <UInput
      v-model="searchModel"
      :placeholder="t('search.placeholder')"
      icon="i-heroicons-magnifying-glass"
      class="flex-1"
    />
    <USelectMenu
      v-model="categoryModel"
      :items="categoryItems"
      :placeholder="t('filter.category')"
      value-key="id"
      class="w-full @lg:w-48"
    >
      <template #leading>
        <UIcon name="i-heroicons-tag" />
      </template>
    </USelectMenu>
  </div>
</template>

<script setup lang="ts">
interface CategoryItem {
  id: string | null
  label: string
}

const props = defineProps<{
  searchQuery: string
  selectedCategory: string | null
  categoryItems: CategoryItem[]
}>()

const emit = defineEmits<{
  'update:searchQuery': [value: string]
  'update:selectedCategory': [value: string | null]
}>()

const { t } = useI18n()

const searchModel = computed({
  get: () => props.searchQuery,
  set: (value: string) => emit('update:searchQuery', value),
})

const categoryModel = computed({
  get: () => props.selectedCategory,
  set: (value: string | null) => emit('update:selectedCategory', value),
})
</script>
