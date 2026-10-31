// The error kinds the marketplace commands report (contracts/commands.md "Error kinds") and the
// language keys that carry their text. The lists are the one place of the spelling: the code
// takes its keys from here and `scripts/check-marketplace-i18n.ts` checks both languages.

/** `HolziError::Marketplace { reason }` of the download and the catalog commands. */
export const MARKETPLACE_REASONS = [
  'marketplace_invalid_url',
  'marketplace_credentials_required',
  'marketplace_insecure_credentials',
  'marketplace_not_found',
  'marketplace_no_hash',
  'marketplace_hash_mismatch',
  'marketplace_size_mismatch',
  'marketplace_too_large',
  'marketplace_incompatible_version',
  'marketplace_download_cancelled',
  'marketplace_source_unreachable',
  'marketplace_source_timeout',
  'marketplace_source_auth_rejected',
  'marketplace_source_bad_response',
  'marketplace_icon_unavailable',
  'marketplace_pending_missing',
  'marketplace_credentials_syncing',
  'marketplace_superseded',
  'marketplace_preset_not_deletable',
  'marketplace_destination_refused',
] as const

/** The kinds of the error line of one source in the catalog (`SourceError.kind`). */
export const SOURCE_ERROR_KINDS = [
  'unreachable',
  'timeout',
  'auth_rejected',
  'credentials_missing',
  'credentials_syncing',
  'bad_response',
  'insecure_credentials',
] as const

/** Why a version cannot be installed (`CatalogVersion.incompatibleReason`, no versions at all). */
export const BLOCKED_REASONS = [
  'needs_newer',
  'needs_older',
  'no_checksum',
  'no_versions',
] as const

export const reasonKey = (reason: string): string =>
  `marketplace.errors.${reason}`

export const sourceErrorKey = (kind: string): string =>
  `marketplace.sourceError.${kind}`

/** The download was ended by the user or by a closing vault: not a failure to report. */
export const isCancelled = (error: unknown): boolean =>
  typeof error === 'object' &&
  error !== null &&
  (error as { kind?: unknown }).kind === 'Marketplace' &&
  ((error as { reason?: unknown }).reason ===
    'marketplace_download_cancelled' ||
    (error as { reason?: unknown }).reason === 'marketplace_superseded')
