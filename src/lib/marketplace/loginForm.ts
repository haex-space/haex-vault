// The login part of the marketplace form (spec 069 US5, FR-026, FR-027) as pure rules: what
// `marketplace_save` gets for a login the user typed or chose, when the form has to ask for one,
// and what it refuses before anything is sent. A secret only lives in the draft until it is sent.
//
// The check scripts load this file without the `@bindings` alias and the bindings import each other
// without extensions, so the shapes of `MarketplaceInput` that matter here are declared locally.

type MarketplaceAuthType = 'none' | 'bearer' | 'basic'

/** The login fields of `MarketplaceInput`. */
export type LoginFields = {
  authType: MarketplaceAuthType
  credentialsItemId?: string
  newCredentials?: { username?: string; secret: string }
}

export type LoginDraft = {
  authType: MarketplaceAuthType
  /** The user asked for another login than the row has (also set while the row lacks one). */
  replace: boolean
  /** Typed in the form, or an entry of the password manager. */
  source: 'new' | 'entry'
  entryId: string
  /** Basic only. */
  username: string
  /** The token (bearer) or the password (basic). */
  secret: string
}

/** What the saved row has: its auth type and the entry it points at. */
export type KeptLogin = {
  authType: MarketplaceAuthType
  credentialsItemId?: string
}

export type LoginProblem = 'insecure' | 'entry' | 'username' | 'secret'

/** An `http://` address that is not on this device. The host refuses every `http` login in a
 * release build and takes loopback ones in a debug build only (the end-to-end suite), so the form
 * does not stop a loopback address and lets the host answer. */
export function isPlainHttp(baseUrl: string): boolean {
  const text = baseUrl.trim()
  if (!/^http:\/\//i.test(text)) return false
  try {
    const host = new URL(text).hostname
    return !(
      host === 'localhost' ||
      host === '[::1]' ||
      /^127(\.\d{1,3}){3}$/.test(host)
    )
  } catch {
    return true
  }
}

export function emptyLogin(): LoginDraft {
  return {
    authType: 'none',
    replace: false,
    source: 'new',
    entryId: '',
    username: '',
    secret: '',
  }
}

/** Whether the form has to take a login: a login is wanted and the saved row has none that fits,
 * or the user asked for another. */
export function needsLoginInput(
  draft: LoginDraft,
  kept: KeptLogin | null,
): boolean {
  if (draft.authType === 'none') return false
  return (
    draft.replace ||
    !kept?.credentialsItemId ||
    kept.authType !== draft.authType
  )
}

/** The reason the form must not send the login yet, or `null`. */
export function loginProblem(
  draft: LoginDraft,
  baseUrl: string,
  kept: KeptLogin | null,
): LoginProblem | null {
  if (draft.authType === 'none') return null
  if (isPlainHttp(baseUrl)) return 'insecure'
  if (!needsLoginInput(draft, kept)) return null
  if (draft.source === 'entry') return draft.entryId ? null : 'entry'
  if (draft.authType === 'basic' && draft.username.trim() === '')
    return 'username'
  return draft.secret === '' ? 'secret' : null
}

/** The login fields of the input of `marketplace_save` / `marketplace_test`. */
export function loginFields(
  draft: LoginDraft,
  kept: KeptLogin | null,
): LoginFields {
  const { authType } = draft
  if (authType === 'none') return { authType }
  if (!needsLoginInput(draft, kept))
    return { authType, credentialsItemId: kept?.credentialsItemId }
  if (draft.source === 'entry')
    return { authType, credentialsItemId: draft.entryId }
  return {
    authType,
    newCredentials: {
      username: authType === 'basic' ? draft.username.trim() : undefined,
      secret: draft.secret,
    },
  }
}

/** Forgets what was typed once it was sent. */
export function clearLoginSecrets(draft: LoginDraft) {
  draft.secret = ''
}
