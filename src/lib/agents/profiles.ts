// Agent profiles (spec 083): labels and the error codes of the profile commands. A change of a
// profile's rights asks nothing, however much it gives (FR-016). Pure, relative `.ts` imports only.

/** The codes Rust names in the reason of a refused profile command (`agent_profiles::commands`). */
export const PROFILE_ERROR_CODES = [
  'name_empty',
  'standard_fixed',
  'profile_missing',
  'subject_unknown',
] as const

/** The code of a refused profile command, or `null` for any other failure. */
export function profileErrorCode(error: unknown): string | null {
  if (!error || typeof error !== 'object') return null
  const { kind, reason } = error as { kind?: unknown; reason?: unknown }
  if (kind !== 'InvalidInput' || typeof reason !== 'string') return null
  return (PROFILE_ERROR_CODES as readonly string[]).includes(reason)
    ? reason
    : null
}

/** The id of "Standard" (Rust `STANDARD_PROFILE_ID`); the page shows its translated label. */
export const STANDARD_PROFILE_ID = '61705f70-726f-6669-6c65-5f7374640083'

/** The label of a profile: "Standard" is translated by its id, any other shows its name. */
export function profileLabel(
  t: (key: string) => string,
  id: string,
  name: string,
): string {
  return id === STANDARD_PROFILE_ID ? t('agentProfiles.standard') : name
}

/**
 * The language key of what kind of agent something is: "Externer Agent" or "Lokaler Agent" (a model
 * of the chat, whichever provider it comes from, an Abo-Agent's included, spec 083 FR-027). Takes
 * the kind of an agent (`AgentKind`) or of a subject (`SubjectKind`).
 */
export function kindLabelKey(kind: string): string {
  switch (kind) {
    case 'external':
      return 'agentProfiles.kind.external'
    case 'builtin':
      return 'agentProfiles.kind.builtin'
    case 'provider':
      return 'agentProfiles.kind.provider'
    default:
      return 'agentProfiles.kind.external'
  }
}

/** The slice of a subject the grouping reads (`AgentSubjectView` of the bindings). */
export type Groupable = {
  subject: string
  kind: string
  name: string
  local: boolean
  provider: string | null
  providerSubject: string | null
}

/** A provider's models, in one accordion; `provider` is `null` for the local models. */
export type ModelGroup<T> = {
  key: string
  provider: string | null
  /** The provider as a subject, when it is listed: what a membership of the whole provider is. */
  providerSubject: T | null
  items: T[]
}

/**
 * The models of `subjects` by provider: the local models first, then each provider by name. A
 * provider that is a subject itself (a member of a profile) heads its group instead of being a row.
 * What is no model of a provider (an agent, a model that is gone) is `rest`, in its order.
 */
export function groupByProvider<T extends Groupable>(
  subjects: readonly T[],
): { groups: ModelGroup<T>[]; rest: T[] } {
  const local: T[] = []
  const byProvider = new Map<string, { label: string; items: T[] }>()
  const heads = new Map<string, T>()
  const rest: T[] = []
  for (const subject of subjects) {
    if (subject.kind === 'provider') heads.set(subject.subject, subject)
    else if (subject.kind !== 'builtin') rest.push(subject)
    else if (subject.local) local.push(subject)
    else if (subject.providerSubject) {
      const group = byProvider.get(subject.providerSubject) ?? {
        label: subject.provider ?? subject.providerSubject,
        items: [],
      }
      group.items.push(subject)
      byProvider.set(subject.providerSubject, group)
    } else if (subject.provider) {
      const key = `name:${subject.provider}`
      const group = byProvider.get(key) ?? {
        label: subject.provider,
        items: [],
      }
      group.items.push(subject)
      byProvider.set(key, group)
    } else rest.push(subject)
  }
  const headOf = (items: T[]): T | null => {
    const key = items.find((item) => item.providerSubject)?.providerSubject
    return (key ? heads.get(key) : null) ?? null
  }
  const groups: ModelGroup<T>[] = []
  const localHead = [...heads.values()].find((head) => head.local) ?? null
  if (local.length > 0 || localHead) {
    groups.push({
      key: '__local',
      provider: null,
      providerSubject: headOf(local) ?? localHead,
      items: local,
    })
  }
  for (const [subject, head] of heads) {
    if (head.local || byProvider.has(subject)) continue
    byProvider.set(subject, { label: head.name, items: [] })
  }
  for (const [key, group] of [...byProvider.entries()].sort(
    (a, b) => a[1].label.localeCompare(b[1].label) || a[0].localeCompare(b[0]),
  )) {
    groups.push({
      key,
      provider: group.label,
      providerSubject: heads.get(key) ?? null,
      items: group.items,
    })
  }
  return { groups, rest }
}

/** The name of a model inside its provider's group: the provider needs no repeating there. */
export function modelName(subject: Groupable): string {
  const suffix = subject.provider ? ` (${subject.provider})` : ''
  return suffix && subject.name.endsWith(suffix)
    ? subject.name.slice(0, -suffix.length)
    : subject.name
}
