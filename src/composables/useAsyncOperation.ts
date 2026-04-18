import { getErrorMessage } from '~/utils/errors'

export interface AsyncOperationOptions {
  /**
   * Custom error-to-string transformer. Defaults to `getErrorMessage`
   * which covers `Error`, `string`, and generic objects.
   */
  transform?: (error: unknown) => string
  /**
   * Side-effect callback invoked with the raw error before rethrow.
   * Useful for logging, toasts, or analytics.
   */
  onError?: (error: unknown) => void
}

/**
 * Wrap async work with standardized `isLoading` / `error` state.
 *
 * `execute` rethrows on failure so callers can compose control flow normally.
 * If a call site needs "return null on failure" semantics:
 *
 * ```ts
 * const data = await op.execute(() => fetchThing()).catch(() => null)
 * ```
 */
export function useAsyncOperation(options: AsyncOperationOptions = {}) {
  const isLoading = ref(false)
  const error = ref<string | null>(null)

  async function execute<T>(fn: () => Promise<T>): Promise<T> {
    isLoading.value = true
    error.value = null
    try {
      return await fn()
    } catch (e) {
      error.value = (options.transform ?? getErrorMessage)(e)
      options.onError?.(e)
      throw e
    } finally {
      isLoading.value = false
    }
  }

  return {
    isLoading: readonly(isLoading),
    error: readonly(error),
    execute,
  }
}
