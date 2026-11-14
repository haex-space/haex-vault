import { contactsHeader, contactsHeaderCommand } from '~/lib/contacts/header'
import { NEW_CONTACT } from '~/lib/contacts/registry'

/**
 * The first row of the address book is its window's header (spec 093-app-headers, US2; the row of
 * spec 051, FR-002 and FR-004 before). The sidebar button, the search bound to `?q=` of the place
 * and "+" (a new contact, reading a Visitenkarte in) come from the description of
 * `lib/contacts/header.ts`; this holds the state it is derived from and what its events do.
 */
export function useContactsHeader(
  sidebarVisible: () => boolean,
  toggleSidebar: () => void,
) {
  const { t } = useI18n()
  const router = useTabRouter()

  /** A contact or the reading of a share is open: no search there. */
  const entryOpen = computed(
    () =>
      router.route.path.startsWith('/entry/') ||
      router.route.path.startsWith('/import'),
  )

  const query = ref(router.route.query.q ?? '')
  // A history step to another place or another search puts its own text into the field.
  watch(
    () => router.route.query.q ?? '',
    (value) => {
      if (value !== query.value) query.value = value
    },
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

  useWmHeader(
    () =>
      contactsHeader(
        {
          sidebarVisible: sidebarVisible(),
          entryOpen: entryOpen.value,
          query: query.value,
        },
        (key) => t(key),
      ),
    (event) => {
      const command = contactsHeaderCommand(event)
      if (!command) return
      switch (command.kind) {
        case 'sidebar':
          toggleSidebar()
          break
        case 'search':
          query.value = command.text
          router.setQuery({ q: command.text.trim() ? command.text : null })
          break
        case 'newContact':
          newContact()
          break
        case 'import':
          void router.push('/import')
          break
      }
    },
  )
}
