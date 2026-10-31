// The rights of an external agent as the window edits them (spec 021 FR-008, research R3): per
// scope of "holzi bedienen" nothing, reading, or reading and changing, the group `browser` with
// its websites (spec 065), the extensions whose tools it sees (spec 021 US4) and the group
// `passwords` with its tags (US5). Rust checks every change again (`grants::validate`). Pure, relative
// `.ts` imports only; the shapes below are those of the bindings `Rights` and `BrowserRights`,
// restated because the scripts' Node resolution cannot load bindings that import other bindings.
import type { OperateLevel } from '../../types/bindings/OperateLevel.ts'
import type { SiteLevel } from '../../types/bindings/SiteLevel.ts'

/** `BrowserRights` of the bindings. */
export type BrowserRights = {
  userBrowser: boolean
  sites: { [site in string]: { level: SiteLevel; fillRemembered: boolean } }
}

/** `PasswordRights` of the bindings; `tags: null` reaches every entry. */
export type PasswordRights = {
  level: 'read' | 'readWrite'
  tags: string[] | null
}

/** `Rights` of the bindings. */
export type Rights = {
  operate: { [scope in string]: OperateLevel }
  browser?: BrowserRights
  passwords?: PasswordRights
  extensions?: string[]
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

/** One level per scope; a scope the rights leave out is "none". */
export function levelsOf(rights: Rights | null | undefined): Levels {
  return Object.fromEntries(
    OPERATE_SCOPES.map((scope) => [scope, rights?.operate[scope] ?? 'none']),
  ) as Levels
}

/** The rights the levels, the groups `browser` and `passwords` and the granted extensions stand
 * for; "none" is left out, and `null` keeps a group off. An incomplete password tag draft
 * also keeps the group off, so changing from all entries to tags immediately revokes access. */
export function rightsOf(
  levels: Levels,
  browser: BrowserRights | null = null,
  extensions: string[] = [],
  passwords: PasswordRights | null = null,
): Rights {
  const operate: Record<string, OperateLevel> = {}
  for (const scope of OPERATE_SCOPES) {
    const level = levels[scope]
    if (level !== 'none') operate[scope] = level
  }
  return {
    operate,
    ...(browser ? { browser } : {}),
    ...(passwords &&
    (passwords.tags === null || passwords.tags.some((tag) => tag.trim()))
      ? { passwords }
      : {}),
    ...(extensions.length > 0 ? { extensions } : {}),
  }
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

/** How many scopes an agent may read and how many it may change, for a summary. */
export function rightsCount(rights: Rights): { read: number; write: number } {
  const levels = Object.values(rights.operate)
  return {
    read: levels.filter((level) => level === 'read').length,
    write: levels.filter((level) => level === 'write').length,
  }
}
