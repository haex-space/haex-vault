// The rights of an external agent as the window edits them (spec 021 FR-008, research R3): per
// scope of "holzi bedienen" nothing, reading, or reading and changing, the group `browser` with
// its websites (spec 065), the extensions whose tools it sees (spec 021 US4), the group
// `passwords` with its tags and the group `commands` (US5). Rust checks every change again
// (`grants::validate`). Pure, relative `.ts` imports only; the shapes below are those of the
// bindings `Rights` and `BrowserRights`, restated because the scripts' Node resolution cannot load
// bindings that import other bindings.
import type { OperateLevel } from '../../types/bindings/OperateLevel.ts'
import type { SiteLevel } from '../../types/bindings/SiteLevel.ts'

/** `BrowserRights` of the bindings. */
export type BrowserRights = {
  userBrowser: boolean
  sites: { [site in string]: { level: SiteLevel; fillRemembered: boolean } }
  /** Rules for the requests a page sends to a website (spec 065 FR-044). */
  requests: { [site in string]: 'allow' | 'deny' }
  /** What the agent may do on every website not listed (spec 065 FR-043); none when absent. */
  all?: SiteLevel | null
}

/** `PasswordRights` of the bindings; `tags: null` reaches every entry. */
export type PasswordRights = {
  level: 'read' | 'readWrite'
  tags: string[] | null
  /** The tag new entries get (spec 065 FR-036); within chosen tags one of them. */
  defaultTag?: string | null
}

/** The autonomy modes of an Abo-Agent's own tools (spec 009), `AutonomyMode` of the bindings. */
export const AUTONOMY_MODES = [
  'standard',
  'ungated',
  'gated_permissive',
] as const
export type AutonomyMode = (typeof AUTONOMY_MODES)[number]

/** What the permissive mode still refuses (spec 009), `DenyCategory` of the bindings. */
export const DENY_CATEGORIES = [
  'workspace_escape',
  'network_access',
  'credential_paths',
] as const
export type DenyCategory = (typeof DENY_CATEGORIES)[number]

/** `DelegateRights` of the bindings: how an Abo-Agent's own tools run (spec 083 FR-030). */
export type DelegateRights = {
  autonomy: AutonomyMode
  deny: DenyCategory[]
}

/** `AccessLevel` of the bindings. */
export type AccessLevel = 'read' | 'readWrite' | 'denied'

/** `ContactsRights` of the bindings: what an agent reaches in the address book (spec 083 FR-032).
 * Without an entry the agent is asked at its first access. */
export type ContactsRights = {
  all?: AccessLevel
  /** By the id of the contact group. */
  groups?: { [group in string]: AccessLevel }
}

/** `FolderRight` of the bindings: how far, and whether cloud models reach the folder too. */
export type FolderRight = { level: AccessLevel; cloud?: boolean }

/** `FilesRights` of the bindings: what an agent may do with files (spec 083 FR-033). The device
 * is off without an entry; a storage or an encrypted folder without an entry asks at the first
 * access. */
export type FilesRights = {
  device?: boolean
  /** By storage id. */
  storages?: { [storage in string]: AccessLevel }
  /** By `<storage id>/<folder id>`. */
  folders?: { [folder in string]: FolderRight }
  fileShares?: boolean
  syncRules?: boolean
}

/** `Rights` of the bindings. */
export type Rights = {
  operate: { [scope in string]: OperateLevel }
  browser?: BrowserRights
  passwords?: PasswordRights
  extensions?: string[]
  /** "Alle Erweiterungen" (spec 083 plan Q3): every extension's tools, a later one too. */
  allExtensions?: boolean
  commands?: boolean
  /** How an Abo-Agent's own tools run; absent asks for every call (spec 083 FR-030). */
  delegate?: DelegateRights
  contacts?: ContactsRights
  files?: FilesRights
}

/** The scopes of "holzi bedienen" (Rust `grants::OPERATE_SCOPES`), in the order they are shown. */
export const OPERATE_SCOPES = [
  'wm.read',
  'wm.navigation',
  'wm.layout',
  'chat.read',
  'chat.write',
  'settings.read',
  'settings.device',
  'settings.models',
] as const

export type OperateScope = (typeof OPERATE_SCOPES)[number]
export type Level = OperateLevel | 'none'
export type Levels = Record<OperateScope, Level>

/**
 * What the window shows of "holzi bedienen": one row per topic, not per scope. The row's level
 * stands for every scope of the group, so "Chat" with "Lesen und ändern" is `chat.read` and
 * `chat.write` at `write`. The stored rights stay per scope; Rust knows no groups.
 */
export const OPERATE_GROUPS = [
  { id: 'wm', scopes: ['wm.read', 'wm.navigation', 'wm.layout'] },
  { id: 'chat', scopes: ['chat.read', 'chat.write'] },
  { id: 'settings', scopes: ['settings.read', 'settings.device'] },
  { id: 'models', scopes: ['settings.models'] },
] as const satisfies ReadonlyArray<{
  id: string
  scopes: readonly OperateScope[]
}>

export type OperateGroup = (typeof OPERATE_GROUPS)[number]

const LEVEL_ORDER: Record<Level, number> = { none: 0, read: 1, write: 2 }

/** The level a group shows: the widest of its scopes, so nothing granted stays hidden. Rights
 * edited per scope before the groups can differ inside one group until the user picks a level. */
