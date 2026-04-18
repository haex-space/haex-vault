/**
 * Utility functions for working with HaexSpace extensions
 */

import { convertFileSrc } from '@tauri-apps/api/core'
import {
  EXTENSION_PROTOCOL_PREFIX,
  EXTENSION_PROTOCOL_NAME,
} from '~/config/constants'
import { isMobile, isAndroid, getPlatform } from '~/utils/platform'

export function encodeExtensionInfo(
  name: string,
  publicKey: string,
  version: string,
): string {
  return btoa(JSON.stringify({ name, publicKey, version }))
}

/**
 * Generates a URL for loading an extension icon (synchronous version)
 *
 * On Android, convertFileSrc() doesn't work with absolute file paths,
 * so we use the extension protocol to load icons.
 * On desktop, we use convertFileSrc() for direct file access.
 *
 * @param iconPath - The absolute path to the icon file (from extension.icon)
 * @param publicKey - The extension's public key
 * @param name - The extension name
 * @param version - The extension version
 * @returns The complete icon URL
 */
export function getExtensionIconUrl(
  iconPath: string | null | undefined,
  publicKey: string,
  name: string,
  version: string,
): string {
  if (!iconPath || !publicKey || !name || !version) {
    return ''
  }

  if (isMobile()) {
    // Mobile: Use extension protocol to load icon
    // The iconPath is an absolute path like:
    // /data/data/.../extensions/{publicKey}/{name}/{version}/haextension/icon.png
    // We need to extract the relative path from the version directory
    // (e.g., "haextension/icon.png")
    const versionMarker = `/${version}/`
    const versionIndex = iconPath.indexOf(versionMarker)
    let relativeIconPath: string

    if (versionIndex !== -1) {
      // Extract everything after "{version}/"
      relativeIconPath = iconPath.substring(versionIndex + versionMarker.length)
    } else {
      // Fallback: just use the filename
      relativeIconPath = iconPath.split('/').pop() || iconPath.split('\\').pop() || iconPath
    }

    const encodedInfo = encodeExtensionInfo(name, publicKey, version)

    if (isAndroid()) {
      return `http://${EXTENSION_PROTOCOL_NAME}.localhost/${encodedInfo}/${relativeIconPath}`
    } else {
      return `${EXTENSION_PROTOCOL_PREFIX}${encodedInfo}/${relativeIconPath}`
    }
  } else {
    // Desktop: Use convertFileSrc for direct file access
    return convertFileSrc(iconPath)
  }
}

/**
 * Generates the extension URL for loading an extension in an iframe
 *
 * @param publicKey - The extension's public key (64 hex chars)
 * @param name - The extension name
 * @param version - The extension version
 * @param assetPath - Optional asset path (defaults to 'index.html')
 * @param devServerUrl - Optional dev server URL for development extensions
 * @returns The complete extension URL
 */
export function getExtensionUrl(
  publicKey: string,
  name: string,
  version: string,
  assetPath: string = 'index.html',
  devServerUrl?: string,
): string {
  if (!publicKey || !name || !version) {
    console.error('Missing required extension fields')
    return ''
  }

  // If dev server URL is provided, load directly from dev server
  if (devServerUrl) {
    const cleanUrl = devServerUrl.replace(/\/$/, '') // Remove trailing slash
    const cleanPath = assetPath.replace(/^\//, '') // Remove leading slash
    return cleanPath ? `${cleanUrl}/${cleanPath}` : cleanUrl
  }

  // Production extension: Use custom protocol
  // Encode extension info as base64 for unique origin per extension
  const encodedInfo = encodeExtensionInfo(name, publicKey, version)

  // Android WebView and Windows WebView2 cannot load custom schemes directly;
  // Tauri maps them to http://{scheme}.localhost. Linux/macOS load the scheme natively.
  const platform = getPlatform()
  if (platform === 'android' || platform === 'windows') {
    return `http://${EXTENSION_PROTOCOL_NAME}.localhost/${encodedInfo}/${assetPath}`
  } else {
    return `${EXTENSION_PROTOCOL_PREFIX}${encodedInfo}/${assetPath}`
  }
}
