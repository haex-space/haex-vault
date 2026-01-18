import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface PermissionPromptData {
  extensionId: string
  extensionName: string
  resourceType: 'db' | 'web' | 'fs' | 'shell' | 'filesync'
  action: string
  target: string
}

export type PermissionDecision = 'granted' | 'denied' | 'ask'

// Error code for permission prompt required
export const ERROR_CODE_PERMISSION_PROMPT_REQUIRED = 1004

// Event name for permission prompt from Rust backend (for native WebView mode)
const EVENT_PERMISSION_PROMPT_REQUIRED = 'extension:permission-prompt-required'

// Queue item for pending permission prompts
interface QueuedPrompt {
  data: PermissionPromptData
  resolve: ((result: PermissionDecision) => void) | null
}

// Global state for the permission prompt
const isOpen = ref(false)
const promptData = ref<PermissionPromptData | null>(null)
let resolvePromise: ((result: PermissionDecision) => void) | null = null

// Queue for pending permission prompts (reactive for UI updates)
const promptQueue = ref<QueuedPrompt[]>([])

// Event listener cleanup
let eventUnlisten: UnlistenFn | null = null
let isInitialized = false

/**
 * Generate a unique key for a permission prompt to detect duplicates
 */
function getPromptKey(data: PermissionPromptData): string {
  return `${data.extensionId}:${data.resourceType}:${data.action}:${data.target}`
}

/**
 * Check if a prompt is already in the queue or currently displayed
 */
function isDuplicatePrompt(data: PermissionPromptData): boolean {
  const key = getPromptKey(data)

  // Check current prompt
  if (promptData.value && getPromptKey(promptData.value) === key) {
    return true
  }

  // Check queue
  return promptQueue.value.some(item => getPromptKey(item.data) === key)
}

/**
 * Show the next prompt from the queue if available
 */
function showNextPrompt() {
  if (promptQueue.value.length === 0) {
    return
  }

  const next = promptQueue.value.shift()
  if (next) {
    promptData.value = next.data
    resolvePromise = next.resolve
    isOpen.value = true
  }
}

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
   * Show a permission prompt dialog and wait for user decision.
   * If a dialog is already open, the prompt is queued and shown after the current one is resolved.
   */
  async function promptForPermission(data: PermissionPromptData): Promise<PermissionDecision> {
    // Skip duplicate prompts
    if (isDuplicatePrompt(data)) {
      console.log('[PermissionPrompt] Skipping duplicate prompt:', getPromptKey(data))
      // Return 'ask' to indicate the prompt is pending - caller should wait/retry
      return 'ask'
    }

    return new Promise<PermissionDecision>((resolve) => {
      if (isOpen.value) {
        // Queue the prompt if one is already open
        console.log('[PermissionPrompt] Queuing prompt:', getPromptKey(data))
        promptQueue.value.push({ data, resolve })
      } else {
        // Show immediately
        promptData.value = data
        resolvePromise = resolve
        isOpen.value = true
      }
    })
  }

  /**
   * Save a permission decision (to database or session)
   */
  async function saveDecision(data: PermissionPromptData, decision: PermissionDecision, remember: boolean) {
    if (remember) {
      // Save permanently to database
      try {
        await invoke('resolve_permission_prompt', {
          extensionId: data.extensionId,
          resourceType: data.resourceType,
          action: data.action,
          target: data.target,
          decision,
        })
      } catch (error) {
        console.error('Failed to save permission decision:', error)
      }
    } else {
      // Save only for this session (in backend memory)
      try {
        await invoke('grant_session_permission', {
          extensionId: data.extensionId,
          resourceType: data.resourceType,
          action: data.action,
          target: data.target,
          decision,
        })
      } catch (error) {
        console.error('Failed to save session permission:', error)
      }
    }
  }

  /**
   * Handle user decision from the dialog
   * Called by the dialog component when user clicks a button
   *
   * @param decision - The user's decision (granted or denied)
   * @param remember - If true, save to database permanently. If false, only save for this session.
   * @param applyToAll - If true, apply the same decision to all pending prompts in the queue.
   */
  async function handleDecision(decision: PermissionDecision, remember: boolean, applyToAll: boolean = false) {
    if (!promptData.value) {
      return
    }

    const data = promptData.value

    // Save the current decision
    await saveDecision(data, decision, remember)

    // Close dialog and resolve promise for current prompt
    isOpen.value = false
    resolvePromise?.(decision)
    resolvePromise = null
    promptData.value = null

    // If applyToAll is true, process all queued prompts with the same decision
    if (applyToAll && promptQueue.value.length > 0) {
      console.log(`[PermissionPrompt] Applying decision "${decision}" to ${promptQueue.value.length} queued prompts`)

      // Process all queued prompts
      const queueCopy = [...promptQueue.value]
      promptQueue.value = [] // Clear the queue

      for (const queuedPrompt of queueCopy) {
        // Save the decision for each queued prompt
        await saveDecision(queuedPrompt.data, decision, remember)
        // Resolve the promise if it exists
        queuedPrompt.resolve?.(decision)
      }

      return // Don't show next prompt since we processed them all
    }

    // Show next prompt from queue if available
    if (promptQueue.value.length > 0) {
      // Use nextTick to ensure the dialog closes before opening the next one
      nextTick(() => {
        showNextPrompt()
      })
    }
  }

  /**
   * Cancel the prompt (equivalent to deny for this request only)
   */
  function cancelPrompt() {
    isOpen.value = false
    resolvePromise?.('denied')
    resolvePromise = null
    promptData.value = null

    // Show next prompt from queue if available
    if (promptQueue.value.length > 0) {
      nextTick(() => {
        showNextPrompt()
      })
    }
  }

  /**
   * Initialize the event listener for native WebView permission prompts.
   * Call this once in app.vue onMounted.
   *
   * For native WebView extensions, the Rust backend emits an event when
   * a permission prompt is required. This listener shows the dialog
   * to the user. The extension must retry the request after permission
   * is granted (the backend returns an error to trigger the retry).
   */
  async function init() {
    if (isInitialized) {
      return
    }
    isInitialized = true

    eventUnlisten = await listen<PermissionPromptData>(
      EVENT_PERMISSION_PROMPT_REQUIRED,
      (event) => {
        console.log('[PermissionPrompt] Received event from backend:', event.payload)

        const data = event.payload

        // Skip duplicate prompts
        if (isDuplicatePrompt(data)) {
          console.log('[PermissionPrompt] Skipping duplicate prompt from event:', getPromptKey(data))
          return
        }

        // Session permissions are checked in the Rust backend before this event is emitted
        if (isOpen.value) {
          // Queue the prompt if one is already open
          console.log('[PermissionPrompt] Queuing prompt from event:', getPromptKey(data))
          promptQueue.value.push({ data, resolve: null })
        } else {
          // Show immediately
          promptData.value = data
          resolvePromise = null
          isOpen.value = true
        }
      }
    )

    console.log('[PermissionPrompt] Event listener initialized')
  }

  /**
   * Cleanup the event listener
   */
  function cleanup() {
    if (eventUnlisten) {
      eventUnlisten()
      eventUnlisten = null
    }
    isInitialized = false
  }

  /**
   * Get the number of pending prompts in the queue
   */
  const pendingCount = computed(() => promptQueue.value.length)

  return {
    // State
    isOpen: readonly(isOpen),
    promptData: readonly(promptData),
    pendingCount,

    // Methods
    promptForPermission,
    handleDecision,
    cancelPrompt,
    init,
    cleanup,

    // Type guards
    isPermissionPromptRequired,
    extractPromptData,
  }
}
