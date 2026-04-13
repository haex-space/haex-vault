import { NoCurrentIdentityError } from '@/composables/useCurrentIdentity'

/**
 * Shared error-to-toast helper for operation handlers.
 *
 * Centralises two patterns that recur across settings views:
 * 1. `NoCurrentIdentityError` gets a dedicated, translated toast.
 * 2. All other errors fall back to the operation's own error title plus the
 *    thrown message as description.
 *
 * Callers pass the i18n key for the fallback title. Optionally they can
 * override the no-identity title key (default: `errors.noIdentity`).
 */
export function useOperationErrorToast() {
  const { t } = useI18n({
    useScope: 'global',
    messages: {
      de: {
        operationError: {
          noIdentity: 'Keine Identität verfügbar',
          unknown: 'Unbekannter Fehler',
        },
      },
      en: {
        operationError: {
          noIdentity: 'No identity available',
          unknown: 'Unknown error',
        },
      },
    },
  })
  const { add } = useToast()

  const showOperationError = (
    error: unknown,
    fallbackTitleKey: string,
    options?: { noIdentityTitleKey?: string },
  ) => {
    if (error instanceof NoCurrentIdentityError) {
      add({
        title: options?.noIdentityTitleKey
          ? t(options.noIdentityTitleKey)
          : t('operationError.noIdentity'),
        color: 'error',
      })
      return
    }

    add({
      title: t(fallbackTitleKey),
      description:
        error instanceof Error ? error.message : t('operationError.unknown'),
      color: 'error',
    })
  }

  return { showOperationError }
}
