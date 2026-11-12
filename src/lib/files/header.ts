// What the file browser puts into its window's header (spec 090-app-header-bar, US4, FR-025): the first
// row of spec 044 as a header description, and the commands its events stand for. Pure — relative `.ts`
// imports, so the Node check script (`scripts/check-files-header.ts`) loads it; texts come in through
// `t`, so nothing here needs i18n.
//
// ponytail: these controls exist twice, as this description (one panel, in the window's header) and
// as `components/files/PanelBar.vue` (two panels, in each panel's own header). Upgrade path: render
// `PanelBar` from the description, so there is one list of controls.
import type { WmHeaderControl } from '../../types/bindings/WmHeaderControl.ts'
import type { WmHeaderDescription } from '../../types/bindings/WmHeaderDescription.ts'
import type { WmHeaderEvent } from '../../types/bindings/WmHeaderEvent.ts'
import { breadcrumbs, SORT_KEYS, type Sort } from './state.ts'

/** The state of one panel's bar (the props of `PanelBar.vue`). */
export type PanelBarState = {
  path: string | null
  /** A name shown instead of the path, view and sort, for a panel that shows no folder. */
  title?: string
  /** The name the root shows in the path (a storage's name instead of `/`). */
  rootLabel?: string
  canGoUp: boolean
  canGoBack: boolean
  canGoForward: boolean
  view: 'list' | 'grid'
  sort: Sort
  showHidden: boolean
  canCreate: boolean
  canCreateEncrypted: boolean
}

/** What the frame itself decides: the sidebar and the switch for two panels. */
export type FilesFrameState = {
  sidebarVisible: boolean
  /** The switch exists (not on a phone, spec 062 FR-021). */
  splitAvailable: boolean
  /** Two panels are turned on. */
  splitOn: boolean
  /** The window is too narrow to turn two panels on (FR-022). */
  splitDisabled: boolean
}

export type Translate = (key: string) => string

export type CreateCommand = 'newFolder' | 'newFile' | 'newEncryptedFolder'

/** What a header event asks the file browser to do. */
export type FilesHeaderCommand =
  | { kind: 'sidebar' }
  | { kind: 'split' }
  | { kind: 'back' }
  | { kind: 'forward' }
  | { kind: 'up' }
  | { kind: 'reload' }
  | { kind: 'go'; path: string }
  | { kind: 'view'; view: 'list' | 'grid' }
  | { kind: 'sort'; sort: Sort }
  | { kind: 'hidden'; show: boolean }
  | { kind: 'create'; command: CreateCommand }

const CREATE: Record<string, CreateCommand> = {
  folder: 'newFolder',
  file: 'newFile',
  'encrypted-folder': 'newEncryptedFolder',
}

/** The segments of the path bar: the crumbs, the root under the storage's name when it has one. */
function segments(panel: PanelBarState): { id: string; label: string }[] {
  const crumbs = panel.path ? breadcrumbs(panel.path) : []
  return crumbs.map((crumb, index) => ({
    id: String(index),
    label: index === 0 && panel.rootLabel ? panel.rootLabel : crumb.name,
  }))
}

function button(
  id: string,
  testId: string,
  label: string,
  icon: string,
  disabled: boolean,
  order: number,
): WmHeaderControl {
  return {
    type: 'button',
    id,
    label,
    icon,
    testId,
    disabled,
    collapse: { order },
  }
}

/** The controls of one panel's bar: back, forward, up, the path, reload, create, view, sort. */
function panelControls(
  panel: PanelBarState,
  t: Translate,
): Pick<WmHeaderDescription, 'start' | 'center' | 'end'> {
  const back = button(
    'back',
    'files-back',
    t('files.back'),
    'lucide:arrow-left',
    !panel.canGoBack,
    70,
  )
  const forward = button(
    'forward',
    'files-forward',
    t('files.forward'),
    'lucide:arrow-right',
    !panel.canGoForward,
    60,
  )
  if (panel.title) {
    return {
      start: [back, forward],
      center: [
        {
          type: 'title',
          id: 'title',
          text: panel.title,
          testId: 'files-panel-title',
          collapse: { order: 5, form: 'shrink' },
        },
      ],
      end: [],
    }
  }
  const crumbs = segments(panel)
  const items = (
    panel.canCreateEncrypted
      ? ['folder', 'file', 'encrypted-folder']
      : ['folder', 'file']
  ).map((id) => ({
    type: 'item' as const,
    id,
    label: t(
      id === 'folder'
        ? 'files.create.folder'
        : id === 'file'
          ? 'files.create.file'
          : 'files.create.encryptedFolder',
    ),
    icon:
      id === 'folder'
        ? 'lucide:folder-plus'
        : id === 'file'
          ? 'lucide:file-plus'
          : 'lucide:folder-lock',
  }))
  return {
    start: [
      back,
      forward,
      button(
        'up',
        'files-up',
        t('files.up'),
        'lucide:arrow-up',
        !panel.canGoUp,
        50,
      ),
    ],
    center:
      crumbs.length === 0
        ? []
        : [
            {
              type: 'breadcrumb',
              id: 'path',
              label: t('files.pathBar'),
              testId: 'files-path-bar',
              segments: crumbs,
              editable: true,
              editValue: panel.path ?? '',
              collapse: { order: 15, form: 'shrink' },
            },
          ],
    end: [
      button(
        'reload',
        'files-reload',
        t('files.reload'),
        'lucide:refresh-cw',
        false,
        40,
      ),
      {
        type: 'menu',
        id: 'create',
        label: t('files.create.label'),
        icon: 'lucide:plus',
        testId: 'files-create',
        disabled: !panel.canCreate,
        items,
        collapse: { order: 30 },
      },
      {
        type: 'choice',
        id: 'view',
        label: t('files.view.label'),
        testId: 'files-view',
        selected: panel.view,
        options: [
          { id: 'list', label: t('files.view.list'), icon: 'lucide:list' },
          {
            id: 'grid',
            label: t('files.view.grid'),
            icon: 'lucide:layout-grid',
          },
        ],
        collapse: { order: 10 },
      },
      {
        type: 'menu',
        id: 'sort',
        label: t('files.menu'),
        icon: 'lucide:arrow-down-up',
        testId: 'files-sort',
        items: [
          ...SORT_KEYS.map((key) => ({
            type: 'item' as const,
            id: key,
            label: t(`files.sort.${key}`),
            icon:
              panel.sort.key !== key
                ? 'lucide:minus'
                : panel.sort.ascending
                  ? 'lucide:arrow-up'
                  : 'lucide:arrow-down',
          })),
          { type: 'separator' as const },
          {
            type: 'item' as const,
            id: 'hidden',
            label: t(
              panel.showHidden ? 'files.hidden.hide' : 'files.hidden.show',
            ),
            icon: panel.showHidden ? 'lucide:eye-off' : 'lucide:eye',
          },
        ],
        collapse: { order: 20 },
      },
    ],
  }
}

