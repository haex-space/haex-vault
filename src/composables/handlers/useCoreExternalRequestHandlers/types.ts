export const CORE_REQUEST_EVENT = 'haextension:external:core-request'

export const CORE_METHODS = {
  GET_ITEMS: 'get-items',
  GET_TOTP: 'get-totp',
  CREATE_ITEM: 'create-item',
  UPDATE_ITEM: 'update-item',
  GET_PASSWORD_CONFIG: 'get-password-config',
  GET_PASSWORD_PRESETS: 'get-password-presets',
  PASSKEY_CREATE: 'passkey-create',
  PASSKEY_GET: 'passkey-get',
  PASSKEY_LIST: 'passkey-list',
} as const

/**
 * Mirrors the Rust `PasswordsScope` enum (see
 * `extension::permissions::types::PasswordsScope`) — the tag scope resolved
 * by `check_passwords_permission` for this request. The Read/Write boundary
 * itself is already enforced in Rust before this event is emitted; this is
 * the tag-level refinement within that boundary.
 */
export type PasswordsScope =
  | { type: 'all' }
  | { type: 'tags', tags: string[], default: string | null }

export interface ExternalCoreRequest {
  requestId: string
  publicKey: string
  action: string
  payload: Record<string, unknown>
  extensionPublicKey: string
  extensionName: string
  /** Present for every core (passwords) request; absent/undefined is treated as `all`. */
  scope?: PasswordsScope | null
}

export interface ExternalCoreResponse {
  requestId: string
  success: boolean
  data?: unknown
  error?: string
}

export interface GetItemsPayload {
  url?: string
  fields?: string[]
}

export interface ItemEntry {
  id: string
  title: string
  url: string | null
  fields: Record<string, string>
  hasTotp: boolean
  autofillAliases?: Record<string, string[]> | null
}

export interface GetTotpPayload {
  entryId?: string
}

export type OtpAlgorithm = 'SHA1' | 'SHA256' | 'SHA512'

export interface CreateItemPayload {
  url?: string
  title?: string
  username?: string
  password?: string
  groupId?: string | null
  otpSecret?: string | null
  otpDigits?: number | null
  otpPeriod?: number | null
  otpAlgorithm?: string | null
  iconBase64?: string | null
}

export interface UpdateItemPayload {
  id: string
  url?: string
  title?: string
  username?: string
  password?: string
  otpSecret?: string | null
  otpDigits?: number | null
  otpPeriod?: number | null
  otpAlgorithm?: string | null
  iconBase64?: string | null
}

export interface PasskeyCreatePayload {
  relyingPartyId: string
  relyingPartyName: string
  userHandle: string
  userName: string
  userDisplayName?: string
  challenge: string
  excludeCredentials?: string[]
  requireResidentKey?: boolean
  userVerification?: 'required' | 'preferred' | 'discouraged'
  itemId?: string
}

export interface PasskeyGetPayload {
  relyingPartyId: string
  challenge: string
  allowCredentials?: Array<{
    id: string
    type: 'public-key'
    transports?: string[]
  }>
  userVerification?: 'required' | 'preferred' | 'discouraged'
}

export interface PasskeyListPayload {
  relyingPartyId?: string
  itemId?: string
  discoverableOnly?: boolean
}
