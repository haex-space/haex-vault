import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invoke } from '@tauri-apps/api/core'
import {
  isPermissionPromptRequired,
  extractPromptData,
} from '~/composables/usePermissionPrompt'

const { promptForPermission } = usePermissionPrompt()

type PermissionStatus = 'granted' | 'denied'
type PermissionCheckResult = { status: PermissionStatus }

// Error code emitted by the Rust backend when a permission is denied (mirrors
// `ExtensionError::permission_denied` → code 1002). We also accept the message
// suffix as a belt-and-braces fallback in case the code field is stripped by an
// older Tauri layer.
const ERROR_CODE_PERMISSION_DENIED = 1002

/**
 * Run a backend permission check and uniformly handle:
 *   - granted: backend returned `Ok(())`
 *   - prompt-required: backend returned `PermissionPromptRequired` → show the
 *     consent dialog and resolve to the user's decision
 *   - denied: backend returned a denied error (code 1002 or matching message)
 *   - anything else: re-thrown
 *
 * Centralising this means the catch logic — including the `code === 1002`
 * fallback and the "ask → granted/denied" mapping — lives in one place. The
 * three permission-check handlers below are thin per-resource adapters that
 * only validate their own params and build the invoke argument shape.
 */
async function runPermissionCheck(
  command: string,
  args: Record<string, unknown>,
): Promise<PermissionCheckResult> {
  try {
    await invoke<undefined>(command, args)
    return { status: 'granted' }
  }
  catch (error: unknown) {
    if (isPermissionPromptRequired(error)) {
      const decision = await promptForPermission(extractPromptData(error)!)
      return { status: decision === 'granted' ? 'granted' : 'denied' }
    }
    const err = error as { code?: number; message?: string }
    if (err?.code === ERROR_CODE_PERMISSION_DENIED
      || err?.message?.includes('Permission denied')) {
      return { status: 'denied' }
    }
    throw error
  }
}

export async function handlePermissionsMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!extension || !request) {
    throw new Error('Extension not found')
  }

  const { method, params } = request

  if (method === 'permissions.web.check') {
    return await checkWebPermissionAsync(params, extension)
  }

  if (method === 'permissions.database.check') {
    return await checkDatabasePermissionAsync(params, extension)
  }

  if (method === 'permissions.filesystem.check') {
    return await checkFilesystemPermissionAsync(params, extension)
  }

  throw new Error(`Unknown permission method: ${method}`)
}

async function checkWebPermissionAsync(
  params: Record<string, unknown>,
  extension: IHaexSpaceExtension,
) {
  const url = params.url as string
  const method = (params.method as string) || 'GET'

  if (!url) {
    throw new Error('URL is required')
  }

  return runPermissionCheck('check_web_permission', {
    extensionId: extension.id,
    method,
    url,
  })
}

async function checkDatabasePermissionAsync(
  params: Record<string, unknown>,
  extension: IHaexSpaceExtension,
) {
  const resource = params.resource as string
  const operation = params.operation as string

  if (!resource || !operation) {
    throw new Error('Resource and operation are required')
  }

  return runPermissionCheck('check_database_permission', {
    extensionId: extension.id,
    resource,
    operation,
  })
}

async function checkFilesystemPermissionAsync(
  params: Record<string, unknown>,
  extension: IHaexSpaceExtension,
) {
  const path = params.path as string
  const operation = params.operation as string

  if (!path || !operation) {
    throw new Error('Path and operation are required')
  }

  return runPermissionCheck('check_filesystem_permission', {
    extensionId: extension.id,
    path,
    operation,
  })
}
