// A system app's header description is trusted code, so nothing here rejects it; a development
// build only warns about what is obviously wrong (spec 090-app-header-bar, research R6,
// contracts/header-description.md §System apps). Relative `.ts` imports, so Node loads it.
import type { WmHeaderControl } from '../../types/bindings/WmHeaderControl.ts'
import type { WmHeaderDescription } from '../../types/bindings/WmHeaderDescription.ts'

/** The most controls a description may hold (contracts/header-description.md check 3). */
export const MAX_CONTROLS = 32

/** Controls whose accessible name is their `label` (FR-009); `title` has its text instead. */
const NEEDS_LABEL = new Set([
  'button',
  'toggle',
  'search',
  'menu',
  'choice',
  'breadcrumb',
])

/** What is wrong with a description, as messages for the console; empty when nothing is. */
export function headerProblems(description: WmHeaderDescription): string[] {
  const controls: WmHeaderControl[] = [
    ...(description.start ?? []),
    ...(description.center ?? []),
    ...(description.end ?? []),
  ]
  const problems: string[] = []
  if (controls.length > MAX_CONTROLS)
    problems.push(`${controls.length} controls, at most ${MAX_CONTROLS}`)

  const ids = new Set<string>()
  const add = (id: string) => {
    if (ids.has(id)) problems.push(`id "${id}" is used twice`)
    ids.add(id)
  }
  for (const control of controls) {
    add(control.id)
    if (NEEDS_LABEL.has(control.type) && !control.label)
      problems.push(`control "${control.id}" has no label`)
    if (control.type !== 'menu' && 'active' in control)
      problems.push(`control "${control.id}": active is only for a menu`)
    if (control.type === 'menu')
      for (const item of control.items) if (item.type === 'item') add(item.id)
    if (control.type === 'choice') for (const o of control.options) add(o.id)
    if (control.type === 'breadcrumb')
      for (const s of control.segments) add(s.id)
  }
  return problems
}
