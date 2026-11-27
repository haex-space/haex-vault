<script setup lang="ts">
/**
 * The address book frame (spec 051, FR-002), laid out like the password manager: the sidebar button
 * and the controls are in the window's header (spec 093), below it the sidebar with all contacts and
 * the groups, and the content.
 * The frame is a size container, so the sidebar follows the window's width: from `@2xl` it sits
 * beside the content and can be hidden, below it opens over the content (`useSidebarFrame`). Every
 * place is a tab location (spec 020); only ids go into the path.
 */
import { toast } from 'vue-sonner'

const { t } = useI18n()

const store = useContactsStore()
// A failed (quiet) reload of the store shows as a toast, once for all open contacts tabs.
watch(
  () => store.lastError,
  (message) => {
    if (message) toast.error(message, { id: 'contacts-load-failed' })
  },
)
onMounted(() => {
  void store.reloadAsync()
})

const root = useTemplateRef<HTMLElement>('root')
const sidebar = useTemplateRef<{ $el: HTMLElement }>('sidebar')

const { wideHidden, menuOpen, visible, overlaying, toggle, close } =
  useSidebarFrame(root, () => sidebar.value?.$el)

// The dialogs of the frame; the header, the sidebar and the right-click menus open them.
const actions = provideContactsActions()
const router = useTabRouter()
function afterGroupDelete(id: string) {
  if (router.route.path === `/group/${id}`) void router.push('/')
}

// The first row is the window's header (spec 093-app-headers, US2).
useContactsHeader(() => visible.value, toggle, actions)
</script>

<template>
  <div
    ref="root"
    class="@container flex h-full min-h-0 flex-col bg-muted/20"
    data-contacts-app
  >
    <div class="relative flex min-h-0 flex-1 overflow-hidden">
      <WmSidebar
        id="contacts-sidebar"
        ref="sidebar"
        :open="menuOpen"
        :wide-hidden="wideHidden"
        :aria-label="t('contacts.title')"
        data-testid="contacts-sidebar"
      >
        <ContactsSidebar @navigated="close" />
      </WmSidebar>
      <ContactsContextMenu>
        <main
          class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto"
          :inert="overlaying || undefined"
        >
          <WmRouterView />
        </main>
      </ContactsContextMenu>
    </div>
    <ContactsGroupDialog
      v-model:open="actions.groupDialogOpen.value"
      :group="actions.groupBeingEdited.value"
    />
    <ContactsGroupDeleteDialog
      :group="actions.groupBeingDeleted.value"
      @close="actions.groupBeingDeleted.value = null"
      @deleted="afterGroupDelete"
    />
    <ContactsDeleteDialog
      v-model:open="actions.deleteOpen.value"
      :ids="
        actions.contactBeingDeleted.value
          ? [actions.contactBeingDeleted.value.id]
          : []
      "
      :name="actions.contactBeingDeleted.value?.name"
    />
  </div>
</template>
