// The models an extension may use (spec 055 US2): the effort levels the user can allow, the rank
// of the granted models and the labels of a model's place; the permissions list leaves them out.
// Pure: the question of a job (`components/extensions/PermissionRequestDialog.vue`) and the
// settings (`components/settings/extensions/ExtensionModelsView.vue`, `ExtensionPermissionsView`)
// use it, `scripts/check-extensions-model-grants.ts` tests it. Which models fit a job holzi
// decides; the question only shows the ones it offers.

export const MODEL_KIND = 'model'

/** The permissions the settings list apart from the models, which have their own list. */
export function withoutModels<T extends { kind: string }>(
  rows: readonly T[],
): T[] {
  return rows.filter((row) => row.kind !== MODEL_KIND)
}

/** The efforts a grant can allow, lowest first. */
export const EFFORT_LEVELS = ['none', 'low', 'medium', 'high'] as const
export type EffortLevel = (typeof EFFORT_LEVELS)[number]

export function isEffortLevel(value: string): value is EffortLevel {
  return (EFFORT_LEVELS as readonly string[]).includes(value)
}

/** The ceiling a question starts with: the effort the job wished for, or `none`. */
export function proposedEffort(action: string): EffortLevel {
  return isEffortLevel(action) ? action : 'none'
}

/** The i18n key of an effort level. */
export function effortLabelKey(level: string): string {
  return `extensions.models.effort.${level}`
}

/** The i18n key of where a model runs (`this_device`, `network`, `cloud`). */
export function placeLabelKey(place: string): string {
  return `extensions.models.place.${place}`
}

/** A model as the settings list it; only the fields the rank needs. */
export interface RankedModel {
  id: string
  ceiling?: string
}

/** The ids of the models the extension may use, in its rank (the list comes in that order). */
export function rankOf(models: readonly RankedModel[]): string[] {
  return models.filter((m) => m.ceiling !== undefined).map((m) => m.id)
}

/**
 * The rank after moving `id` one place up (`-1`) or down (`1`). At an edge, or for an id that is
 * not ranked, it stays as it is.
 */
export function moved(
  rank: readonly string[],
  id: string,
  delta: -1 | 1,
): string[] {
  const from = rank.indexOf(id)
  const to = from + delta
  if (from < 0 || to < 0 || to >= rank.length) return [...rank]
  const next = [...rank]
  next[from] = rank[to] as string
  next[to] = id
  return next
}
