// The models an extension may use (spec 055 US2): the effort levels the user can allow, the groups
// of the models by provider and the labels of a model's place; the permissions list leaves them out.
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

/** The efforts the user can choose, lowest first; a model offers the ones it has (`efforts`). */
export const EFFORT_LEVELS = [
  'none',
  'low',
  'medium',
  'high',
  'xhigh',
  'max',
] as const
export type EffortLevel = (typeof EFFORT_LEVELS)[number]

export function isEffortLevel(value: string): value is EffortLevel {
  return (EFFORT_LEVELS as readonly string[]).includes(value)
}

const byLevel = (a: string, b: string) =>
  EFFORT_LEVELS.indexOf(a as EffortLevel) -
  EFFORT_LEVELS.indexOf(b as EffortLevel)

/**
 * The efforts to choose from for a model that offers `efforts`: its own levels, lowest first.
 * A model without a choice of effort has only `none`, which then means "allowed"; the effort
 * the model has now stays in the list even when the model no longer offers it.
 */
export function ceilingChoices(
  efforts: readonly string[],
  current?: string,
): string[] {
  const choices = efforts.length > 0 ? [...efforts] : ['none']
  if (current && isEffortLevel(current) && !choices.includes(current))
    choices.push(current)
  return choices.sort(byLevel)
}

/**
 * The effort a question starts with for a model, which the user changes: `medium` when the model
 * has it, else the middle one of its own levels (the upper of two); `none` for a model without a
 * choice of effort. The extension has no say (spec 055 FR-036).
 */
export function startingEffort(efforts: readonly string[]): string {
  const offered = [...efforts].sort(byLevel)
  if (offered.length === 0) return 'none'
  if (offered.includes('medium')) return 'medium'
  return offered[Math.floor(offered.length / 2)] as string
}

/** The i18n key of an effort level. */
export function effortLabelKey(level: string): string {
  return `extensions.models.effort.${level}`
}

/** The i18n key of where a model runs (`this_device`, `network`, `cloud`). */
export function placeLabelKey(place: string): string {
  return `extensions.models.place.${place}`
}

/** A model as the settings group it; only the fields the grouping needs. */
export interface GroupableModel {
  provider: string
  place: string
}

/** A provider's models, in one accordion; `provider` is `null` for the models of this device. */
export interface ProviderModels<T> {
  key: string
  provider: string | null
  items: T[]
}

/**
 * The models by provider: the models of this device first, then each provider by name; inside a
 * group the models keep the order they come in.
 */
export function groupModelsByProvider<T extends GroupableModel>(
  models: readonly T[],
): ProviderModels<T>[] {
  const local: T[] = []
  const byProvider = new Map<string, T[]>()
  for (const model of models) {
    if (model.place === 'this_device') local.push(model)
    else
      byProvider.set(model.provider, [
        ...(byProvider.get(model.provider) ?? []),
        model,
      ])
  }
  const groups: ProviderModels<T>[] = []
  if (local.length > 0)
    groups.push({ key: '__local', provider: null, items: local })
  for (const [provider, items] of [...byProvider].sort((a, b) =>
    a[0].localeCompare(b[0]),
  ))
    groups.push({ key: `name:${provider}`, provider, items })
  return groups
}

/** A model as the group choice reads it. */
export interface GrantableModel {
  id: string
  efforts: readonly string[]
  agent: boolean
  ceiling?: string
}

/** What the choice for a whole group of models would change. */
export interface GroupChoice {
  /** How many of the models the extension may use now. */
  allowed: number
  total: number
  /** The models that are agents with access to files and commands, which "allow all" would grant. */
  agents: number
}

export function groupChoice(models: readonly GrantableModel[]): GroupChoice {
  return {
    allowed: models.filter((m) => m.ceiling !== undefined).length,
    total: models.length,
    agents: models.filter((m) => m.agent && m.ceiling === undefined).length,
  }
}

/**
 * The effort a model gets when a whole group is allowed at once: its own starting effort
 * (`startingEffort`), since the models of a group do not offer the same efforts. A model that is
 * allowed already keeps what the user chose for it.
 */
export function allowGroup(
  models: readonly GrantableModel[],
): { modelId: string; effort: string }[] {
  return models
    .filter((m) => m.ceiling === undefined)
    .map((m) => ({ modelId: m.id, effort: startingEffort(m.efforts) }))
}
