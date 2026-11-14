import { PASS_KINDS } from '~/lib/passwords/passType'
import { walletHeader, walletHeaderCommand } from '~/lib/wallet/header'
import { walletNewPassLocation } from '~/lib/wallet/registry'

/**
 * The first row of Wallet is its window's header (spec 093-app-headers, US3; the row of spec 092,
 * Phase W before). The sidebar button, the search bound to `?q=` of the place, the list filters,
 * "Scan", "Import" and "New" come from the description of `lib/wallet/header.ts`; this holds the
 * state it is derived from and what its events do. The search text is the only free text a place
 * carries; it is never a secret (the search looks at title, address, tag and folder names only).
 *
 * The capture dialog is not here: its flag is returned so the frame mounts it itself and it stays
 * open while its button is collapsed into the overflow panel (FR-009).
 */
export function useWalletHeader(
  sidebarVisible: () => boolean,
  toggleSidebar: () => void,
) {
  const { t } = useI18n()
  const router = useTabRouter()
  const view = useWalletListViewStore()
  const { pickManyAsync } = usePickedFile()
  const walletImport = useWalletImportStore()

  const capturing = ref(false)

  /** A pass, the trash or another detail place is open: search, filters and the actions leave. The
   * list ("All") and a folder are list places: "New" there makes the pass in the folder shown. */
  const passOpen = computed(
    () =>
      router.route.path !== '/' && !router.route.path.startsWith('/folder/'),
  )

  const query = ref(router.route.query.q ?? '')
  // A history step to another place or another search puts its own text into the field.
  watch(
    () => router.route.query.q ?? '',
    (value) => {
      if (value !== query.value) query.value = value
    },
  )

  const kind = computed(() => {
    const value = router.route.query.kind
    return PASS_KINDS.find((candidate) => candidate === value) ?? null
  })

  /** "New" makes the pass in the folder the list shows, else at the top level. */
  function newPass() {
    const folder = router.route.path.startsWith('/folder/')
      ? router.route.params.id
      : undefined
    void router.push(walletNewPassLocation(undefined, folder))
  }

  /** Asks for pass files and hands them to the import dialog, which reads them one by one. */
  async function importFilesAsync() {
    const files = await pickManyAsync([
      { name: t('wallet.import.filter'), extensions: ['pkpass'] },
    ])
    if (files.length > 0) walletImport.enqueue(files)
  }

  useWmHeader(
    () =>
      walletHeader(
        {
          sidebarVisible: sidebarVisible(),
          passOpen: passOpen.value,
          query: query.value,
          sort: view.sort,
          kind: kind.value,
          hideExpired: view.hideExpired,
        },
        (key) => t(key),
      ),
    (event) => {
      const command = walletHeaderCommand(event)
      if (!command) return
      switch (command.kind) {
        case 'sidebar':
          toggleSidebar()
          break
        case 'search':
          query.value = command.text
          router.setQuery({ q: command.text.trim() ? command.text : null })
          break
        case 'sort':
          view.sort = command.sort
          break
        case 'kind':
          router.setQuery({ kind: command.value })
          break
        case 'toggleHideExpired':
          view.hideExpired = !view.hideExpired
          break
        case 'capture':
          capturing.value = true
          break
        case 'import':
          void importFilesAsync()
          break
        case 'new':
          newPass()
          break
      }
    },
  )

  return { capturing }
}
