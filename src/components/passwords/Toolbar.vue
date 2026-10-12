<script setup lang="ts">
/**
 * The first row of the password manager frame (spec 034, FR-007, FR-039): the sidebar button, the
 * search field bound to `?q=` of the place with the tag filter beside it, "+", which creates an
 * entry or a folder (in the open folder), and the menu with generator, import, tags and settings.
 * The search text is the only free text a place carries; it is never a secret (the search looks
 * at title, username, URL and tag names only). While an entry is open, search, tag filter and "+"
 * leave the row (spec 036, FR-010a); the entry's breadcrumbs stand above it.
 */
import { Search } from '@lucide/vue'

const props = defineProps<{
  sidebarVisible: boolean
}>()

const emit = defineEmits<{
  toggleSidebar: []
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const router = useTabRouter()

/** An entry is open: its view, its editor or a new one. */
const entryOpen = computed(() => router.route.path.startsWith('/entry/'))

const query = ref(router.route.query.q ?? '')
// A history step to another place or another search puts its own text into the field.
watch(
  () => router.route.query.q ?? '',
  (value) => {
    if (value !== query.value) query.value = value
  },
)

function onUpdate(value: string | number | null | undefined) {
  query.value = String(value ?? '')
  onInput()
}

function onInput() {
  router.setQuery({ q: query.value.trim() ? query.value : null })
}

function clear() {
  query.value = ''
  onInput()
}

const sidebarLabel = computed(() =>
  props.sidebarVisible
    ? t('passwords.sidebar.hide')
    : t('passwords.sidebar.show'),
)

const settingsOpen = ref(false)
const tagManager = ref(false)

const folderDialog = ref(false)
/** A new folder goes into the folder the list shows, else to the top level. */
const openFolderId = computed(() =>
  router.route.path.startsWith('/folder/')
    ? (router.route.params.id ?? null)
    : null,
)

// After "Neu → Eintrag" the editor has the focus (or gets it with the first click); the menu would
// hand it back to its button as it closes, taking it from the field just clicked.
let toEditor = false

function newEntry() {
  toEditor = true
  void router.push('/entry/new')
}

function keepEditorFocus(event: Event) {
  if (toEditor) event.preventDefault()
  toEditor = false
}
</script>

<template>
  <div class="flex items-center gap-1">
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="sidebarLabel"
      :tooltip="sidebarLabel"
      :aria-expanded="sidebarVisible"
      aria-controls="passwords-sidebar"
      data-testid="passwords-sidebar-toggle"
      @click="emit('toggleSidebar')"
    >
      <Icon name="lucide:panel-left" class="size-4" />
    </UiButton>
    <div v-if="entryOpen" class="flex-1" />
    <UiInput
      v-else
      :model-value="query"
      type="search"
      :placeholder="t('passwords.search.placeholder')"
      :aria-label="t('passwords.search.label')"
      :labels="fieldLabels.input.value"
      :prepend-icon="Search"
      clearable
      class="min-w-0 flex-1 [&_input::-webkit-search-cancel-button]:hidden"
      data-testid="passwords-search"
      @update:model-value="onUpdate"
      @keydown.esc.prevent.stop="clear"
    />
    <PasswordsTagFilter v-if="!entryOpen" />
    <div class="flex items-center gap-1">
      <ShadcnDropdownMenu v-if="!entryOpen">
        <ShadcnDropdownMenuTrigger as-child>
          <UiButton
            size="icon"
            class="shrink-0"
            :aria-label="t('passwords.new')"
            :tooltip="t('passwords.new')"
            data-testid="passwords-new"
          >
            <Icon name="lucide:plus" class="size-4" />
          </UiButton>
        </ShadcnDropdownMenuTrigger>
        <ShadcnDropdownMenuContent
          align="end"
          @close-auto-focus="keepEditorFocus"
        >
          <ShadcnDropdownMenuItem
            class="gap-2"
            data-testid="passwords-new-entry"
            @select="newEntry"
          >
            <Icon name="lucide:key-round" class="size-4" />
            {{ t('passwords.newMenu.entry') }}
          </ShadcnDropdownMenuItem>
          <ShadcnDropdownMenuItem
            class="gap-2"
            data-testid="passwords-new-folder-menu"
            @select="folderDialog = true"
          >
            <Icon name="lucide:folder-plus" class="size-4" />
            {{ t('passwords.newMenu.folder') }}
          </ShadcnDropdownMenuItem>
        </ShadcnDropdownMenuContent>
      </ShadcnDropdownMenu>
      <ShadcnDropdownMenu>
        <ShadcnDropdownMenuTrigger as-child>
          <UiButton
            variant="ghost"
            size="icon"
            class="shrink-0"
            :aria-label="t('passwords.menu')"
            :tooltip="t('passwords.menu')"
            data-testid="passwords-menu"
          >
            <Icon name="lucide:menu" class="size-4" />
          </UiButton>
        </ShadcnDropdownMenuTrigger>
        <ShadcnDropdownMenuContent align="end">
          <ShadcnDropdownMenuItem
            class="gap-2"
            data-testid="passwords-open-generator"
            @select="router.push('/generator')"
          >
            <Icon name="lucide:dices" class="size-4" />
            {{ t('passwords.generator.title') }}
          </ShadcnDropdownMenuItem>
          <ShadcnDropdownMenuItem
            class="gap-2"
            data-testid="passwords-open-import"
            @select="router.push('/import')"
          >
            <Icon name="lucide:file-down" class="size-4" />
            {{ t('passwords.import.open') }}
          </ShadcnDropdownMenuItem>
          <ShadcnDropdownMenuItem
            class="gap-2"
            data-testid="passwords-manage-tags"
            @select="tagManager = true"
          >
            <Icon name="lucide:tags" class="size-4" />
            {{ t('passwords.tags.manage') }}
          </ShadcnDropdownMenuItem>
          <ShadcnDropdownMenuSeparator />
          <ShadcnDropdownMenuItem
            class="gap-2"
            data-testid="passwords-settings"
            @select="settingsOpen = true"
          >
            <Icon name="lucide:settings-2" class="size-4" />
            {{ t('passwords.settings.title') }}
          </ShadcnDropdownMenuItem>
        </ShadcnDropdownMenuContent>
      </ShadcnDropdownMenu>
    </div>
    <PasswordsClipboardSetting v-model:open="settingsOpen" />
    <PasswordsTagManager v-model:open="tagManager" />
    <PasswordsFolderDialog
      v-model:open="folderDialog"
      :group="null"
      :parent-id="openFolderId"
    />
  </div>
</template>
