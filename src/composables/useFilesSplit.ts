import {
  inject,
  nextTick,
  provide,
  shallowReactive,
  type InjectionKey,
} from 'vue'
import type { Drive } from '@bindings/Drive'
import type { SourceRef } from '@bindings/SourceRef'
import type { Sources } from '@bindings/Sources'
import {
  type FilesPlace,
  filesLocation,
  parseFilesPlace,
} from '~/lib/files/registry'
import {
  disableSplit,
  enableSplit,
  incomingHistory,
  readSplit,
  type SplitSide,
  type SplitState,
  swapActive,
  withSplit,
} from '~/lib/files/split'
import type { PlaceLock } from '~/lib/files/clipboard'
import { placeOnDrive } from '~/lib/files/drives'
import {
  canGoBack,
  canGoForward,
  locationsEqual,
  type TabHistory,
} from '~/lib/wm/navigation'

/**
 * The frame of the file browser (spec 062, `contracts/panel-context.md`): it owns where the tab
 * stands and hands every panel a context to read its place and to write it back. Every write to the
 * tab location goes through here, so the keys of the split (`split`, `side`, `o.*`) are never lost;
 * a panel never calls `useTabRouter()`.
 *
 * The plain keys of the location (and its history) belong to the *active* panel; the other panel's
 * place is the frame's state, mirrored into the current entry. Each panel has its own history: the
 * tab's history is the active panel's, the other one's waits in memory (`lane`) and is swapped in
 * when that panel becomes the active one (`data-model.md`, research R2).
 */

/** What a panel offers the frame and the other panel (data-model.md "PanelHandle"). */
export type PanelHandle = {
  side: SplitSide
  /** Where this panel takes transfers in; `null` while it has no folder yet. */
  target(): { source: SourceRef; path: string | null; lock: PlaceLock } | null
  reload(): unknown
  /** Ends the selection (the entries a drag took out of this panel are gone). */
  clearSelection(): void
  /** Focuses the last focused entry, else the first. */
  focus(): void
  /** A key event of the frame outside both panels (toolbar, sidebar): the active panel acts on it. */
  keydown(event: KeyboardEvent): void
}

/** Query parts a panel changes without moving; `null` removes one. */
export type FilesQueryPatch = Partial<
  Record<
    'open' | 'q' | 't' | 's' | 'd' | 'vw' | 'so' | 'sr' | 'sh',
    string | null
  >
>

export type FilesPanelContext = {
  side: SplitSide
  /** The place of this panel. */
  place: ComputedRef<FilesPlace>
  /** This panel is the active one. */
  active: ComputedRef<boolean>
  /** Two panels show. */
  split: ComputedRef<boolean>
  /** Navigates this panel to `place` (a new history entry unless `replace`); a panel that is not the
   * active one becomes it first. */
  go(place: FilesPlace, options?: { replace?: boolean }): void
  /** Sets the starting place of this panel (replaces the entry, activates nothing). */
  initialize(place: FilesPlace): void
  setQuery(patch: FilesQueryPatch, options?: { push?: boolean }): void
  /** This panel's own history (spec 062: back and forward are per panel); going makes it the active
   * one first. */
  canGoBack: ComputedRef<boolean>
  canGoForward: ComputedRef<boolean>
  back(): void
  forward(): void
  register(handle: PanelHandle): () => void
  /** The other panel, `null` without a split. */
  other(): PanelHandle | null
  /** Makes this panel the active one. */
  activate(): void
  /** Moves the focus to the other panel and makes it the active one. */
  switchPanel(): void
  sources: Ref<Sources | null>
  reloadSources(): void
  /** The drives of this device (spec 064); `drivesKnown` once the list came. */
  drives: Readonly<Ref<readonly Drive[]>>
  drivesKnown: Readonly<Ref<boolean>>
  tabId: string
  /** The tab is the one in front of its window. */
  tabActive: ComputedRef<boolean>
  openStorageSettings(): void
  /** Where a panel renders its path bar and its viewer: elements of the frame (not ids, as several
   * file browser tabs are mounted at once). `null` until the frame is mounted. */
  pathHost: Readonly<Ref<HTMLElement | null>>
  viewerHost: Readonly<Ref<HTMLElement | null>>
}

/** The frame hands each panel its context by side. */
export const FILES_PANEL_KEY: InjectionKey<
  (side: SplitSide) => FilesPanelContext
> = Symbol('filesPanel')

/** The context of the panel on `side`, from the frame above. */
export function useFilesPanel(side: SplitSide): FilesPanelContext {
  const panel = inject(FILES_PANEL_KEY)
  if (!panel) throw new Error('a file browser panel needs its frame')
  return panel(side)
}

const DEVICE_START: FilesPlace = { source: { kind: 'device' }, path: null }

const opposite = (side: SplitSide): SplitSide =>
  side === 'left' ? 'right' : 'left'

