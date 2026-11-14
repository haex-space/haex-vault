// What the address book puts into its window's header (spec 093-app-headers, US2): the first row of
// spec 051 as a header description, and the commands its events stand for. Pure — relative `.ts`
// imports, so the Node check script (`scripts/check-contacts-header.ts`) loads it; texts come in
// through `t`, so nothing here needs i18n.
import type { WmHeaderDescription } from '../../types/bindings/WmHeaderDescription.ts'
import type { WmHeaderEvent } from '../../types/bindings/WmHeaderEvent.ts'
import { sidebarToggle } from '../wm/headerParts.ts'

export type ContactsHeaderState = {
  sidebarVisible: boolean
  /** A contact or the reading of a share is open: no search and no "+" there. */
  entryOpen: boolean
  query: string
}

export type Translate = (key: string) => string

/** What a header event asks the address book to do. */
export type ContactsHeaderCommand =
  | { kind: 'sidebar' }
  | { kind: 'search'; text: string }
  | { kind: 'newContact' }
  | { kind: 'import' }

export function contactsHeader(
  state: ContactsHeaderState,
  t: Translate,
): WmHeaderDescription {
  const start = [
    sidebarToggle({
      visible: state.sidebarVisible,
      hideLabel: t('contacts.sidebar.hide'),
      showLabel: t('contacts.sidebar.show'),
      testId: 'contacts-sidebar-toggle',
    }),
  ]
  if (state.entryOpen) return { start }
  return {
    start,
    center: [
      {
        type: 'search',
        id: 'q',
        label: t('contacts.search.label'),
        placeholder: t('contacts.search.placeholder'),
        value: state.query,
        testId: 'contacts-search',
        collapse: { order: 10, form: 'shrink' },
      },
    ],
    end: [
      {
        type: 'menu',
        id: 'add',
        label: t('contacts.add'),
        icon: 'lucide:plus',
        testId: 'contacts-add',
        collapse: { order: 40 },
        items: [
          {
            type: 'item',
            id: 'new',
            label: t('contacts.new'),
            icon: 'lucide:user-plus',
          },
          {
            type: 'item',
            id: 'import',
            label: t('contacts.import.menu'),
            icon: 'lucide:scan-qr-code',
          },
        ],
      },
    ],
  }
}

/** The command an event stands for; `null` for an event the app does not act on. */
export function contactsHeaderCommand(
  event: WmHeaderEvent,
): ContactsHeaderCommand | null {
  switch (event.id) {
    case 'sidebar':
      return event.kind === 'toggle' ? { kind: 'sidebar' } : null
    case 'q':
      return event.kind === 'input'
        ? { kind: 'search', text: String(event.value ?? '') }
        : null
    case 'add':
      if (event.kind !== 'click') return null
      return event.value === 'new'
        ? { kind: 'newContact' }
        : event.value === 'import'
          ? { kind: 'import' }
          : null
    default:
      return null
  }
}
