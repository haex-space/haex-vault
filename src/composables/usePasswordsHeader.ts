import { passwordsHeader, passwordsHeaderCommand } from '~/lib/passwords/header'

/**
 * The first row of the password manager is its window's header (spec 093-app-headers, US1; the row
 * of spec 034 FR-007, FR-039 before). The sidebar button, the search bound to `?q=` of the place, the
 * tag filter, "+" (an entry or a folder, in the open folder) and the menu come from the description
 * of `lib/passwords/header.ts`; this holds the state it is derived from and what its events do. The
 * search text is the only free text a place carries; it is never a secret (the search looks at
 * title, username, URL and tag names only).
 *
 * The dialogs the controls open are not here: their flags are returned so the frame mounts them
 * itself and they stay open while their control is collapsed into the overflow panel (FR-009).
 */
export function usePasswordsHeader(
  sidebarVisible: () => boolean,
  toggleSidebar: () => void,
) {
  const { t } = useI18n()
  const router = useTabRouter()
  const store = usePasswordsStore()

  const entryOpen = computed(() => router.route.path.startsWith('/entry/'))

  const query = ref(router.route.query.q ?? '')
  // A history step to another place or another search puts its own text into the field.
  watch(
    () => router.route.query.q ?? '',
    (value) => {
      if (value !== query.value) query.value = value
    },
  )

  /** The tag that filters the list: the store merges equal tags, so `?tag=` is matched over `ids`. */
  const activeTagId = computed(() => {
    const id = router.route.query.tag
    if (!id) return null
    return store.loginTags.find((tag) => tag.ids.includes(id))?.id ?? null
  })

  const settingsOpen = ref(false)
  const tagManagerOpen = ref(false)
  const folderDialogOpen = ref(false)
  /** A new folder goes into the folder the list shows, else to the top level. */
  const openFolderId = computed(() =>
    router.route.path.startsWith('/folder/')
      ? (router.route.params.id ?? null)
      : null,
  )

  useWmHeader(
    () =>
      passwordsHeader(
        {
          sidebarVisible: sidebarVisible(),
          entryOpen: entryOpen.value,
          query: query.value,
          tags: store.loginTags,
          activeTagId: activeTagId.value,
        },
        (key) => t(key),
      ),
    (event) => {
      const command = passwordsHeaderCommand(event)
      if (!command) return
      switch (command.kind) {
        case 'sidebar':
          toggleSidebar()
          break
        case 'search':
          query.value = command.text
          router.setQuery({ q: command.text.trim() ? command.text : null })
          break
        case 'tag':
          void router.push(command.id === null ? '/' : `/?tag=${command.id}`)
          break
        case 'newEntry':
          void router.push('/entry/new')
          break
        case 'newFolder':
          folderDialogOpen.value = true
          break
        case 'generator':
          void router.push('/generator')
          break
        case 'import':
          void router.push('/import')
          break
        case 'tags':
          tagManagerOpen.value = true
          break
        case 'settings':
          settingsOpen.value = true
          break
      }
    },
  )

  return { settingsOpen, tagManagerOpen, folderDialogOpen, openFolderId }
}
