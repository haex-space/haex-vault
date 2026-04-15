/**
 * Fetch response validation utilities.
 * Centralizes the common `response.json().catch(() => ({}))` error handling pattern.
 */

/**
 * Throws an error if the response is not OK, with parsed error message from the response body.
 * Replaces the repetitive pattern:
 * ```ts
 * const error = await response.json().catch(() => ({}))
 * throw new Error(`Failed to X: ${error.error || response.statusText}`)
 * ```
 */
export async function throwIfNotOk(response: Response, context: string): Promise<void> {
  if (response.ok) return

  const error = await response.json().catch(() => ({}))
  throw new Error(`Failed to ${context}: ${error.error || error.message || response.statusText}`)
}

/**
 * Parses JSON from response, returning empty object on failure.
 * Replaces the repetitive pattern: `await response.json().catch(() => ({}))`
 */
export async function safeJson<T = Record<string, unknown>>(response: Response): Promise<T> {
  return response.json().catch(() => ({})) as T
}
