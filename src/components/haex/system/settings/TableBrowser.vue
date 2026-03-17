<template>
  <HaexSystemSettingsLayout
    :title="tableName"
    :description="`${total} ${t('rows')}`"
    show-back
    sticky-header
    @back="$emit('back')"
  >
    <!-- Loading -->
    <div
      v-if="isLoading"
      class="flex justify-center py-16"
    >
      <UIcon
        name="i-lucide-loader-2"
        class="w-8 h-8 animate-spin text-muted"
      />
    </div>

    <!-- Table -->
    <div
      v-else-if="columns.length > 0"
      class="space-y-4"
    >
      <div class="overflow-x-auto rounded-lg border border-default">
        <table class="w-full text-xs font-mono">
          <thead>
            <!-- Column headers (sortable) -->
            <tr class="bg-muted/30">
              <th
                v-for="col in columns"
                :key="col"
                class="text-left px-3 py-2 text-muted font-medium whitespace-nowrap border-b border-default cursor-pointer hover:text-highlighted select-none transition-colors"
                @click="toggleSort(col)"
              >
                <div class="flex items-center gap-1">
                  {{ col }}
                  <UIcon
                    v-if="sortColumn === col"
                    :name="sortDirection === 'ASC' ? 'i-lucide-arrow-up' : 'i-lucide-arrow-down'"
                    class="w-3 h-3"
                  />
                </div>
              </th>
            </tr>
            <!-- Column filters -->
            <tr class="bg-muted/10">
              <td
                v-for="col in columns"
                :key="`filter-${col}`"
                class="px-1 py-1 border-b border-default"
              >
                <input
                  v-model="columnFilters[col]"
                  :placeholder="t('filter')"
                  class="w-full bg-transparent text-xs px-2 py-1 rounded border border-transparent focus:border-primary/50 outline-none placeholder:text-muted/50"
                  @input="onFilterDebounced"
                >
              </td>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="(row, i) in rows"
              :key="i"
              :class="[
                'border-b border-default last:border-0 hover:bg-muted/20 transition-colors',
                isTombstone(row) ? 'bg-red-500/5 text-red-400' : isModified(row) && 'bg-info/5',
              ]"
            >
              <td
                v-for="(cell, j) in row"
                :key="j"
                class="px-3 py-1.5 whitespace-nowrap max-w-80 truncate"
                :title="String(cell)"
              >
                <span
                  v-if="cell === null"
                  class="text-muted italic"
                >NULL</span>
                <span
                  v-else-if="typeof cell === 'number'"
                  class="text-info"
                >{{ cell }}</span>
                <span v-else>{{ cell }}</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Pagination + Reset -->
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <span class="text-sm text-muted">
            <template v-if="total > 0">
              {{ offset + 1 }}–{{ Math.min(offset + pageSize, total) }} / {{ total }}
            </template>
            <template v-else>
              {{ t('noResults') }}
            </template>
          </span>
          <UiButton
            v-if="hasActiveFilters"
            icon="i-lucide-x"
            variant="ghost"
            color="neutral"
            @click="resetFilters"
          >
            {{ t('resetFilters') }}
          </UiButton>
        </div>
        <div
          v-if="total > pageSize"
          class="flex gap-2"
        >
          <UiButton
            icon="i-lucide-chevron-left"
            variant="ghost"
            :disabled="offset === 0"
            @click="offset -= pageSize; loadData()"
          />
          <UiButton
            icon="i-lucide-chevron-right"
            variant="ghost"
            :disabled="offset + pageSize >= total"
            @click="offset += pageSize; loadData()"
          />
        </div>
      </div>
    </div>

    <!-- Empty -->
    <div
      v-else
      class="text-center py-16 text-muted"
    >
      <UIcon
        name="i-lucide-database"
        class="w-12 h-12 mx-auto mb-2 opacity-30"
      />
      <p>{{ t('empty') }}</p>
    </div>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { useDebounceFn } from '@vueuse/core'

