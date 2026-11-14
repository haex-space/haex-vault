// What Wallet puts into its window's header (spec 093-app-headers, US3): the first row of spec 092 as
// a header description, and the commands its events stand for. Pure — relative `.ts` imports, so the
// Node check script (`scripts/check-wallet-header.ts`) loads it; texts come in through `t`, so
// nothing here needs i18n.
import type { WmHeaderControl } from '../../types/bindings/WmHeaderControl.ts'
import type { WmHeaderDescription } from '../../types/bindings/WmHeaderDescription.ts'
import type { WmHeaderEvent } from '../../types/bindings/WmHeaderEvent.ts'
import {
  PASS_KIND_ICONS,
  PASS_KINDS,
  type PassKind,
} from '../passwords/passType.ts'
import { sidebarToggle } from '../wm/headerParts.ts'

export type WalletSort = 'date' | 'title'

export type WalletHeaderState = {
  sidebarVisible: boolean
  /** A pass, the trash or another detail place is open: search, filters and the actions leave. */
  passOpen: boolean
  query: string
  sort: WalletSort
  /** The kind that filters the list (`?kind=`), `null` for all kinds. */
  kind: PassKind | null
  hideExpired: boolean
}

export type Translate = (key: string) => string

/** What a header event asks Wallet to do. */
export type WalletHeaderCommand =
  | { kind: 'sidebar' }
  | { kind: 'search'; text: string }
  | { kind: 'sort'; sort: WalletSort }
  | { kind: 'kind'; value: PassKind | null }
  | { kind: 'toggleHideExpired' }
  | { kind: 'capture' }
  | { kind: 'import' }
  | { kind: 'new' }

const SORTS: readonly WalletSort[] = ['date', 'title']
const SORT_ICONS: Record<WalletSort, string> = {
  date: 'lucide:calendar',
  title: 'lucide:arrow-down-a-z',
}

export function walletHeader(
  state: WalletHeaderState,
  t: Translate,
): WmHeaderDescription {
  const start = [
    sidebarToggle({
      visible: state.sidebarVisible,
      hideLabel: t('wallet.sidebar.hide'),
      showLabel: t('wallet.sidebar.show'),
      testId: 'wallet-sidebar-toggle',
    }),
  ]
  if (state.passOpen) return { start }

  const button = (
    id: string,
    label: string,
    icon: string,
    order: number,
  ): WmHeaderControl => ({
    type: 'button',
    id,
    label,
    icon,
    testId: `wallet-${id}`,
    collapse: { order },
  })
  return {
    start,
    center: [
      {
        type: 'search',
        id: 'q',
        label: t('wallet.search.label'),
        placeholder: t('wallet.search.placeholder'),
        value: state.query,
        testId: 'wallet-search',
        collapse: { order: 10, form: 'shrink' },
      },
    ],
    end: [
      {
        type: 'menu',
        id: 'filters',
        label: t('wallet.filters.label'),
        icon: 'lucide:sliders-horizontal',
        testId: 'wallet-filters',
        // A filter or a sort that is not the default (FR-008).
        active:
          state.kind !== null || state.hideExpired || state.sort !== 'date',
        collapse: { order: 20 },
        items: [
          // A menu entry has no group heading, so the sort says what it sorts.
          ...SORTS.map((sort) => ({
            type: 'item' as const,
            id: `sort-${sort}`,
            label: `${t('wallet.filters.sort')}: ${t(`wallet.filters.sorts.${sort}`)}`,
            icon: SORT_ICONS[sort],
            checked: state.sort === sort,
          })),
          { type: 'separator' },
          {
            type: 'item',
            id: 'kind-all',
            label: t('wallet.filters.allKinds'),
            icon: 'lucide:wallet',
            checked: state.kind === null,
          },
          ...PASS_KINDS.map((kind) => ({
            type: 'item' as const,
            id: `kind-${kind}`,
            label: t(`passwords.kinds.${kind}`),
            icon: PASS_KIND_ICONS[kind],
            checked: state.kind === kind,
          })),
          { type: 'separator' },
          {
            type: 'item',
            id: 'hide-expired',
            label: t('wallet.filters.hideExpired'),
            icon: 'lucide:calendar-x',
            checked: state.hideExpired,
          },
        ],
      },
      button('capture', t('wallet.capture.title'), 'lucide:scan-line', 25),
      button('import', t('wallet.import.button'), 'lucide:file-down', 30),
      button('new', t('wallet.new'), 'lucide:plus', 40),
    ],
  }
}

/** The command an event stands for; `null` for an event the app does not act on. */
export function walletHeaderCommand(
  event: WmHeaderEvent,
): WalletHeaderCommand | null {
  switch (event.id) {
    case 'sidebar':
      return event.kind === 'toggle' ? { kind: 'sidebar' } : null
    case 'q':
      return event.kind === 'input'
        ? { kind: 'search', text: String(event.value ?? '') }
        : null
    case 'filters': {
      if (event.kind !== 'click' || typeof event.value !== 'string') return null
      const entry = event.value
      if (entry === 'hide-expired') return { kind: 'toggleHideExpired' }
      if (entry === 'kind-all') return { kind: 'kind', value: null }
      const sort = SORTS.find((candidate) => entry === `sort-${candidate}`)
      if (sort) return { kind: 'sort', sort }
      const kind = PASS_KINDS.find((candidate) => entry === `kind-${candidate}`)
      return kind ? { kind: 'kind', value: kind } : null
    }
    case 'capture':
    case 'import':
    case 'new':
      return event.kind === 'click' ? { kind: event.id } : null
    default:
      return null
  }
}
