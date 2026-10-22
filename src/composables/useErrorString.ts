/** contracts/tauri-commands.md (spec 009-autonomous-delegate-mode): the
 * `send_message` `InvalidInput` reason prefix for an autonomy mode the
 * connected delegate's installed CLI does not support (FR-013/SC-006).
 * Checked before the generic `InvalidInput` fallback below — other
 * `InvalidInput` errors keep their existing mapping. */
const AUTONOMY_UNAVAILABLE_PREFIX =
  'adapter start: backend unavailable: autonomy mode unavailable:'

// `InvalidInput` is deliberately excluded: unlike the kinds below, it's a
// generic catch-all bucket reused across the whole backend (providers,
// chat, voice, device commands — see src-tauri/src/providers/mod.rs
// map_adapter_error), not something only HF commands return, so it must
// fall through to the `reason`-based generic mapping below instead of
// this HF-flavored one.
const HF_ERROR_KINDS = new Set([
  'Network',
  'Timeout',
  'HttpStatus',
  'RateLimited',
  'UnsupportedFormat',
  'TokenizerRequired',
  'HardwareConfirmationRequired',
  'ModelDownload',
  'ModelRegistrationFailed',
  'ModelNotFound',
])

/**
 * Converts backend and JavaScript failures into displayable text. Shared by
 * the chat page (composer/thread errors) and `useModelsStore` (model
 * lifecycle errors) so the HF/idempotency-kind mapping has one source.
 */
