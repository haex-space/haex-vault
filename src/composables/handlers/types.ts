// Shared types for extension message handlers
import type { IHaexSpaceExtension } from '~/types/haexspace'

export interface ExtensionRequest {
  id: string
  method: string
  params: Record<string, unknown>
  timestamp: number
}

export interface ExtensionInstance {
  extension: IHaexSpaceExtension
  windowId: string
}
