<script setup lang="ts">
/**
 * The first row of the address book (spec 051, FR-002, FR-004): the sidebar button, the search
 * field bound to `?q=` of the place, and "+", a menu with a new contact and reading a Visitenkarte in. While a contact is open the search
 * leaves the row.
 */
import { Search } from '@lucide/vue'
import { NEW_CONTACT } from '~/lib/contacts/registry'

const props = defineProps<{
  sidebarVisible: boolean
}>()

const emit = defineEmits<{
  toggleSidebar: []
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const router = useTabRouter()

/** A contact or the reading of a share is open: no search there. */
const entryOpen = computed(
  () =>
    router.route.path.startsWith('/entry/') ||
    router.route.path.startsWith('/import'),
)

const query = ref(router.route.query.q ?? '')
watch(
  () => router.route.query.q ?? '',
  (value) => {
    if (value !== query.value) query.value = value
  },
)

function onUpdate(value: string | number | null | undefined) {
  query.value = String(value ?? '')
  router.setQuery({ q: query.value.trim() ? query.value : null })
}

function clear() {
  onUpdate('')
}

const sidebarLabel = computed(() =>
  props.sidebarVisible
    ? t('contacts.sidebar.hide')
    : t('contacts.sidebar.show'),
)

/** A new contact starts in the group the list shows. */
function newContact() {
  const group = router.route.path.startsWith('/group/')
    ? router.route.params.id
    : undefined
  void router.push(
    group
      ? `/entry/${NEW_CONTACT}/edit?group=${encodeURIComponent(group)}`
      : `/entry/${NEW_CONTACT}/edit`,
  )
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
      aria-controls="contacts-sidebar"
      data-testid="contacts-sidebar-toggle"
      @click="emit('toggleSidebar')"
    >
      <Icon name="lucide:panel-left" class="size-4" />
    </UiButton>
    <div v-if="entryOpen" class="flex-1" />
    <UiInput
      v-else
      :model-value="query"
      type="search"
      :placeholder="t('contacts.search.placeholder')"
      :aria-label="t('contacts.search.label')"
      :labels="fieldLabels.input.value"
      :prepend-icon="Search"
      clearable
      class="min-w-0 flex-1 [&_input::-webkit-search-cancel-button]:hidden"
      data-testid="contacts-search"
      @update:model-value="onUpdate"
      @keydown.esc.prevent.stop="clear"
    />
    <ShadcnDropdownMenu v-if="!entryOpen">
      <ShadcnDropdownMenuTrigger as-child>
        <UiButton
          size="icon"
          class="shrink-0"
          :aria-label="t('contacts.add')"
          :tooltip="t('contacts.add')"
          data-testid="contacts-add"
        >
          <Icon name="lucide:plus" class="size-4" />
        </UiButton>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent align="end">
        <ShadcnDropdownMenuItem
          class="gap-2"
          data-testid="contacts-new"
          @select="newContact"
        >
          <Icon name="lucide:user-plus" class="size-4" />
          {{ t('contacts.new') }}
        </ShadcnDropdownMenuItem>
        <ShadcnDropdownMenuItem
          class="gap-2"
          data-testid="contacts-import-menu"
          @select="router.push('/import')"
        >
          <Icon name="lucide:scan-qr-code" class="size-4" />
          {{ t('contacts.import.menu') }}
        </ShadcnDropdownMenuItem>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
  </div>
</template>
