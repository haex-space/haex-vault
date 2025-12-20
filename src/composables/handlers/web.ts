import { TAURI_COMMANDS } from '@haex-space/vault-sdk'
import type { IHaexSpaceExtension } from '~/types/haexspace'
import type { ExtensionRequest } from './types'
import { invokeWithPermissionPrompt } from './invoke'

export async function handleWebMethodAsync(
  request: ExtensionRequest,
  extension: IHaexSpaceExtension,
) {
  if (!extension || !request) {
    throw new Error('Extension not found')
  }

  const { method, params } = request

  switch (method) {
    case TAURI_COMMANDS.web.fetch: {
      const url = params.url as string
      const httpMethod = (params.method as string) || undefined
      const headers = (params.headers as Record<string, string>) || undefined
      const body = params.body as string | undefined
      const timeout = (params.timeout as number) || undefined

      if (!url) {
        throw new Error('URL is required')
      }

      const response = await invokeWithPermissionPrompt<{
        status: number
        status_text: string
        headers: Record<string, string>
        body: string
        url: string
      }>(TAURI_COMMANDS.web.fetch, {
        url,
        method: httpMethod,
        headers,
        body,
        timeout,
        publicKey: extension.publicKey,
        name: extension.name,
      })

      return {
        status: response.status,
        statusText: response.status_text,
        headers: response.headers,
        body: response.body,
        url: response.url,
      }
    }

    case TAURI_COMMANDS.web.open: {
      const url = params.url as string

      if (!url) {
        throw new Error('URL is required')
      }

      await invokeWithPermissionPrompt(TAURI_COMMANDS.web.open, {
        url,
        publicKey: extension.publicKey,
        name: extension.name,
      })

      return
    }

    default:
      throw new Error(`Unknown web method: ${method}`)
  }
}
