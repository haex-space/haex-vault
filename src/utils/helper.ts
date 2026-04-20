import { getPlatform } from '~/utils/platform'

const SIZE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const

export const readableFileSize = (
  sizeInByte: number | string | bigint = 0,
): string => {
  const size =
    typeof sizeInByte === 'bigint'
      ? Number(sizeInByte)
      : typeof sizeInByte === 'string'
        ? parseInt(sizeInByte)
        : sizeInByte

  if (!size) return '0 B'

  const unitIndex = Math.min(
    Math.floor(Math.log(size) / Math.log(1024)),
    SIZE_UNITS.length - 1,
  )
  const value = size / Math.pow(1024, unitIndex)
  return `${value.toFixed(unitIndex > 0 ? 1 : 0)} ${SIZE_UNITS[unitIndex]}`
}

export const getSingleRouteParam = (
  param: string | string[] | undefined,
): string => {
  const _param = Array.isArray(param) ? (param.at(0) ?? '') : (param ?? '')
  return decodeURIComponent(_param)
}

export const filterAsync = async <T>(
  arr: T[],
  predicate: (value: T, index: number, array: T[]) => Promise<boolean>,
) => {
  // 1. Mappe jedes Element auf ein Promise, das zu true/false auflöst
  const results = await Promise.all(arr.map(predicate))

  // 2. Filtere das ursprüngliche Array basierend auf den Ergebnissen
  return arr.filter((_value, index) => results[index])
}

export const getContrastingTextColor = (
  hexColor?: string | null,
): 'black' | 'white' => {
  if (!hexColor) {
    return 'black' // Fallback
  }

  // Entferne das '#' vom Anfang
  let color = hexColor.startsWith('#') ? hexColor.slice(1) : hexColor

  // Handle Kurzform-Hex-Werte (z.B. "F0C" -> "FF00CC")
  if (color.length === 3) {
    color = color
      .split('')
      .map((char) => char + char)
      .join('')
  }

  if (color.length !== 6) {
    return 'black' // Fallback für ungültige Farben
  }

  // Konvertiere Hex zu RGB
  const r = parseInt(color.substring(0, 2), 16)
  const g = parseInt(color.substring(2, 4), 16)
  const b = parseInt(color.substring(4, 6), 16)

  // Berechne die wahrgenommene Luminanz nach der WCAG-Formel.
  // Werte von 0 (schwarz) bis 255 (weiß).
  const luminance = 0.299 * r + 0.587 * g + 0.114 * b

  // Wähle die Textfarbe basierend auf einem Schwellenwert.
  // Ein Wert > 186 wird oft als "hell" genug für schwarzen Text angesehen.
  return luminance > 186 ? 'black' : 'white'
}

export const getFileName = (fullPath: string) => {
  const seperator = getPlatform() === 'windows' ? '\\' : '/'
  return fullPath.split(seperator).pop()
}

/**
 * Get the directory path from a full file path
 */
export const getDirectoryPath = (fullPath: string) => {
  const separator = getPlatform() === 'windows' ? '\\' : '/'
  const parts = fullPath.split(separator)
  parts.pop() // Remove filename
  return parts.join(separator)
}

/**
 * Shorten a path for display by:
 * - Replacing home directory with ~
 * - Truncating middle segments if too long
 *
 * @param fullPath The full path to shorten
 * @param maxLength Maximum length before truncation (default: 40)
 */
export const shortenPath = (fullPath: string, maxLength: number = 40): string => {
  if (!fullPath) return ''

  const isWindows = getPlatform() === 'windows'
  const separator = isWindows ? '\\' : '/'

  let path = fullPath

  // Replace home directory with ~ (Unix-like systems)
  if (!isWindows) {
    const homeDir = '/home/'
    if (path.startsWith(homeDir)) {
      const afterHome = path.slice(homeDir.length)
      const firstSlash = afterHome.indexOf('/')
      if (firstSlash !== -1) {
        path = '~' + afterHome.slice(firstSlash)
      }
    }
  }

  // If short enough, return as-is
  if (path.length <= maxLength) {
    return path
  }

  // Split into parts and truncate middle
  const parts = path.split(separator)

  if (parts.length <= 3) {
    // Can't really shorten further
    return path
  }

  // Keep first part (~ or drive letter) and last 2 parts
  const first = parts[0]
  const last = parts.slice(-2).join(separator)

  return `${first}${separator}...${separator}${last}`
}
