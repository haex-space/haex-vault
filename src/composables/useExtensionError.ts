import type { SerializedExtensionError } from '~~/src-tauri/bindings/SerializedExtensionError'

/**
 * Type guard to check if error is a SerializedExtensionError
 */
export function isSerializedExtensionError(error: unknown): error is SerializedExtensionError {
  return (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'message' in error &&
    'type' in error
  )
}

/**
 * Composable for handling extension errors
 */
export function useExtensionError() {
  return {
    isSerializedExtensionError,
  }
}