/**
 * The header of the file browser. With one panel (`panel` given) the whole first row of spec 044 is
 * in it; with two (`panel` is `null`) each panel has its own bar and the header keeps only the
 * sidebar button and the switch (FR-026).
 */
export function filesHeader(
  frame: FilesFrameState,
  panel: PanelBarState | null,
  t: Translate,
): WmHeaderDescription {
  const sidebarLabel = t(
    frame.sidebarVisible ? 'files.sidebar.hide' : 'files.sidebar.show',
  )
  const sidebar: WmHeaderControl = {
    type: 'toggle',
    id: 'sidebar',
    label: sidebarLabel,
    icon: 'lucide:panel-left',
    pressed: frame.sidebarVisible,
    testId: 'files-sidebar-toggle',
    collapse: { form: 'keep' },
  }
  const split: WmHeaderControl[] = frame.splitAvailable
    ? [
        {
          type: 'toggle',
          id: 'split',
          label: t('files.split.toggle'),
          tooltip: t(
            frame.splitDisabled
              ? 'files.split.tooNarrow'
              : 'files.split.toggle',
          ),
          icon: 'lucide:columns-2',
          pressed: frame.splitOn,
          disabled: frame.splitDisabled,
          testId: 'files-split-toggle',
          collapse: { order: 80 },
        },
      ]
    : []
  const controls = panel
    ? panelControls(panel, t)
    : { start: [], center: [], end: [] }
  return {
    start: [sidebar, ...(controls.start ?? [])],
    center: controls.center ?? [],
    end: [...(controls.end ?? []), ...split],
  }
}

/** The command a header event stands for, `null` for an event that asks for nothing. */
export function filesHeaderCommand(
  event: WmHeaderEvent,
  panel: PanelBarState | null,
): FilesHeaderCommand | null {
  if (event.id === 'sidebar' && event.kind === 'toggle')
    return { kind: 'sidebar' }
  if (event.id === 'split' && event.kind === 'toggle') return { kind: 'split' }
  if (!panel) return null
  const value = event.value
  switch (event.id) {
    case 'back':
    case 'forward':
    case 'up':
    case 'reload':
      return event.kind === 'click' ? { kind: event.id } : null
    case 'path': {
      if (typeof value !== 'string') return null
      if (event.kind === 'submit') {
        const path = value.trim()
        return path ? { kind: 'go', path } : null
      }
      const crumb =
        event.kind === 'select' && panel.path
          ? breadcrumbs(panel.path)[Number(value)]
          : undefined
      return crumb ? { kind: 'go', path: crumb.path } : null
    }
    case 'create': {
      const command = typeof value === 'string' ? CREATE[value] : undefined
      return event.kind === 'click' && command
        ? { kind: 'create', command }
        : null
    }
    case 'view':
      return event.kind === 'select' && (value === 'list' || value === 'grid')
        ? { kind: 'view', view: value }
        : null
    case 'sort': {
      if (event.kind !== 'click') return null
      if (value === 'hidden') return { kind: 'hidden', show: !panel.showHidden }
      const key = SORT_KEYS.find((candidate) => candidate === value)
      return key
        ? {
            kind: 'sort',
            sort: {
              key,
              ascending: panel.sort.key === key ? !panel.sort.ascending : true,
            },
          }
        : null
    }
    default:
      return null
  }
}

/** The part of a DOM element `inWindowHeader` reads (an `Element` fits; the Node check passes stubs). */
export type HeaderKeyTarget = {
  closest(selector: string): HeaderKeyTarget | null
  getAttribute(name: string): string | null
}

/**
 * Whether a key event's target is a control of the header of window `windowId`. The header is drawn
 * by the window, outside the file browser's own root, so its keys do not reach the frame's keyboard
 * handler by bubbling; the frame listens for them itself and uses this to take only its own
 * window's (several windows and tabs are mounted at once).
 */
export function inWindowHeader(
  target: HeaderKeyTarget | null,
  windowId: string,
): boolean {
  const owner = target
    ?.closest('[data-testid="wm-header"]')
    ?.closest('[data-wm-window-id]')
  return !!owner && owner.getAttribute('data-wm-window-id') === windowId
}