export function useFilesSplit(options: {
  sources: Ref<Sources | null>
  reloadSources: () => void
  drives: Readonly<Ref<readonly Drive[]>>
  drivesKnown: Readonly<Ref<boolean>>
  pathHost: Readonly<Ref<HTMLElement | null>>
  viewerHost: Readonly<Ref<HTMLElement | null>>
  /** The platform offers two panels (not on a phone, FR-021). */
  available: Readonly<Ref<boolean>>
  /** The panel area is wide enough for two panels (FR-022). */
  fits: Readonly<Ref<boolean>>
}) {
  const router = useTabRouter()
  const wm = useWindowManagerStore()
  const { tabId, windowId, openApp } = useWmTab()

  // What the frame knows beyond the plain keys; read from the location once (start, session).
  const first = readSplit(router.route)
  const start = first.on ? enableSplit(first) : first
  const stored = ref({ on: start.on, side: start.side, other: start.other })
  /** The history of the panel that is not the active one (kept in memory only). */
  const lane = shallowRef<TabHistory | null>(null)

  const active = computed(() => parseFilesPlace(router.route) ?? DEVICE_START)
  const state = computed((): SplitState => ({
    ...stored.value,
    active: active.value,
  }))
  const shown = computed(
    () => stored.value.on && options.available.value && options.fits.value,
  )
  const activeSide = computed(() => stored.value.side)

  const tabActive = computed(
    () =>
      wm.windows.find((window) => window.id === windowId)?.activeTabId ===
      tabId,
  )

  function keep(next: SplitState) {
    stored.value = { on: next.on, side: next.side, other: next.other }
  }

  /** Writes the split keys of the state into the current entry, when they differ (contract rule 2). */
  function mirror() {
    const wanted = withSplit(router.route, state.value)
    if (!locationsEqual(router.route, wanted)) router.replace(wanted)
  }
  // After a jump in the history (back, forward, session) or a push from outside, the entry may not
  // carry the keys of the split.
  watch(() => JSON.stringify([router.route.path, router.route.query]), mirror, {
    flush: 'post',
  })

  function write(to: FilesPlace, replace: boolean) {
    const location = withSplit(
      filesLocation(placeOnDrive(to, options.drives.value)),
      state.value,
    )
    if (replace) router.replace(location)
    else router.push(location)
  }

  // A place written from outside the frame (an agent, a restored tab of before) learns its drive
  // once the list knows it, and a deeper drive mounted later replaces the one it named (spec 064
  // FR-025).
  watch(
    () => [active.value, options.drives.value] as const,
    ([place, drives]) => {
      const on = placeOnDrive(place, drives)
      if (on.drive !== place.drive || on.droot !== place.droot) write(on, true)
    },
    { immediate: true },
  )

  /** Makes `side` the active panel: its history comes in, the other's waits as the lane. Whether it
   * is the active one now. */
  function activate(side: SplitSide): boolean {
    const current = state.value
    if (side === current.side) return true
    if (!current.other) return false
    const next = swapActive(current)
    const previous = router.swapHistory(
      incomingHistory(lane.value, withSplit(filesLocation(next.active), next)),
    )
    if (!previous) return false
    lane.value = previous
    keep(next)
    return true
  }

  function toggle() {
    keep(stored.value.on ? disableSplit(state.value) : enableSplit(state.value))
    lane.value = null
    mirror()
  }

  const handles = shallowReactive(new Map<SplitSide, PanelHandle>())
  const openStorageSettings = () => openApp('system.settings', '/storage')

  /** Keys outside both panels act on the active panel (FR-016); the panels handle their own. */
  function onKeydown(event: KeyboardEvent) {
    const target = event.target instanceof Element ? event.target : null
    if (target?.closest('[data-files-panel]')) return
    handles.get(stored.value.side)?.keydown(event)
  }

  function switchFrom(side: SplitSide) {
    const to = opposite(side)
    if (!shown.value || !activate(to)) return
    void nextTick(() => handles.get(to)?.focus())
  }

  const panel = (side: SplitSide): FilesPanelContext => ({
    side,
    place: computed(() =>
      side === stored.value.side
        ? active.value
        : (stored.value.other ?? DEVICE_START),
    ),
    active: computed(() => side === stored.value.side),
    split: shown,
    go(to, goOptions = {}) {
      if (activate(side)) write(to, goOptions.replace === true)
    },
    initialize(to) {
      if (side === stored.value.side) {
        write(to, true)
        return
      }
      stored.value = { ...stored.value, other: to }
      mirror()
    },
    canGoBack: computed(() =>
      side === stored.value.side
        ? router.canGoBack
        : lane.value !== null && canGoBack(lane.value),
    ),
    canGoForward: computed(() =>
      side === stored.value.side
        ? router.canGoForward
        : lane.value !== null && canGoForward(lane.value),
    ),
    back() {
      if (activate(side)) router.back()
    },
    forward() {
      if (activate(side)) router.forward()
    },
    setQuery(patch, queryOptions) {
      if (activate(side)) router.setQuery(patch, queryOptions)
    },
    register(handle) {
      handles.set(side, handle)
      return () => {
        if (handles.get(side) === handle) handles.delete(side)
      }
    },
    other: () => (shown.value ? (handles.get(opposite(side)) ?? null) : null),
    activate: () => void activate(side),
    switchPanel: () => switchFrom(side),
    sources: options.sources,
    reloadSources: options.reloadSources,
    drives: options.drives,
    drivesKnown: options.drivesKnown,
    tabId,
    tabActive,
    openStorageSettings,
    pathHost: options.pathHost,
    viewerHost: options.viewerHost,
  })
  provide(FILES_PANEL_KEY, panel)

  return {
    /** The place of the active panel. */
    place: active,
    /** The sidebar: it keeps the view and the sort of the active panel. */
    go: (to: FilesPlace, goOptions: { replace?: boolean } = {}) =>
      write(
        { vw: active.value.vw, so: active.value.so, ...to },
        goOptions.replace === true,
      ),
    tabId,
    tabActive,
    openStorageSettings,
    onKeydown,
    toggle,
    on: computed(() => stored.value.on),
    shown,
    activeSide,
    /** The other panel has a place, so it is mounted (hidden while the split is off). */
    hasOther: computed(() => stored.value.other !== null),
  }
}
