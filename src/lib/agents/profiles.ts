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
 * The language key of what kind of agent something is: "Externer Agent", "Abo-Agent" (a delegate
 * that works through a subscription) or "Lokaler Agent" (a model of the chat). Takes the kind of
 * an agent (`AgentKind`) or of a subject (`SubjectKind`).
 */
export function kindLabelKey(kind: string): string {
  switch (kind) {
    case 'external':
      return 'agentProfiles.kind.external'
    case 'builtin':
      return 'agentProfiles.kind.builtin'
    default:
      return 'agentProfiles.kind.subscription'
  }
}
