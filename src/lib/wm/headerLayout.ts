// How the window header collapses when it is too narrow (spec 090-app-header-bar, FR-011,
// FR-014–FR-017, contracts/collapse.md). Pure: the component measures the controls and applies the
// result. Relative sibling imports only, so the Node check script
// (`scripts/check-wm-header.ts`) loads it without Nuxt.

export type HeaderArea = 'start' | 'center' | 'end'
export type CollapseForm = 'overflow' | 'icon' | 'shrink' | 'keep'

/** One control as measured: its place, what the app ordered and how wide each form is. */
export type LayoutControl = {
  id: string
  area: HeaderArea
  /** Position inside its area, in the order the app gave. */
  index: number
  /** The app's own `collapse.order`, `null` when it said nothing. */
  order: number | null
  form: CollapseForm
  fullWidth: number
  /** Width of the icon form; `null` when the control has no icon. */
  iconWidth: number | null
  /** The type's smallest width for `shrink`; `null` for a type that cannot shrink. */
  minWidth: number | null
}

export type LayoutFixed = {
  /** The window buttons (one on a compact window). */
  windowButtons: number
  /** The button that opens the overflow menu. */
  menuButton: number
  /** Between two neighbours, and between the last one and the window buttons. */
  gap: number
  /** Left and right padding of the row together. */
  padding: number
}

export type ControlState = 'row' | 'icon' | 'overflow' | { shrunk: number }

export type HeaderLayout = {
  states: Map<string, ControlState>
  /** Whether the overflow menu button shows: at least one control moved into it. */
  menuVisible: boolean
}

/** The smallest width a control of this type can shrink to (research R7); `null` if it cannot. */
export function minWidthOf(type: string): number | null {
  if (type === 'search') return 120
  if (type === 'breadcrumb') return 96
  if (type === 'title') return 64
  return null
}

/** A control without an order of its own collapses after every control that has one. */
const DEFAULT_ORDER_BASE = 1001

/**
 * The control's place rank (data-model.md): the end area from its last control, then the centre
 * from its last, then the start from its last: 0, 1, 2, … The one that collapses first has the
 * lowest rank.
 */
function placeRanks(controls: readonly LayoutControl[]): Map<string, number> {
  const count = { start: 0, center: 0, end: 0 }
  for (const c of controls) count[c.area] += 1
  const before = {
    end: 0,
    center: count.end,
    start: count.end + count.center,
  }
  return new Map(
    controls.map((c) => [c.id, before[c.area] + (count[c.area] - 1 - c.index)]),
  )
}

/** What a form really is for this control: a form that does not fit counts as `overflow`. */
function effectiveForm(c: LayoutControl): CollapseForm {
  if (c.form === 'icon' && c.iconWidth === null) return 'overflow'
  if (c.form === 'shrink' && c.minWidth === null) return 'overflow'
  return c.form
}

/**
 * Which controls stay in the row, which give way and how, for a header `available` px wide.
 *
 * Always starts from "everything in the row" (so widening gives the controls back in the reverse
 * order and in their previous form, FR-017): first the controls in order of their `order`, each in
 * its own form while the row does not fit; then icons and shrunk controls move into the menu; at
 * last the `keep` controls do, in the order of the places. The window buttons and the menu button
 * never move.
 */
export function layoutHeader(
  controls: readonly LayoutControl[],
  fixed: LayoutFixed,
  available: number,
): HeaderLayout {
  const ranks = placeRanks(controls)
  const rank = (c: LayoutControl) => ranks.get(c.id) ?? 0
  const orderOf = (c: LayoutControl) => c.order ?? DEFAULT_ORDER_BASE + rank(c)
  const byOrder = (a: LayoutControl, b: LayoutControl) =>
    orderOf(a) - orderOf(b) || rank(a) - rank(b)

  const states = new Map<string, ControlState>(
    controls.map((c) => [c.id, 'row']),
  )

  const widthOf = (c: LayoutControl, state: ControlState): number => {
    if (state === 'row') return c.fullWidth
    if (state === 'icon') return c.iconWidth ?? c.fullWidth
    if (state === 'overflow') return 0
    return state.shrunk
  }
  const menuVisible = () => [...states.values()].some((s) => s === 'overflow')
  const needed = (): number => {
    let items = 0
    let width = 0
    for (const c of controls) {
      const state = states.get(c.id) ?? 'row'
      if (state === 'overflow') continue
      items += 1
      width += widthOf(c, state)
    }
    if (menuVisible()) {
      items += 1
      width += fixed.menuButton
    }
    return fixed.padding + fixed.windowButtons + width + fixed.gap * items
  }
  const fits = () => needed() <= available

  const giving = controls.filter((c) => effectiveForm(c) !== 'keep')
  const keeping = controls.filter((c) => effectiveForm(c) === 'keep')

  // First stage: each control in its own form, in order, until the row fits.
  for (const c of [...giving].sort(byOrder)) {
    if (fits()) break
    const form = effectiveForm(c)
    if (form === 'overflow') {
      states.set(c.id, 'overflow')
    } else if (form === 'icon') {
      states.set(c.id, 'icon')
    } else if (form === 'shrink') {
      const min = c.minWidth ?? c.fullWidth
      const width = Math.max(min, c.fullWidth - (needed() - available))
      if (width < c.fullWidth) states.set(c.id, { shrunk: width })
    }
  }

  // Second stage: what is only an icon or shrunk now goes into the menu, in the same order.
  if (!fits()) {
    for (const c of [...giving].sort(byOrder)) {
      if (fits()) break
      const state = states.get(c.id)
      if (
        state === 'icon' ||
        (state !== undefined && typeof state === 'object')
      )
        states.set(c.id, 'overflow')
    }
  }

  // Third stage, the safety net: the `keep` controls, in the order of the places.
  if (!fits()) {
    for (const c of [...keeping].sort((a, b) => rank(a) - rank(b))) {
      if (fits()) break
      states.set(c.id, 'overflow')
    }
  }

  return { states, menuVisible: menuVisible() }
}
