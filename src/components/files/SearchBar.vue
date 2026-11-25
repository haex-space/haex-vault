<script setup lang="ts">
/**
 * The search field and the filter of the file browser (spec 044 FR-027 to FR-029): typing searches
 * from the open folder down, 200 ms after the last key; the filter (types, size, time) applies to
 * the open folder and to search hits. Both live in the tab's location; this row only edits them.
 */
import type { FileCategory } from '@bindings/FileCategory'
import {
  CATEGORIES,
  DATE_RANGES,
  type FilesFilter,
  isFiltering,
  NO_FILTER,
  SIZE_RANGES,
} from '~/lib/files/filters'

const props = defineProps<{ query: string; filter: FilesFilter }>()
const emit = defineEmits<{
  search: [query: string]
  filter: [filter: FilesFilter]
}>()

const { t } = useI18n()

const text = ref(props.query)
watch(
  () => props.query,
  (query) => {
    if (query !== text.value) text.value = query
  },
)
watchDebounced(text, (value) => emit('search', value), { debounce: 200 })

function clear() {
  text.value = ''
  emit('search', '')
}

function toggleType(type: FileCategory, on: boolean) {
  const types = on
    ? [...props.filter.types, type]
    : props.filter.types.filter((candidate) => candidate !== type)
  emit('filter', { ...props.filter, types })
}

function pickSize(picked: unknown) {
  const value = String(picked)
  emit('filter', {
    ...props.filter,
    size: value === 'any' ? null : (value as FilesFilter['size']),
  })
}

function pickDate(picked: unknown) {
  const value = String(picked)
  emit('filter', {
    ...props.filter,
    date: value === 'any' ? null : (value as FilesFilter['date']),
  })
}
</script>

<template>
  <div class="flex h-11 shrink-0 items-center gap-1 px-2">
    <UiInput
      v-model="text"
      class="min-w-0 flex-1"
      type="search"
      :placeholder="t('files.search.placeholder')"
      :aria-label="t('files.search.placeholder')"
      data-testid="files-search"
      @keydown.esc="clear"
    >
      <template #prepend>
        <ShadcnInputGroupAddon align="inline-start">
          <Icon name="lucide:search" class="size-4" />
        </ShadcnInputGroupAddon>
      </template>
      <template #append>
        <ShadcnInputGroupAddon v-if="text" align="inline-end">
          <UiButtonClose
            :label="t('files.search.clear')"
            size="icon-sm"
            class="size-6"
            data-testid="files-search-clear"
            @click="clear"
          />
        </ShadcnInputGroupAddon>
      </template>
    </UiInput>
    <ShadcnDropdownMenu>
      <ShadcnDropdownMenuTrigger as-child>
        <UiButtonIcon
          :label="t('files.filter.label')"
          :variant="isFiltering(filter) ? 'secondary' : 'ghost'"
          data-testid="files-filter"
          ><Icon name="lucide:list-filter" class="size-4"
        /></UiButtonIcon>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent align="end" class="min-w-52">
        <ShadcnDropdownMenuLabel>{{
          t('files.filter.type')
        }}</ShadcnDropdownMenuLabel>
        <ShadcnDropdownMenuCheckboxItem
          v-for="type in CATEGORIES"
          :key="type"
          :model-value="filter.types.includes(type)"
          :data-testid="`files-filter-type-${type}`"
          @update:model-value="(on: boolean) => toggleType(type, on)"
          @select.prevent
        >
          {{ t(`files.filter.types.${type}`) }}
        </ShadcnDropdownMenuCheckboxItem>
        <ShadcnDropdownMenuSeparator />
        <ShadcnDropdownMenuLabel>{{
          t('files.filter.size')
        }}</ShadcnDropdownMenuLabel>
        <ShadcnDropdownMenuRadioGroup
          :model-value="filter.size ?? 'any'"
          @update:model-value="pickSize"
        >
          <ShadcnDropdownMenuRadioItem
            v-for="size in ['any', ...SIZE_RANGES]"
            :key="size"
            :value="size"
            :data-testid="`files-filter-size-${size}`"
            @select.prevent
          >
            {{ t(`files.filter.sizes.${size}`) }}
          </ShadcnDropdownMenuRadioItem>
        </ShadcnDropdownMenuRadioGroup>
        <ShadcnDropdownMenuSeparator />
        <ShadcnDropdownMenuLabel>{{
          t('files.filter.date')
        }}</ShadcnDropdownMenuLabel>
        <ShadcnDropdownMenuRadioGroup
          :model-value="filter.date ?? 'any'"
          @update:model-value="pickDate"
        >
          <ShadcnDropdownMenuRadioItem
            v-for="date in ['any', ...DATE_RANGES]"
            :key="date"
            :value="date"
            :data-testid="`files-filter-date-${date}`"
            @select.prevent
          >
            {{ t(`files.filter.dates.${date}`) }}
          </ShadcnDropdownMenuRadioItem>
        </ShadcnDropdownMenuRadioGroup>
        <template v-if="isFiltering(filter)">
          <ShadcnDropdownMenuSeparator />
          <ShadcnDropdownMenuItem
            data-testid="files-filter-clear"
            @select="emit('filter', NO_FILTER)"
          >
            {{ t('files.filter.clear') }}
          </ShadcnDropdownMenuItem>
        </template>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
  </div>
</template>
