// Re-export types from bindings for backwards compatibility
export type { ExtensionManifest as IHaexSpaceExtensionManifest } from '~~/src-tauri/bindings/ExtensionManifest'
export type { ExtensionInfoResponse as IHaexSpaceExtension } from '~~/src-tauri/bindings/ExtensionInfoResponse'

/**
 * Marketplace extension with additional metadata
 * Extends IHaexSpaceExtension with marketplace-specific fields
 */
export interface IMarketplaceExtension extends Omit<IHaexSpaceExtension, 'enabled'> {
  downloads: number
  rating: number
  verified: boolean
  tags: string[]
  category: string
  downloadUrl: string
  isInstalled: boolean
  installedVersion?: string // The version that is currently installed (if different from marketplace version)
}
