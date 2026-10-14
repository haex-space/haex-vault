<script setup lang="ts">
/**
 * The sidebar of the address book (spec 051, FR-002, FR-003): all contacts, the groups with their
 * counts, new group, rename and delete of a group, and the way to the settings of the address
 * book. Only ids travel.
 */
import type { ContactGroup } from '@bindings/ContactGroup'

const emit = defineEmits<{
  navigated: []
}>()

const { t } = useI18n()
const router = useTabRouter()
const store = useContactsStore()
const { openApp } = useWmTab()

const activeGroup = computed(() =>
  router.route.path.startsWith('/group/') ? router.route.params.id : null,
)
const allActive = computed(() => router.route.path === '/')

const counts = computed(() => {
  const byGroup = new Map<string, number>()
  for (const header of store.headers) {
    for (const group of header.groups) {
      byGroup.set(group, (byGroup.get(group) ?? 0) + 1)
    }
  }
  return byGroup
})

const groupDialog = ref(false)
const editing = ref<ContactGroup | null>(null)
const deleting = ref<ContactGroup | null>(null)

function go(path: string) {
  void router.push(path)
  emit('navigated')
}

function newGroup() {
  editing.value = null
  groupDialog.value = true
}

function rename(group: ContactGroup) {
  editing.value = group
  groupDialog.value = true
}

function afterDelete(id: string) {
  if (activeGroup.value === id) go('/')
}
</script>

<template>
  <div
    class="flex h-full min-h-0 flex-col gap-3 overflow-y-auto p-2"
    data-testid="contacts-sidebar-content"
  >
    <ul class="flex flex-col gap-0.5">
      <li>
        <button
          type="button"
          class="flex min-h-9 w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left hover:bg-foreground/5"
          :class="allActive ? 'bg-foreground/10 font-medium' : ''"
          data-testid="contacts-all"
          @click="go('/')"
        >
          <Icon name="lucide:contact" class="size-4 shrink-0" />
          <span class="min-w-0 flex-1 truncate">{{
            t('contacts.sidebar.all')
          }}</span>
          <span class="text-xs text-muted-foreground">{{
            store.headers.length
          }}</span>
        </button>
      </li>
      <li
        v-for="group in store.groups"
        :key="group.id"
        class="group/row flex items-center"
      >
        <button
          type="button"
          class="flex min-h-9 min-w-0 flex-1 items-center gap-2 rounded-lg px-3 py-1.5 text-left hover:bg-foreground/5"
          :class="
            activeGroup === group.id ? 'bg-foreground/10 font-medium' : ''
          "
          :data-testid="`contacts-group-${group.id}`"
          @click="go(`/group/${group.id}`)"
        >
          <Icon name="lucide:users" class="size-4 shrink-0" />
          <span class="min-w-0 flex-1 truncate">{{ group.name }}</span>
          <span class="text-xs text-muted-foreground">{{
            counts.get(group.id) ?? 0
          }}</span>
        </button>
        <ShadcnDropdownMenu>
          <ShadcnDropdownMenuTrigger as-child>
            <UiButton
              variant="ghost"
              size="icon"
              class="size-8 shrink-0"
              :aria-label="t('contacts.groups.menu', { name: group.name })"
            >
              <Icon name="lucide:ellipsis-vertical" class="size-4" />
            </UiButton>
          </ShadcnDropdownMenuTrigger>
          <ShadcnDropdownMenuContent align="end">
            <ShadcnDropdownMenuItem class="gap-2" @select="rename(group)">
              <Icon name="lucide:pencil" class="size-4" />
              {{ t('contacts.groups.rename') }}
            </ShadcnDropdownMenuItem>
            <ShadcnDropdownMenuItem
              class="gap-2 text-destructive"
              @select="deleting = group"
            >
              <Icon name="lucide:trash-2" class="size-4" />
              {{ t('contacts.groups.delete') }}
            </ShadcnDropdownMenuItem>
          </ShadcnDropdownMenuContent>
        </ShadcnDropdownMenu>
      </li>
      <li>
        <button
          type="button"
          class="flex min-h-9 w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left text-muted-foreground hover:bg-foreground/5"
          data-testid="contacts-new-group"
          @click="newGroup"
        >
          <Icon name="lucide:plus" class="size-4 shrink-0" />
          <span>{{ t('contacts.groups.new') }}</span>
        </button>
      </li>
    </ul>

    <div class="mt-auto">
      <button
        type="button"
        class="flex min-h-9 w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left hover:bg-foreground/5"
        data-testid="contacts-settings"
        @click="openApp('system.settings', '/contacts')"
      >
        <Icon name="lucide:settings-2" class="size-4 shrink-0" />
        <span>{{ t('contacts.sidebar.settings') }}</span>
      </button>
    </div>

    <ContactsGroupDialog v-model:open="groupDialog" :group="editing" />
    <ContactsGroupDeleteDialog
      :group="deleting"
      @close="deleting = null"
      @deleted="afterDelete"
    />
  </div>
</template>
