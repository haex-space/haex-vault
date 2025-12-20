// Handler Invoke Utilities
// Provides permission-aware invoke wrapper for extension commands

import { invoke } from '@tauri-apps/api/core'
import {
  isPermissionPromptRequired,
  extractPromptData,
} from '~/composables/usePermissionPrompt'

const { promptForPermission } = usePermissionPrompt()

/**
 * Wraps an invoke call with permission prompt handling.
 * If the backend returns a permission prompt required error,
 * shows the permission dialog and retries on approval.
 */
export async function invokeWithPermissionPrompt<T>(
  command: string,
  args: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, args)
  }
  catch (error) {
    if (isPermissionPromptRequired(error)) {
      const promptData = extractPromptData(error)!
      const decision = await promptForPermission(promptData)

      if (decision === 'granted' || decision === 'ask') {
        // Retry the request after permission granted/allowed once
        return await invoke<T>(command, args)
      }

      // User denied - rethrow original error
      throw error
    }
    throw error
  }
}
