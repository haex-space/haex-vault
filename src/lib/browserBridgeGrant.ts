// The grant the user picks for a browser extension (spec 049, FR-010): shared by the approval
// question and the editor in the settings. Rust checks every grant again (`approvals::validate`):
// never wider than declared, a tag scope with tags, and for reading and writing in a tag scope a
// default tag inside it.
import type { BridgeAccess } from '~/types/bindings/BridgeAccess'

/** The fields while the user picks; `scope: 'tags'` may hold no tag yet. */
export type GrantDraft = {
  access: BridgeAccess
  scope: 'all' | 'tags'
  tags: string[]
  /** `''`: none. */
  defaultTag: string
}

export function draftOf(
  access: BridgeAccess,
  scopeTags: string[] | null,
  defaultTag: string | null,
): GrantDraft {
  const tags = [...(scopeTags ?? [])]
  return {
    access,
    scope: scopeTags ? 'tags' : 'all',
    tags,
    defaultTag: defaultTag ?? (scopeTags ? (tags[0] ?? '') : ''),
  }
}

export function needsDefaultTag(draft: GrantDraft): boolean {
  return draft.access === 'readWrite' && draft.scope === 'tags'
}

/** Whether Rust would take the draft. */
export function grantComplete(draft: GrantDraft): boolean {
  if (draft.scope === 'tags' && draft.tags.length === 0) return false
  if (needsDefaultTag(draft)) return draft.tags.includes(draft.defaultTag)
  return true
}

/** What the commands take. */
export function grantOf(draft: GrantDraft): {
  access: BridgeAccess
  scopeTags: string[] | null
  defaultTag: string | null
} {
  return {
    access: draft.access,
    scopeTags: draft.scope === 'tags' ? [...draft.tags] : null,
    defaultTag: draft.defaultTag || null,
  }
}
