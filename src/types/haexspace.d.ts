// Re-export types from bindings for backwards compatibility
export type { ExtensionManifest as IHaexSpaceExtensionManifest } from '~~/src-tauri/bindings/ExtensionManifest'
import type { ExtensionInfoResponse } from '~~/src-tauri/bindings/ExtensionInfoResponse'

/**
 * Extension with computed icon URL for display
 * Extends ExtensionInfoResponse with iconUrl for cross-platform compatibility
 */
export interface IHaexSpaceExtension extends ExtensionInfoResponse {
  /** Computed URL for displaying the icon (works on all platforms) */
  iconUrl?: string
}

// Re-export marketplace SDK types
export type {
  ExtensionListItem,
  ExtensionDetail,
  ExtensionVersion,
  CategoryWithCount,
  DownloadResponse,
} from '@haex-space/marketplace-sdk'

/**
 * Marketplace extension view model
 * Extends SDK ExtensionListItem with local installation status
 */
export interface MarketplaceExtensionViewModel {
  // From API
  id: string
  extensionId: string
  name: string
  slug: string
  shortDescription: string
  iconUrl: string | null
  verified: boolean
  totalDownloads: number
  averageRating: number | null
  reviewCount: number
  tags: string[] | null
  publishedAt: string | null
  publisher: {
    displayName: string
    slug: string
    verified: boolean
  } | null
  category: {
    name: string
    slug: string
  } | null
  versions: ExtensionVersion[]
  // Local state
  isInstalled: boolean
  installedVersion?: string
  latestVersion?: string
}