export function useErrorString() {
  const { t, te } = useI18n()

  /** Maps a caught value to a localized or safely serialized error message. */
  function errString(e: unknown): string {
    if (typeof e === 'string') return e
    if (e && typeof e === 'object' && 'kind' in e) {
      const kind = (e as { kind: unknown }).kind
      const reason = (e as { reason?: unknown }).reason
      if (kind === 'InvalidIdempotencyKey')
        return t('errors.invalidIdempotencyKey')
      if (kind === 'IdempotencyKeyConflict')
        return t('errors.idempotencyKeyConflict')
      // Spec 013 US4 (contracts/frontend-surface.md): fieldless variants with no `reason`, so
      // without an explicit mapping they would otherwise fall through to `JSON.stringify(e)`
      // below.
      if (kind === 'VaultAlreadyOpenElsewhere')
        return t('errors.vaultAlreadyOpenElsewhere')
      if (kind === 'VaultAlreadyActive') return t('errors.vaultAlreadyActive')
      if (kind === 'VaultClosed') return t('errors.vaultClosed')
      if (kind === 'TransactionTooLarge') return t('errors.transactionTooLarge')
      // Spec 043 (contract picked-file.md): a chosen file that gives nothing back, a full device.
      if (kind === 'Unreadable') return t('errors.unreadable')
      if (kind === 'NotEnoughSpace') return t('errors.notEnoughSpace')
      // Spec 043 FR-024: the server's certificate is not trusted; the connection was refused.
      if (kind === 'UntrustedCertificate')
        return t('errors.untrustedCertificate')
      // Spec 034 (contracts/tauri-commands.md §Fehlerarten): the password manager's own kinds. They
      // carry a reason or numbers, never a value, so they must not reach `JSON.stringify` below.
      if (kind === 'PasswordsNotFound') return t('errors.passwords.notFound')
      if (kind === 'PasswordsForbidden') return t('errors.passwords.forbidden')
      if (kind === 'PasswordsConflict') {
        if (reason === 'changed') return t('errors.passwords.conflictChanged')
        if (reason === 'deleted') return t('errors.passwords.conflictDeleted')
        return t('errors.passwords.conflict')
      }
      if (kind === 'PasswordsAttachmentTooLarge') {
        const limit = (e as { limit?: unknown }).limit
        const mib = typeof limit === 'number' ? Math.floor(limit / 1048576) : 0
        return t('errors.passwords.attachmentTooLarge', { limit: mib })
      }
      // Spec 036 (contracts/tauri-commands.md §Fehlerarten): references and the copy.
      if (kind === 'PasswordsReference') {
        return reason === 'missing'
          ? t('errors.passwords.referenceMissing')
          : t('errors.passwords.referenceCycle')
      }
      if (kind === 'PasswordsReferenceCycle')
        return t('errors.passwords.referenceCycleOnSave')
      if (kind === 'PasswordsIntoTrash') return t('errors.passwords.intoTrash')
      if (kind === 'PasswordsImportFailed') {
        if (typeof reason === 'string' && reason.length > 0) {
          const key = `errors.passwords.importReason.${reason}`
          return te(key) ? t(key) : t('errors.passwords.importFailed')
        }
        return t('errors.passwords.importFailed')
      }
      // Spec 035 (contracts/appearance-actions.md): the appearance actions name the cause by the
      // language key in `reason`, and the field of a refused file in `field`.
      if (kind === 'AppearanceError') {
        const field = (e as { field?: unknown }).field
        return typeof reason === 'string' && te(reason)
          ? t(reason, { field: typeof field === 'string' ? field : '' })
          : t('settings.appearance.failed')
      }
      // Spec 051: the sync with the phone's address book.
      if (kind === 'ContactsDenied') return t('contacts.error.contactsDenied')
      if (kind === 'PhoneContactsUnavailable') return t('errors.notAvailable')
      // Spec 049: the External Bridge exists on the desktop only.
      if (kind === 'BrowserBridgeUnavailable') return t('errors.notAvailable')
      if (kind === 'BrowserBridgeClientNotFound')
        return t('browserBridge.clients.notFound')
      if (kind === 'CardInvalid') return t('contacts.error.cardInvalid')
      if (kind === 'CardOwn') return t('contacts.error.cardOwn')
      if (kind === 'CardOlder') return t('contacts.error.cardOlder')
      if (kind === 'CardTooLarge') return t('contacts.error.cardTooLarge')
      // Spec 017: the extension host's kinds name a reason or a state, never data of an extension.
      if (kind === 'ExtensionInstall') {
        const key = `extensions.install.errors.${String(reason)}`
        return te(key) ? t(key) : t('errors.extensions.install')
      }
      if (kind === 'ExtensionNotFound') return t('errors.extensions.notFound')
      if (kind === 'ExtensionDisabled') return t('errors.extensions.disabled')
      if (kind === 'ExtensionNotReady') {
        const status = (e as { status?: unknown }).status
        const key = `extensions.status.${String(status)}`
        return te(key) ? t(key) : t('errors.extensions.notReady')
      }
      if (kind === 'InvalidInput' && typeof reason === 'string') {
        if (reason.startsWith(AUTONOMY_UNAVAILABLE_PREFIX))
          return t('errors.autonomyUnavailable')
        // Generic across providers/chat/voice/device commands (see
        // `HF_ERROR_KINDS`'s own comment) — a localized label plus the
        // backend's own reason, the same "translated frame + raw detail"
        // shape `HuggingFaceModelManagement.vue` builds by hand from
        // `hfErrorKey`/`hfErrorDetail`, so a real reason like "claude not
        // found on PATH" stays visible instead of being replaced outright
        // by a canned message.
        return reason.length > 0
          ? `${t('errors.invalidInput')}: ${reason}`
          : t('errors.invalidInput')
      }
      if (typeof kind === 'string' && HF_ERROR_KINDS.has(kind))
        return t(hfErrorKey(e))
      // HolziError variants serialize as `{ kind, ...fields }` with no
      // Display-rendered message on the wire (see src-tauri/src/error.rs) —
      // `reason` is the closest thing to human-readable text most variants
      // carry. Falls back to the raw shape only for the few fieldless
      // variants (e.g. WrongPassphrase) that have no text at all.
      if (typeof reason === 'string' && reason.length > 0) return reason
      return JSON.stringify(e)
    }
    if (e instanceof Error) return e.message
    return String(e)
  }

  return { errString }
}
