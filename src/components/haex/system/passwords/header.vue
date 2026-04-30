<template>
  <div class="bg-elevated/50 border-b border-default px-3 py-2 backdrop-blur-md">
    <div class="flex items-center gap-2">
      <UiInput
        v-model="searchInput"
        :placeholder="t('search')"
        leading-icon="i-lucide-search"
        clearable
        class="flex-1"
        @keydown="onSearchKeydown"
      />

      <UDropdownMenu
        :items="addMenuItems"
        :content="{ align: 'end' }"
      >
        <UButton
          :aria-label="t('add')"
          icon="i-lucide-plus"
          color="primary"
          variant="solid"
          class="shrink-0"
        />
      </UDropdownMenu>

      <!-- Sort (Stage 3): name / created / modified -->
      <UiButton
        :tooltip="t('sort')"
        icon="i-lucide-arrow-up-down"
        color="neutral"
        variant="outline"
        class="shrink-0"
        disabled
      />

      <UDropdownMenu
        :items="moreMenuItems"
        :content="{ align: 'end' }"
      >
        <UButton
          :aria-label="t('more')"
          icon="i-lucide-more-vertical"
          color="neutral"
          variant="outline"
          class="shrink-0"
        />
      </UDropdownMenu>
    </div>

    <HaexSystemPasswordsDialogTagManager v-model:open="tagManagerOpen" />

    <HaexSystemPasswordsDialogGroupEditor
      v-model:open="groupEditorOpen"
      mode="create"
      :group="null"
      :create-parent-id="createParentId"
    />

    <HaexSystemPasswordsImportBitwarden v-model:open="importBitwardenOpen" />
    <HaexSystemPasswordsImportLastpass v-model:open="importLastpassOpen" />
    <HaexSystemPasswordsImportKeepass v-model:open="importKeepassOpen" />
  </div>
</template>

<script setup lang="ts">
import type { DropdownMenuItem } from '@nuxt/ui'

const { searchInput } = storeToRefs(usePasswordsSearchStore())
const { selectedGroupId } = storeToRefs(usePasswordsGroupsStore())
const nav = usePasswordsNavigation()
const { t } = useI18n()

const groupEditorOpen = ref(false)
const createParentId = ref<string | null>(null)

const openCreateGroupDialog = () => {
  // New folder inherits the currently-viewed group as parent (root when in
  // "All Passwords"). Consistent with what users expect from a file manager.
  createParentId.value = selectedGroupId.value
  groupEditorOpen.value = true
}

const addMenuItems = computed<DropdownMenuItem[][]>(() => [
  [
    {
      label: t('addMenu.item'),
      icon: 'i-lucide-key',
      onSelect: () => nav.startCreate(),
    },
    {
      label: t('addMenu.folder'),
      icon: 'i-lucide-folder',
      onSelect: openCreateGroupDialog,
    },
  ],
])

const tagManagerOpen = ref(false)
const importBitwardenOpen = ref(false)
const importLastpassOpen = ref(false)
const importKeepassOpen = ref(false)

const moreMenuItems = computed<DropdownMenuItem[][]>(() => [
  [
    {
      label: t('moreMenu.tags'),
      icon: 'i-lucide-tag',
      onSelect: () => { tagManagerOpen.value = true },
    },
  ],
  [
    {
      label: t('moreMenu.importBitwarden'),
      icon: 'i-simple-icons-bitwarden',
      onSelect: () => { importBitwardenOpen.value = true },
    },
    {
      label: t('moreMenu.importLastpass'),
      icon: 'i-simple-icons-lastpass',
      onSelect: () => { importLastpassOpen.value = true },
    },
    {
      label: t('moreMenu.importKeepass'),
      icon: 'i-simple-icons-keepassxc',
      onSelect: () => { importKeepassOpen.value = true },
    },
  ],
])

// Keep Ctrl+A scoped to the input — the layout-level shortcut will select all items in Stage 3.
const onSearchKeydown = (event: KeyboardEvent) => {
  if (event.key === 'a' && (event.ctrlKey || event.metaKey)) {
    event.stopPropagation()
  }
}
</script>

<i18n lang="yaml">
de:
  search: Suchen…
  add: Hinzufügen
  sort: Sortieren
  more: Mehr
  addMenu:
    item: Passwort anlegen
    folder: Ordner anlegen
  moreMenu:
    tags: Tags verwalten
    importBitwarden: Import von Bitwarden
    importLastpass: Import von LastPass
    importKeepass: Import von KeePass
en:
  search: Search…
  add: Add
  sort: Sort
  more: More
  addMenu:
    item: New password
    folder: New folder
  moreMenu:
    tags: Manage tags
    importBitwarden: Import from Bitwarden
    importLastpass: Import from LastPass
    importKeepass: Import from KeePass
</i18n>
