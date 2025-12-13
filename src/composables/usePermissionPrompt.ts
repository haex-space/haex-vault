import { invoke } from '@tauri-apps/api/core'

export interface PermissionPromptData {
  extensionId: string
  extensionName: string
  resourceType: 'db' | 'web' | 'fs' | 'shell'
  action: string
  target: string
}

export type PermissionDecision = 'granted' | 'denied' | 'ask'

// Error code for permission prompt required
export const ERROR_CODE_PERMISSION_PROMPT_REQUIRED = 1004

// Global state for the permission prompt
const isOpen = ref(false)
const promptData = ref<PermissionPromptData | null>(null)
let resolvePromise: ((result: PermissionDecision) => void) | null = null

/**
 * Type guard to check if an error requires a permission prompt
 */
export function isPermissionPromptRequired(error: unknown): error is PermissionPromptData & { code: number } {
  return (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    (error as { code: number }).code === ERROR_CODE_PERMISSION_PROMPT_REQUIRED &&
    'extensionId' in error &&
    'extensionName' in error &&
    'resourceType' in error &&
    'action' in error &&
    'target' in error
  )
}

/**
 * Extract PermissionPromptData from error
 */
export function extractPromptData(error: unknown): PermissionPromptData | null {
  if (!isPermissionPromptRequired(error)) {
    return null
  }

  // After type guard passes, we know error has the required shape
  const errorObj = error as PermissionPromptData & { code: number }

  return {
    extensionId: errorObj.extensionId,
    extensionName: errorObj.extensionName,
    resourceType: errorObj.resourceType,
    action: errorObj.action,
    target: errorObj.target,
  }
}

/**
 * Composable for managing runtime permission prompts
 *
 * Usage in handler:
 * ```ts
 * const { promptForPermission, isPermissionPromptRequired } = usePermissionPrompt()
 *
 * try {
 *   return await invoke('some_command', { ... })
 * } catch (error) {
 *   if (isPermissionPromptRequired(error)) {
 *     const decision = await promptForPermission(extractPromptData(error)!)
 *     if (decision === 'granted' || decision === 'ask') {
 *       // Retry the request
 *       return await invoke('some_command', { ... })
 *     }
 *     // User denied - return error to extension
 *     throw error
 *   }
 *   throw error
 * }
 * ```
 */
export function usePermissionPrompt() {
  /**
   * Show a permission prompt dialog and wait for user decision
   */
  async function promptForPermission(data: PermissionPromptData): Promise<PermissionDecision> {
    promptData.value = data
    isOpen.value = true

    return new Promise<PermissionDecision>((resolve) => {
      resolvePromise = resolve
    })
  }

  /**
   * Handle user decision from the dialog
   * Called by the dialog component when user clicks a button
   */
  async function handleDecision(decision: PermissionDecision) {
    if (!promptData.value) {
      return
    }

    // For "granted" or "denied", save to database
    // For "ask" (one-time allow), don't save - just allow this request
    if (decision !== 'ask') {
      try {
        await invoke('resolve_permission_prompt', {
          extensionId: promptData.value.extensionId,
          resourceType: promptData.value.resourceType,
          action: promptData.value.action,
          target: promptData.value.target,
          decision,
        })
      } catch (error) {
        console.error('Failed to save permission decision:', error)
      }
    }

    // Close dialog and resolve promise
    isOpen.value = false
    resolvePromise?.(decision)
    resolvePromise = null
    promptData.value = null
  }

  /**
   * Cancel the prompt (equivalent to deny for this request only)
   */
  function cancelPrompt() {
    isOpen.value = false
    resolvePromise?.('denied')
    resolvePromise = null
    promptData.value = null
  }

  return {
    // State
    isOpen: readonly(isOpen),
    promptData: readonly(promptData),

    // Methods
    promptForPermission,
    handleDecision,
    cancelPrompt,

    // Type guards
    isPermissionPromptRequired,
    extractPromptData,
  }
}
