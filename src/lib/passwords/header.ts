// What the password manager puts into its window's header (spec 093-app-headers, US1): the first row
// of spec 034 as a header description, and the commands its events stand for. Pure — relative `.ts`
// imports, so the Node check script (`scripts/check-passwords-header.ts`) loads it; texts come in
// through `t`, so nothing here needs i18n.
import type { WmHeaderControl } from '../../types/bindings/WmHeaderControl.ts'
import type { WmHeaderDescription } from '../../types/bindings/WmHeaderDescription.ts'
import type { WmHeaderEvent } from '../../types/bindings/WmHeaderEvent.ts'
import { sidebarToggle } from '../wm/headerParts.ts'

export type PasswordsHeaderState = {
  sidebarVisible: boolean
  /** An entry is open: its view, its editor or a new one. Search, tag filter and "+" leave. */
  entryOpen: boolean
  query: string
  /** The tags of the password manager with their counts (`loginTags` of the store). */
  tags: readonly { id: string; name: string; itemCount: number }[]
  /** The tag that filters the list (`?tag=`), `null` for all tags. */
  activeTagId: string | null
}

export type Translate = (key: string) => string

/** What a header event asks the password manager to do. */
export type PasswordsHeaderCommand =
  | { kind: 'sidebar' }
  | { kind: 'search'; text: string }
  | { kind: 'tag'; id: string | null }
  | { kind: 'newEntry' }
  | { kind: 'newFolder' }
  | { kind: 'generator' }
  | { kind: 'import' }
  | { kind: 'tags' }
  | { kind: 'settings' }

/** The entry of the tag filter that stands for "no tag". A tag id is never this word. */
const ALL_TAGS = 'all'

export function passwordsHeader(
  state: PasswordsHeaderState,
  t: Translate,
): WmHeaderDescription {
  const start: WmHeaderControl[] = [
    sidebarToggle({
      visible: state.sidebarVisible,
      hideLabel: t('passwords.sidebar.hide'),
      showLabel: t('passwords.sidebar.show'),
      testId: 'passwords-sidebar-toggle',
    }),
  ]
  const menu: WmHeaderControl = {
    type: 'menu',
    id: 'menu',
    label: t('passwords.menuTitle'),
    icon: 'lucide:menu',
    testId: 'passwords-menu',
    collapse: { order: 30 },
    items: [
      {
        type: 'item',
        id: 'generator',
        label: t('passwords.generator.title'),
        icon: 'lucide:dices',
      },
      {
        type: 'item',
        id: 'import',
        label: t('passwords.import.open'),
        icon: 'lucide:file-down',
      },
      {
        type: 'item',
        id: 'tags',
        label: t('passwords.tags.manage'),
        icon: 'lucide:tags',
      },
      { type: 'separator' },
      {
        type: 'item',
        id: 'settings',
        label: t('passwords.settings.title'),
        icon: 'lucide:settings-2',
      },
    ],
  }
  if (state.entryOpen) return { start, end: [menu] }

  const end: WmHeaderControl[] = []
  if (state.tags.length) {
    end.push({
      type: 'menu',
      id: 'tag-filter',
      label: t('passwords.tagFilter.label'),
      icon: 'lucide:tag',
      testId: 'passwords-tag-filter',
      active: state.activeTagId !== null,
      collapse: { order: 20 },
      items: [
        {
          type: 'item',
          id: ALL_TAGS,
          label: t('passwords.tagFilter.all'),
          icon: 'lucide:tags',
          checked: state.activeTagId === null,
        },
        { type: 'separator' },
        ...state.tags.map((tag) => ({
          type: 'item' as const,
          id: tag.id,
          label: `${tag.name} (${tag.itemCount})`,
          icon: 'lucide:tag',
          checked: state.activeTagId === tag.id,
        })),
      ],
    })
  }
  end.push(
    {
      type: 'menu',
      id: 'new',
      label: t('passwords.new'),
      icon: 'lucide:plus',
      testId: 'passwords-new',
      collapse: { order: 40 },
      items: [
        {
          type: 'item',
          id: 'entry',
          label: t('passwords.newMenu.entry'),
          icon: 'lucide:key-round',
        },
        {
          type: 'item',
          id: 'folder',
          label: t('passwords.newMenu.folder'),
          icon: 'lucide:folder-plus',
        },
      ],
    },
    menu,
  )
  return {
    start,
    center: [
      {
        type: 'search',
        id: 'q',
        label: t('passwords.search.label'),
        placeholder: t('passwords.search.placeholder'),
        value: state.query,
        testId: 'passwords-search',
        collapse: { order: 10, form: 'shrink' },
      },
    ],
    end,
  }
}

/** The command an event stands for; `null` for an event the app does not act on. */
export function passwordsHeaderCommand(
  event: WmHeaderEvent,
): PasswordsHeaderCommand | null {
  const value = event.value
  switch (event.id) {
    case 'sidebar':
      return event.kind === 'toggle' ? { kind: 'sidebar' } : null
    case 'q':
      return event.kind === 'input'
        ? { kind: 'search', text: String(value ?? '') }
        : null
    case 'tag-filter':
      if (event.kind !== 'click' || typeof value !== 'string') return null
      return { kind: 'tag', id: value === ALL_TAGS ? null : value }
    case 'new':
      if (event.kind !== 'click') return null
      return value === 'entry'
        ? { kind: 'newEntry' }
        : value === 'folder'
          ? { kind: 'newFolder' }
          : null
    case 'menu':
      if (event.kind !== 'click') return null
      return value === 'generator' ||
        value === 'import' ||
        value === 'tags' ||
        value === 'settings'
        ? { kind: value }
        : null
    default:
      return null
  }
}
