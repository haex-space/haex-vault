/**
 * The file browser's device preferences (spec 044 FR-003, data-model.md): list or grid, the sort,
 * whether hidden entries show, and what "Orte" of the sidebar holds beyond the known places: the
 * added folders and the known places the user removed (spec 071). Stored per device
 * (ADR 0001), never synced.
 */
import type { PrefScope } from '~/composables/usePreferences'
import {
  addPlaces,
  type FilesPin,
  type FilesPlaces,
  formatPins,
  parseHidden,
  parsePins,
  removePlace,
} from '~/lib/files/pins'
import {
  DEFAULT_SORT,
  formatSort,
  parseSort,
  type Sort,
} from '~/lib/files/state'

export type FilesView = 'list' | 'grid'

const KEYS = {
  view: 'files.view',
  sort: 'files.sort',
  hidden: 'files.hidden',
  pins: 'files.pins',
  hiddenPlaces: 'files.hiddenPlaces',
} as const

const view = ref<FilesView>('list')
const sort = ref<Sort>(DEFAULT_SORT)
const showHidden = ref(false)
const pins = ref<FilesPin[]>([])
const hiddenPlaces = ref<string[]>([])
let loaded: Promise<void> | null = null

export function useFilesPrefs() {
  const { getPrefAsync, setPrefAsync } = usePreferences()
  const { currentDeviceInfoAsync } = useDevice()

  async function scopeAsync(): Promise<PrefScope> {
    const device = await currentDeviceInfoAsync()
    return { kind: 'device', uuid: device.vaultDeviceUuid }
  }

  async function loadAsync(): Promise<void> {
    const scope = await scopeAsync()
    const [storedView, storedSort, storedHidden, storedPins, storedPlaces] =
      await Promise.all([
        getPrefAsync(scope, KEYS.view),
        getPrefAsync(scope, KEYS.sort),
        getPrefAsync(scope, KEYS.hidden),
        getPrefAsync(scope, KEYS.pins),
        getPrefAsync(scope, KEYS.hiddenPlaces),
      ])
    view.value = storedView === 'grid' ? 'grid' : 'list'
    sort.value = parseSort(storedSort)
    showHidden.value = storedHidden === 'true'
    pins.value = parsePins(storedPins)
    hiddenPlaces.value = parseHidden(storedPlaces)
  }

  /** Loads once per app process; later calls share the first load. */
  function ensureLoaded(): Promise<void> {
    loaded ??= loadAsync().catch(() => {
      loaded = null
    })
    return loaded
  }

  async function saveAsync(key: string, value: string): Promise<void> {
    await setPrefAsync(await scopeAsync(), key, value)
  }

  function setView(next: FilesView) {
    view.value = next
    void saveAsync(KEYS.view, next)
  }

  function setSort(next: Sort) {
    sort.value = next
    void saveAsync(KEYS.sort, formatSort(next))
  }

  function setShowHidden(next: boolean) {
    showHidden.value = next
    void saveAsync(KEYS.hidden, String(next))
  }

  const places = computed<FilesPlaces>(() => ({
    pins: pins.value,
    hidden: hiddenPlaces.value,
  }))

  function setPlaces(next: FilesPlaces) {
    if (next.pins !== pins.value) {
      pins.value = next.pins
      void saveAsync(KEYS.pins, formatPins(next.pins))
    }
    if (next.hidden !== hiddenPlaces.value) {
      hiddenPlaces.value = next.hidden
      void saveAsync(KEYS.hiddenPlaces, JSON.stringify(next.hidden))
    }
  }

  /** Puts `added` into "Orte"; `knownPaths` are the device's known places, which come back instead. */
  const pin = (added: FilesPin[], knownPaths: readonly string[]) =>
    setPlaces(addPlaces(places.value, added, knownPaths))
  /** Takes `removed` out of "Orte": a known place is hidden, an added folder unpinned. */
  const unpin = (removed: FilesPin, knownPaths: readonly string[]) =>
    setPlaces(removePlace(places.value, removed, knownPaths))

  return {
    view: readonly(view),
    sort: readonly(sort),
    showHidden: readonly(showHidden),
    pins: readonly(pins),
    places,
    ensureLoaded,
    setView,
    setSort,
    setShowHidden,
    pin,
    unpin,
  }
}
