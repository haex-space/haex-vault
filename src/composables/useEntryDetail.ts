import { toast } from 'vue-sonner'
import type { ItemDetail } from '@bindings/ItemDetail'
import { displayTitle, isExpired, localDay } from '~/lib/passwords/format'
import { entryFreshness, type EntryFreshness } from '~/lib/passwords/remote'
import { isInTrash, trashGroupIds } from '~/lib/passwords/tree'

/**
 * The detail of one entry for its view (spec 034 US1 and US8, spec 092 Phase W): loaded once, not
 * replaced by a change from another window, an agent or another device (a banner says so and offers
 * to reload), and the facts the view shows next to it: title, expiry and whether it lies in the
 * trash. The password manager and Wallet use it for the view of a login and of a pass.
 *
 * An own change reloads the entry itself, so the banner waits a moment before it shows, to let that
 * settle.
 */
export function useEntryDetail(itemId: () => string) {
  const { errString } = useErrorString()
  const router = useTabRouter()
  const store = usePasswordsStore()
  const { getItemAsync } = usePasswords()

  const detail = ref<ItemDetail | null>(null)
  const notFound = ref(false)
  /** Whether the entry could not be loaded; the message went out as a toast. */
  const loadFailed = ref(false)

  async function loadAsync() {
    try {
      detail.value = await getItemAsync(itemId())
      notFound.value = false
      loadFailed.value = false
    } catch (cause) {
      if ((cause as { kind?: string }).kind === 'PasswordsNotFound') {
        detail.value = null
        notFound.value = true
      } else {
        loadFailed.value = true
        toast.error(errString(cause))
      }
    }
  }

  const freshness = computed(() =>
    entryFreshness({
      overviewLoaded: store.hasLoadedOnce,
      present: store.headersById.has(itemId()),
      loadedToken: detail.value?.updatedAt,
      currentToken: store.headersById.get(itemId())?.updatedAt,
    }),
  )
  const shownFreshness = ref<EntryFreshness>('fresh')
  let freshnessTimer: ReturnType<typeof setTimeout> | null = null
  watch(
    freshness,
    (value) => {
      if (freshnessTimer !== null) clearTimeout(freshnessTimer)
      if (value === 'fresh') {
        shownFreshness.value = 'fresh'
        return
      }
      freshnessTimer = setTimeout(() => {
        shownFreshness.value = freshness.value
      }, 400)
    },
    { immediate: true },
  )
  onBeforeUnmount(() => {
    if (freshnessTimer !== null) clearTimeout(freshnessTimer)
  })

  /** An entry deleted elsewhere leaves the history entry behind; skipping it keeps back from
   * returning to it. */
  function leaveDeleted() {
    if (!router.skipCurrent()) router.replace('/')
  }

  onMounted(loadAsync)

  const title = computed(() => displayTitle(detail.value?.title))
  /** An entry in the trash can be looked at, not changed or deleted again. */
  const inTrash = computed(() => {
    const groupId =
      store.headersById.get(itemId())?.groupId ?? detail.value?.groupId
    return isInTrash(groupId, trashGroupIds(store.groups))
  })
  const expired = computed(() =>
    isExpired(detail.value?.expiresAt, localDay(new Date())),
  )

  return {
    detail,
    notFound,
    loadFailed,
    loadAsync,
    shownFreshness,
    leaveDeleted,
    title,
    inTrash,
    expired,
  }
}