export function groupLevel(
  levels: Partial<Record<OperateScope, Level>>,
  group: OperateGroup,
): Level {
  let widest: Level = 'none'
  for (const scope of group.scopes) {
    const level = levels[scope] ?? 'none'
    if (LEVEL_ORDER[level] > LEVEL_ORDER[widest]) widest = level
  }
  return widest
}

/** The levels with every scope of the group set to `level`. */
export function withGroupLevel(
  levels: Levels,
  group: OperateGroup,
  level: Level,
): Levels {
  const next = { ...levels }
  for (const scope of group.scopes) next[scope] = level
  return next
}

/** One level per scope; a scope the rights leave out is "none". */
export function levelsOf(rights: Rights | null | undefined): Levels {
  return Object.fromEntries(
    OPERATE_SCOPES.map((scope) => [scope, rights?.operate[scope] ?? 'none']),
  ) as Levels
}

/** The rights as the window edits them, one value per group; `null` keeps a group off. */
export type RightsDraft = {
  levels: Levels
  browser: BrowserRights | null
  extensions: string[]
  allExtensions: boolean
  passwords: PasswordRights | null
  commands: boolean
  delegate: DelegateRights | null
  contacts: ContactsRights
  files: FilesRights
}

/** The draft of stored rights; `null` stands for an agent without any. */
export function rightsDraftOf(rights: Rights | null | undefined): RightsDraft {
  return {
    levels: levelsOf(rights),
    browser: rights?.browser ?? null,
    extensions: rights?.extensions ?? [],
    allExtensions: rights?.allExtensions ?? false,
    passwords: rights?.passwords ?? null,
    commands: rights?.commands ?? false,
    delegate: rights?.delegate ?? null,
    contacts: rights?.contacts ?? {},
    files: rights?.files ?? {},
  }
}

/** Whether the group `passwords` names what it reaches: off, every entry or at least one tag;
 * writing within chosen tags also names the one of them new entries get. */
export function passwordsComplete(passwords: PasswordRights | null): boolean {
  if (!passwords || passwords.tags === null) return true
  const tags = passwords.tags
  if (!tags.some((tag) => tag.trim())) return false
  return (
    passwords.level !== 'readWrite' ||
    (!!passwords.defaultTag && tags.includes(passwords.defaultTag))
  )
}

/** The rights a draft stands for; "none" and groups that are off are left out. An incomplete
 * password tag draft also keeps the group off, so changing from all entries to tags immediately
 * revokes access. */
export function rightsOf(draft: RightsDraft): Rights {
  const operate: Record<string, OperateLevel> = {}
  for (const scope of OPERATE_SCOPES) {
    const level = draft.levels[scope]
    if (level !== 'none') operate[scope] = level
  }
  const {
    browser,
    passwords,
    extensions,
    allExtensions,
    commands,
    delegate,
    contacts,
    files,
  } = draft
  return {
    operate,
    ...(browser ? { browser } : {}),
    ...(passwords && passwordsComplete(passwords) ? { passwords } : {}),
    ...(extensions.length > 0 ? { extensions } : {}),
    ...(allExtensions ? { allExtensions } : {}),
    ...(commands ? { commands } : {}),
    ...(delegate ? { delegate } : {}),
    ...(contactsSet(contacts) ? { contacts } : {}),
    ...(filesSet(files) ? { files } : {}),
  }
}

/** Whether the files group names anything. */
export function filesSet(files: FilesRights): boolean {
  return (
    !!files.device ||
    !!files.fileShares ||
    !!files.syncRules ||
    Object.keys(files.storages ?? {}).length > 0 ||
    Object.keys(files.folders ?? {}).length > 0
  )
}

/** Whether the contacts group names anything. */
export function contactsSet(contacts: ContactsRights): boolean {
  return (
    contacts.all !== undefined || Object.keys(contacts.groups ?? {}).length > 0
  )
}

/**
 * The website of what the user typed, written as Rust's `agent_browser::sites::Site` writes it:
 * scheme, host in lower case (IDN as punycode) and a port only when it is not the default. Without
 * a scheme `https://` is assumed. `null` for anything that is no https or http website, or that
 * carries a user name or password.
 */
export function siteOf(text: string): string | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  let url: URL
  try {
    url = new URL(trimmed.includes('://') ? trimmed : `https://${trimmed}`)
  } catch {
    return null
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') return null
  if (url.username || url.password || !url.hostname) return null
  return url.origin
}

/** The i18n key of the name of the topic `scope` belongs to (its group in "holzi bedienen"), or
 * `null` for a scope that is in none. */
export function topicKeyOf(scope: string): string | null {
  const group = OPERATE_GROUPS.find((candidate) =>
    (candidate.scopes as readonly string[]).includes(scope),
  )
  return group ? `mcpAgents.rights.groups.${group.id}.title` : null
}

/** How many topics (`OPERATE_GROUPS`) an agent may read and how many it may change, for a
 * summary. */
export function rightsCount(rights: Rights): { read: number; write: number } {
  const levels = OPERATE_GROUPS.map((group) =>
    groupLevel(rights.operate as Partial<Record<OperateScope, Level>>, group),
  )
  return {
    read: levels.filter((level) => level === 'read').length,
    write: levels.filter((level) => level === 'write').length,
  }
}
