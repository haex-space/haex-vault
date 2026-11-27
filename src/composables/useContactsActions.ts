import { inject, provide, ref, type InjectionKey } from 'vue'
import type { ContactGroup } from '@bindings/ContactGroup'
import { NEW_CONTACT } from '~/lib/contacts/registry'

/** The contact a delete confirmation is open for. */
export type ContactsDeleteTarget = { id: string; name: string }

/**
 * What the address book can be asked to do from several places (the header's "+", the sidebar and
 * the right-click menus): its dialogs are rendered once by `ContactsApp.vue`, which provides this;
 * per window, so two address books never open each other's dialogs.
 */
function createContactsActions() {
  const router = useTabRouter()

  const groupDialogOpen = ref(false)
  const groupBeingEdited = ref<ContactGroup | null>(null)
  const groupBeingDeleted = ref<ContactGroup | null>(null)
  const deleteOpen = ref(false)
  const contactBeingDeleted = ref<ContactsDeleteTarget | null>(null)

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

  function editContact(id: string) {
    void router.push(`/entry/${id}/edit`)
  }

  function deleteContact(target: ContactsDeleteTarget) {
    contactBeingDeleted.value = target
    deleteOpen.value = true
  }

  function newGroup() {
    groupBeingEdited.value = null
    groupDialogOpen.value = true
  }

  function renameGroup(group: ContactGroup) {
    groupBeingEdited.value = group
    groupDialogOpen.value = true
  }

  function deleteGroup(group: ContactGroup) {
    groupBeingDeleted.value = group
  }

  return {
    groupDialogOpen,
    groupBeingEdited,
    groupBeingDeleted,
    deleteOpen,
    contactBeingDeleted,
    newContact,
    editContact,
    deleteContact,
    newGroup,
    renameGroup,
    deleteGroup,
  }
}

export type ContactsActions = ReturnType<typeof createContactsActions>

const CONTACTS_ACTIONS_KEY: InjectionKey<ContactsActions> =
  Symbol('contactsActions')

/** Called by `ContactsApp.vue`, once per window. */
export function provideContactsActions(): ContactsActions {
  const actions = createContactsActions()
  provide(CONTACTS_ACTIONS_KEY, actions)
  return actions
}

export function useContactsActions(): ContactsActions {
  const actions = inject(CONTACTS_ACTIONS_KEY)
  if (!actions)
    throw new Error('useContactsActions needs the address book frame')
  return actions
}
