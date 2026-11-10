<script setup lang="ts">
/**
 * The sidebar of the address book (spec 051, FR-002, FR-003): all contacts, the groups with their
 * counts, new group, rename and delete of a group, sending a whole group to a phone of the vault
 * (FR-018c), and the way to the settings of the address book. Only ids travel.
 */
import { toast } from 'vue-sonner'
import type { ContactGroup } from '@bindings/ContactGroup'
import type { ContactsPhoneSummary } from '@bindings/ContactsPhoneSummary'

const emit = defineEmits<{
  navigated: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useContactsStore()
const { groupTargetSetAsync } = useContactTargets()
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

async function sendAsync(group: ContactGroup, phone: ContactsPhoneSummary) {
  const name = phone.name ?? t('contacts.targets.unnamedPhone')
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // choice of the window calls the command.
    await groupTargetSetAsync(group.id, phone.device)
    await store.quietReloadAsync()
    const count = counts.value.get(group.id) ?? 0
    toast.success(t('contacts.targets.sent', { phone: name, count }, count))
  } catch (cause) {
    toast.error(errString(cause))
  }
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
        <SidebarRow
          icon="lucide:contact"
          :active="allActive"
          data-testid="contacts-all"
          @click="go('/')"
        >
          {{ t('contacts.sidebar.all') }}
          <template #trailing>
            <span class="text-xs text-muted-foreground">{{
              store.headers.length
            }}</span>
          </template>
        </SidebarRow>
      </li>
      <li
        v-for="group in store.groups"
        :key="group.id"
        class="group/row flex items-center"
      >
        <SidebarRow
          class="flex-1"
          icon="lucide:users"
          :active="activeGroup === group.id"
          :data-testid="`contacts-group-${group.id}`"
          @click="go(`/group/${group.id}`)"
        >
          {{ group.name }}
          <template #trailing>
            <span class="text-xs text-muted-foreground">{{
              counts.get(group.id) ?? 0
            }}</span>
          </template>
        </SidebarRow>
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
              v-for="phone in store.phones"
              :key="phone.device"
              class="gap-2"
              :data-testid="`contacts-group-send-${phone.device}`"
              @select="sendAsync(group, phone)"
            >
              <Icon name="lucide:smartphone" class="size-4" />
              {{
                t('contacts.groups.sendTo', {
                  phone: phone.name ?? t('contacts.targets.unnamedPhone'),
                })
              }}
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
        <SidebarRow
          class="text-muted-foreground"
          icon="lucide:plus"
          data-testid="contacts-new-group"
          @click="newGroup"
        >
          {{ t('contacts.groups.new') }}
        </SidebarRow>
      </li>
    </ul>

    <div class="mt-auto">
      <SidebarRow
        icon="lucide:settings-2"
        data-testid="contacts-settings"
        @click="openApp('system.settings', '/contacts')"
      >
        {{ t('contacts.sidebar.settings') }}
      </SidebarRow>
    </div>

    <ContactsGroupDialog v-model:open="groupDialog" :group="editing" />
    <ContactsGroupDeleteDialog
      :group="deleting"
      @close="deleting = null"
      @deleted="afterDelete"
    />
  </div>
</template>
