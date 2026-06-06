<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
    show-back
    @back="$emit('back')"
  >
    <template #actions>
      <UiButton
        :label="t('add')"
        icon="i-lucide-plus"
        color="primary"
        @click="onAdd"
      />
    </template>

    <div v-if="loading" class="flex justify-center py-8">
      <UIcon
        name="i-heroicons-arrow-path"
        class="w-8 h-8 animate-spin text-primary"
      />
    </div>

    <div v-else-if="!rows.length" class="text-center py-8 text-muted">
      {{ t('empty') }}
    </div>

    <div v-else class="space-y-2">
      <div
        v-for="row in rows"
        :key="row.id"
        class="p-4 rounded-lg border border-default bg-default"
      >
        <div class="flex items-center gap-3">
          <UIcon name="i-mdi-store" class="size-6 shrink-0 text-primary" />
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 flex-wrap">
              <span class="font-semibold truncate">{{ row.name }}</span>
              <UBadge v-if="row.isDefault" color="primary" variant="subtle" size="sm">
                {{ t('badge.default') }}
              </UBadge>
              <UBadge :color="authBadgeColor(row.authType)" variant="subtle" size="sm">
                {{ t(`authType.${row.authType}`) }}
              </UBadge>
              <UBadge v-if="!row.enabled" color="neutral" variant="subtle" size="sm">
                {{ t('badge.disabled') }}
              </UBadge>
            </div>
            <div class="text-sm text-muted truncate">{{ row.baseUrl }}</div>
          </div>

          <USwitch
            :model-value="row.enabled"
            :aria-label="t('aria.toggleEnabled')"
            @update:model-value="(v: boolean) => onToggleAsync(row, v)"
          />

          <UDropdownMenu :items="rowActions(row)">
            <UiButton icon="i-lucide-ellipsis-vertical" variant="ghost" color="neutral" />
          </UDropdownMenu>
        </div>
      </div>
    </div>

    <HaexSystemSettingsExtensionsMarketplaceEditDialog
      v-model:open="dialogOpen"
      :row="editingRow"
      @saved="reloadAsync"
    />

    <UModal v-model:open="confirmDeleteOpen" :title="t('delete.title')">
      <template #body>
        <p>
          {{ t('delete.description', { name: rowToDelete?.name ?? '' }) }}
        </p>
      </template>
      <template #footer>
        <div class="flex justify-end gap-2 w-full">
          <UiButton variant="ghost" :label="t('cancel')" @click="confirmDeleteOpen = false" />
          <UiButton color="error" :label="t('delete.confirm')" @click="onConfirmDeleteAsync" />
        </div>
      </template>
    </UModal>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
import {
  deleteMarketplaceAsync,
  loadAllMarketplacesAsync,
  setMarketplaceEnabledAsync,
} from '@/composables/useMarketplaces'
import type { SelectHaexMarketplaces } from '@/database/schemas/marketplaces'

defineEmits<{
  back: []
}>()

const { t } = useI18n()
const { add } = useToast()

const rows = ref<SelectHaexMarketplaces[]>([])
const loading = ref(true)

const dialogOpen = ref(false)
const editingRow = ref<SelectHaexMarketplaces | null>(null)

const confirmDeleteOpen = ref(false)
const rowToDelete = ref<SelectHaexMarketplaces | null>(null)

const reloadAsync = async () => {
  loading.value = true
  try {
    rows.value = await loadAllMarketplacesAsync()
  } catch (error) {
    add({ description: t('error.load', { msg: (error as Error).message }), color: 'error' })
  } finally {
    loading.value = false
  }
}

const onAdd = () => {
  editingRow.value = null
  dialogOpen.value = true
}

const onEdit = (row: SelectHaexMarketplaces) => {
  editingRow.value = row
  dialogOpen.value = true
}

const onToggleAsync = async (row: SelectHaexMarketplaces, enabled: boolean) => {
  try {
    await setMarketplaceEnabledAsync(row.id, enabled)
    await reloadAsync()
  } catch (error) {
    add({ description: t('error.toggle', { msg: (error as Error).message }), color: 'error' })
  }
}

const onDelete = (row: SelectHaexMarketplaces) => {
  rowToDelete.value = row
  confirmDeleteOpen.value = true
}

const onConfirmDeleteAsync = async () => {
  const row = rowToDelete.value
  if (!row) return
  try {
    await deleteMarketplaceAsync(row.id)
    add({ description: t('success.deleted'), color: 'success' })
    confirmDeleteOpen.value = false
    rowToDelete.value = null
    await reloadAsync()
  } catch (error) {
    add({ description: t('error.delete', { msg: (error as Error).message }), color: 'error' })
  }
}

const rowActions = (row: SelectHaexMarketplaces) => [[
  {
    label: t('action.edit'),
    icon: 'i-lucide-pencil',
    onSelect: () => onEdit(row),
  },
  {
    label: t('action.delete'),
    icon: 'i-lucide-trash-2',
    color: 'error' as const,
    disabled: row.isDefault,
    onSelect: () => onDelete(row),
  },
]]

const authBadgeColor = (authType: string) => {
  switch (authType) {
    case 'none': return 'neutral'
    case 'bearer': return 'info'
    case 'basic': return 'warning'
    case 'did': return 'success'
    default: return 'neutral'
  }
}

onMounted(() => { reloadAsync() })
</script>

<i18n lang="yaml">
de:
  title: Marketplaces verwalten
  description: Lege fest, welche Marketplaces für Erweiterungen genutzt werden. Mehrere Quellen werden gleichzeitig durchsucht und Treffer kombiniert.
  add: Marketplace hinzufügen
  empty: Keine Marketplaces konfiguriert.
  cancel: Abbrechen
  badge:
    default: Standard
    disabled: Deaktiviert
  authType:
    none: Keine Auth
    bearer: Bearer
    basic: Basic
    did: DID
  action:
    edit: Bearbeiten
    delete: Löschen
  aria:
    toggleEnabled: Aktiviert
  delete:
    title: Marketplace löschen?
    description: '"{name}" wird endgültig aus der Liste entfernt.'
    confirm: Löschen
  success:
    deleted: Marketplace gelöscht
  error:
    load: Laden fehlgeschlagen — {msg}
    toggle: Status ändern fehlgeschlagen — {msg}
    delete: Löschen fehlgeschlagen — {msg}
en:
  title: Manage marketplaces
  description: Choose which marketplaces are queried for extensions. Multiple sources are searched in parallel and results combined.
  add: Add marketplace
  empty: No marketplaces configured.
  cancel: Cancel
  badge:
    default: Default
    disabled: Disabled
  authType:
    none: No auth
    bearer: Bearer
    basic: Basic
    did: DID
  action:
    edit: Edit
    delete: Delete
  aria:
    toggleEnabled: Enabled
  delete:
    title: Delete marketplace?
    description: '"{name}" will be permanently removed.'
    confirm: Delete
  success:
    deleted: Marketplace deleted
  error:
    load: Load failed — {msg}
    toggle: Toggle failed — {msg}
    delete: Delete failed — {msg}
</i18n>
