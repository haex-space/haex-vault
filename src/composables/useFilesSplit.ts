import { inject, provide, type InjectionKey } from 'vue'
import type { SourceRef } from '@bindings/SourceRef'
import type { Sources } from '@bindings/Sources'
import {
  type FilesPlace,
  filesLocation,
  parseFilesPlace,
} from '~/lib/files/registry'
import { readSplit, type SplitSide, withSplit } from '~/lib/files/split'

/**
 * The frame of the file browser (spec 062, `contracts/panel-context.md`): it owns where the tab
 * stands and hands every panel a context to read its place and to write it back. Every write to the
 * tab location goes through here, so the keys of the split (`split`, `side`, `o.*`) are never lost;
 * a panel never calls `useTabRouter()`. Until the split arrives there is one panel, always the
 * active one.
 */

/** What a panel offers the frame and the other panel (data-model.md "PanelHandle"). */
export type PanelHandle = {
  side: SplitSide
  /** Where this panel takes transfers in; `null` while it has no folder yet. */
  target(): { source: SourceRef; path: string | null; readOnly: boolean } | null
  reload(): unknown
  /** Focuses the last focused entry, else the first. */
  focus(): void
  /** A key event of the frame outside both panels (toolbar, sidebar): the active panel acts on it. */
  keydown(event: KeyboardEvent): void
}

/** Query parts a panel changes without moving; `null` removes one. */
export type FilesQueryPatch = Partial<
  Record<'open' | 'q' | 't' | 's' | 'd', string | null>
>

export type FilesPanelContext = {
  side: SplitSide
  /** The place of this panel, read from the tab location. */
  place: ComputedRef<FilesPlace>
  /** This panel is the active one. */
  active: ComputedRef<boolean>
  /** Two panels show. */
  split: ComputedRef<boolean>
  /** Navigates this panel to `place` (a new history entry unless `replace`). */
  go(place: FilesPlace, options?: { replace?: boolean }): void
  /** Sets the starting place of this panel (replaces the entry, activates nothing). */
  initialize(place: FilesPlace): void
  setQuery(patch: FilesQueryPatch, options?: { push?: boolean }): void
  register(handle: PanelHandle): () => void
  /** The other panel, `null` without a split. */
  other(): PanelHandle | null
  /** Makes this panel the active one. */
  activate(): void
  /** Moves the focus to the other panel and makes it the active one. */
  switchPanel(): void
  sources: Ref<Sources | null>
  reloadSources(): void
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

export function useFilesSplit(options: {
  sources: Ref<Sources | null>
  reloadSources: () => void
  pathHost: Readonly<Ref<HTMLElement | null>>
  viewerHost: Readonly<Ref<HTMLElement | null>>
}) {
  const router = useTabRouter()
  const wm = useWindowManagerStore()
  const { tabId, windowId, openApp } = useWmTab()

  const place = computed(
    (): FilesPlace =>
      parseFilesPlace(router.route) ?? {
        source: { kind: 'device' },
        path: null,
      },
  )
  const tabActive = computed(
    () =>
      wm.windows.find((window) => window.id === windowId)?.activeTabId ===
      tabId,
  )

  function write(to: FilesPlace, replace: boolean) {
    const location = withSplit(filesLocation(to), readSplit(router.route))
    if (replace) router.replace(location)
    else router.push(location)
  }
  const go = (to: FilesPlace, goOptions: { replace?: boolean } = {}) =>
    write(to, goOptions.replace === true)

  const openStorageSettings = () => openApp('system.settings', '/storage')
  const handles = new Map<SplitSide, PanelHandle>()

  /** Keys outside both panels act on the active panel (FR-016); the panels handle their own. */
  function onKeydown(event: KeyboardEvent) {
    const target = event.target instanceof Element ? event.target : null
    if (target?.closest('[data-files-panel]')) return
    handles.get('left')?.keydown(event)
  }

  const panel = (side: SplitSide): FilesPanelContext => ({
    side,
    place,
    active: computed(() => true),
    split: computed(() => false),
    go,
    initialize: (to) => write(to, true),
    setQuery: (patch, queryOptions) => router.setQuery(patch, queryOptions),
    register(handle) {
      handles.set(side, handle)
      return () => {
        if (handles.get(side) === handle) handles.delete(side)
      }
    },
    other: () => null,
    activate() {},
    switchPanel() {},
    sources: options.sources,
    reloadSources: options.reloadSources,
    tabId,
    tabActive,
    openStorageSettings,
    pathHost: options.pathHost,
    viewerHost: options.viewerHost,
  })
  provide(FILES_PANEL_KEY, panel)

  return {
    place,
    go,
    tabId,
    tabActive,
    openStorageSettings,
    onKeydown,
  }
}
