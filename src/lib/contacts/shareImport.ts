// The choices when a share is read in (spec 060 FR-024 to FR-025): the default of every item, what
// a change of choice does, what may be accepted and what the accepting would do. Defaults add what
// is new and replace nothing. Pure, so `scripts/check-contacts-share.ts` runs it without vue.

// The shapes of the generated bindings (`ContactSharePreview`, `ContactShareDecision`) as far as
// the choices use them; declared here so the check script loads this file without the bindings.
export type ImportChoice = 'add' | 'update' | 'skip' | 'keep' | 'replace'

export type ImportExisting = { id: string }

export type ImportItem = {
  id: number
  item: { type: string; field?: string }
  state: 'new' | 'same' | 'conflict'
  choices: ImportChoice[]
  default?: ImportChoice | null
  existing: ImportExisting[]
}

export type ImportPreview = {
  target: { kind: 'new' } | { kind: 'contact'; id: string }
  identity: { kind: 'none' | 'new' | 'matches' | 'mismatch' }
  items: ImportItem[]
  blocked: boolean
}

/** What the person chose for one item; `valueId` names the value an `update` replaces. */
export type ImportDecision = {
  item: number
  choice: ImportChoice
  valueId?: string
}

export type Decisions = ReadonlyMap<number, ImportDecision>

/** The default of every item that has a choice: add what is new, keep what is there. */
export function defaultDecisions(preview: ImportPreview): Decisions {
  const decisions = new Map<number, ImportDecision>()
  for (const item of preview.items) {
    if (item.choices.length === 0 || !item.default) continue
    decisions.set(item.id, { item: item.id, choice: item.default })
  }
  return decisions
}

/** `choice` for an item; an update of a value starts at the first value it could replace. */
export function choose(
  preview: ImportPreview,
  decisions: Decisions,
  itemId: number,
  choice: ImportChoice,
): Decisions {
  const item = preview.items.find((candidate) => candidate.id === itemId)
  if (!item || !item.choices.includes(choice)) return decisions
  const next = new Map(decisions)
  next.set(itemId, {
    item: itemId,
    choice,
    ...(choice === 'update' && item.existing.length > 0
      ? { valueId: item.existing[0]!.id }
      : {}),
  })
  return next
}

/** The value an update replaces. */
export function replacing(
  decisions: Decisions,
  itemId: number,
  valueId: string,
): Decisions {
  const current = decisions.get(itemId)
  if (current?.choice !== 'update') return decisions
  const next = new Map(decisions)
  next.set(itemId, { ...current, valueId })
  return next
}

export type ImportProblem = 'blocked' | 'mismatch' | 'needsValue'

/** Why the share cannot be accepted as chosen, or `null`. */
export function validate(
  preview: ImportPreview,
  decisions: Decisions,
): ImportProblem | null {
  if (preview.blocked) return 'blocked'
  if (
    preview.target.kind === 'contact' &&
    preview.identity.kind === 'mismatch'
  ) {
    return 'mismatch'
  }
  for (const decision of decisions.values()) {
    const item = preview.items.find(
      (candidate) => candidate.id === decision.item,
    )
    if (
      decision.choice === 'update' &&
      item &&
      item.existing.length > 0 &&
      !decision.valueId
    ) {
      return 'needsValue'
    }
  }
  return null
}

/** The decisions for the backend: only items that have a choice. */
export function toArgs(
  preview: ImportPreview,
  decisions: Decisions,
): ImportDecision[] {
  return preview.items
    .filter((item) => item.choices.length > 0)
    .flatMap((item) => {
      const decision = decisions.get(item.id)
      return decision ? [decision] : []
    })
}

export type ImportSummary = {
  added: number
  updated: number
  skipped: number
  kept: number
}

/** What accepting would do, for the line above the button. Items that are the same, or that the
 * target fills without asking, are not counted. */
export function summary(
  preview: ImportPreview,
  decisions: Decisions,
): ImportSummary {
  const counts: ImportSummary = { added: 0, updated: 0, skipped: 0, kept: 0 }
  for (const item of preview.items) {
    if (item.choices.length === 0) continue
    const choice = decisions.get(item.id)?.choice ?? item.default
    if (choice === 'add' || choice === 'replace') counts.added += 1
    else if (choice === 'update') counts.updated += 1
    else if (choice === 'skip') counts.skipped += 1
    else if (choice === 'keep') counts.kept += 1
  }
  return counts
}
