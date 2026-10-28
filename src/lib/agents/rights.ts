// The rights of an external agent as the window edits them (spec 021 FR-008, research R3): per
// scope of "holzi bedienen" nothing, reading, or reading and changing. Rust checks every change
// again (`grants::validate`). Pure, relative `.ts` imports only.
import type { OperateLevel } from '../../types/bindings/OperateLevel.ts'
import type { Rights } from '../../types/bindings/Rights.ts'

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

/** The rights the levels stand for; "none" is left out. */
export function rightsOf(levels: Levels): Rights {
  const operate: Record<string, OperateLevel> = {}
  for (const scope of OPERATE_SCOPES) {
    const level = levels[scope]
    if (level !== 'none') operate[scope] = level
  }
  return { operate }
}

/** How many scopes an agent may read and how many it may change, for a summary. */
export function rightsCount(rights: Rights): { read: number; write: number } {
  const levels = Object.values(rights.operate)
  return {
    read: levels.filter((level) => level === 'read').length,
    write: levels.filter((level) => level === 'write').length,
  }
}
