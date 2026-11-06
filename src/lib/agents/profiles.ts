// Agent profiles (spec 083): whether a change of a profile's rights gives any agent in it more than
// it had, which the page confirms by naming how many agents it applies to (FR-016, research R11).
// Pure, relative `.ts` imports only.
import type { Rights } from './rights.ts'

/** The fields of `Rights` `widens` compares; `check-agent-profiles.ts` fails for one it misses. */
export const COMPARED_RIGHTS = [
  'operate',
  'browser',
  'passwords',
  'extensions',
  'allExtensions',
  'commands',
] as const satisfies ReadonlyArray<keyof Rights>

/** none < read < write, for a topic of "holzi bedienen". */
const OPERATE_RANK: Record<string, number> = { read: 1, write: 2 }

/** What a website allows, from refused to operating: a blocked website is refused, "none" asks. */
const SITE_RANK: Record<string, number> = {
  blocked: 0,
  none: 1,
  read: 2,
  operate: 3,
}

const siteRank = (level: string | null | undefined) =>
  SITE_RANK[level ?? 'none'] ?? 1

function operateWidens(before: Rights['operate'], after: Rights['operate']) {
  return Object.entries(after).some(
    ([scope, level]) =>
      (OPERATE_RANK[level] ?? 0) > (OPERATE_RANK[before[scope] ?? ''] ?? 0),
  )
}

function passwordsWidens(
  before: Rights['passwords'],
  after: Rights['passwords'],
) {
  if (!after) return false
  if (!before) return true
  if (after.level === 'readWrite' && before.level !== 'readWrite') return true
  // All entries reach more than any choice of tags; more tags reach more entries.
  if (before.tags !== null) {
    if (after.tags === null) return true
    if (after.tags.some((tag) => !before.tags?.includes(tag))) return true
  }
  // A tag new entries get that the profile did not name before.
  return !!after.defaultTag && after.defaultTag !== before.defaultTag
}

function browserWidens(before: Rights['browser'], after: Rights['browser']) {
  if (!after) return false
  if (!before) return true
  if (after.userBrowser && !before.userBrowser) return true
  const allBefore = before.all ?? null
  const allAfter = after.all ?? null
  if (siteRank(allAfter) > siteRank(allBefore)) return true
  const effective = (rights: typeof before, all: string | null, site: string) =>
    rights.sites[site]?.level ?? all
  for (const site of new Set([
    ...Object.keys(before.sites),
    ...Object.keys(after.sites),
  ])) {
    if (
      siteRank(effective(after, allAfter, site)) >
      siteRank(effective(before, allBefore, site))
    ) {
      return true
    }
    // A login fill that asks no more.
    if (
      after.sites[site]?.fillRemembered &&
      !before.sites[site]?.fillRemembered
    ) {
      return true
    }
  }
  // A request rule that is new at "allow", or turned to "allow".
  return Object.entries(after.requests).some(
    ([site, rule]) => rule === 'allow' && before.requests[site] !== 'allow',
  )
}

/**
 * Whether `after` gives anything `before` does not, over every field of `Rights`: a topic or an
 * extension or the commands that is new, a higher level, more entries of the password manager,
 * another website or a higher level for one (also one that was blocked), or a request rule that
 * allows. Giving less, or only moving between equal rights, is no widening.
 */
export function widens(before: Rights, after: Rights): boolean {
  return (
    operateWidens(before.operate, after.operate) ||
    passwordsWidens(before.passwords, after.passwords) ||
    browserWidens(before.browser, after.browser) ||
    (after.allExtensions === true && before.allExtensions !== true) ||
    (before.allExtensions !== true &&
      (after.extensions ?? []).some(
        (id) => !before.extensions?.includes(id),
      )) ||
    (after.commands === true && before.commands !== true)
  )
}

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