const props = defineProps<{
  tableName: string
}>()

defineEmits<{
  back: []
}>()

const { t } = useI18n()

const isLoading = ref(true)
const columns = ref<string[]>([])
const rows = ref<unknown[][]>([])
const total = ref(0)
const offset = ref(0)
const pageSize = 50

// Sort
const sortColumn = ref<string | null>(null)
const sortDirection = ref<'ASC' | 'DESC'>('ASC')

// Column filters
const columnFilters = ref<Record<string, string>>({})

const hasActiveFilters = computed(() =>
  Object.values(columnFilters.value).some(v => v !== ''),
)

const tombstoneColIndex = computed(() => columns.value.indexOf('haex_tombstone'))
const hlcColIndex = computed(() => columns.value.indexOf('haex_column_hlcs'))

const isTombstone = (row: unknown[]) => {
  const idx = tombstoneColIndex.value
  if (idx === -1) return false
  return row[idx] === 1 || row[idx] === '1'
}

const isModified = (row: unknown[]) => {
  const idx = hlcColIndex.value
  if (idx === -1) return false
  const val = row[idx]
  return val !== null && val !== '' && val !== '{}'
}

const toggleSort = (col: string) => {
  if (sortColumn.value === col) {
    sortDirection.value = sortDirection.value === 'ASC' ? 'DESC' : 'ASC'
  } else {
    sortColumn.value = col
    sortDirection.value = 'ASC'
  }
  offset.value = 0
  loadData()
}

const resetFilters = () => {
  columnFilters.value = {}
  sortColumn.value = null
  offset.value = 0
  loadData()
}

const onFilterDebounced = useDebounceFn(() => {
  offset.value = 0
  loadData()
}, 300)

const buildWhereClause = (): { clause: string; params: unknown[] } => {
  const conditions: string[] = []
  const params: unknown[] = []

  // Per-column filters
  for (const col of columns.value) {
    const val = columnFilters.value[col]?.trim()
    if (val) {
      conditions.push(`CAST("${col}" AS TEXT) LIKE ?`)
      params.push(`%${val}%`)
    }
  }

  const clause = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : ''
  return { clause, params }
}

const buildOrderClause = (): string => {
  if (!sortColumn.value) return ''
  return `ORDER BY "${sortColumn.value}" ${sortDirection.value}`
}

const loadData = async () => {
  isLoading.value = true

  try {
    // Load columns first (needed for search clause)
    if (columns.value.length === 0) {
      const colResult = await invoke<unknown[][]>('sql_select', {
        sql: `SELECT name FROM pragma_table_info("${props.tableName}") ORDER BY cid`,
        params: [],
      })
      columns.value = colResult.map(row => String(row[0]))
    }

    const { clause: where, params: whereParams } = buildWhereClause()
    const order = buildOrderClause()

    const [countResult, dataResult] = await Promise.all([
      invoke<unknown[][]>('sql_select', {
        sql: `SELECT COUNT(*) FROM "${props.tableName}" ${where}`,
        params: whereParams,
      }),
      invoke<unknown[][]>('sql_select', {
        sql: `SELECT * FROM "${props.tableName}" ${where} ${order} LIMIT ${pageSize} OFFSET ${offset.value}`,
        params: whereParams,
      }),
    ])

    total.value = Number(countResult[0]?.[0] ?? 0)
    rows.value = dataResult
  } catch (error) {
    console.error('Failed to load table data:', error)
  } finally {
    isLoading.value = false
  }
}

onMounted(() => loadData())
</script>

<i18n lang="yaml">
de:
  rows: Einträge
  empty: Keine Einträge in dieser Tabelle
  search: Suche in allen Spalten...
  filter: Filtern...
  resetFilters: Filter zurücksetzen
  noResults: Keine Ergebnisse
en:
  rows: rows
  empty: No entries in this table
  search: Search all columns...
  filter: Filter...
  resetFilters: Reset filters
  noResults: No results
</i18n>
