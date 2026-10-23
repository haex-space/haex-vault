// Small pure helpers of the password manager window (spec 034-password-manager): how a header is
// named, when an entry counts as expired.

import { viewerKind } from '../files/viewerKind.ts'

/** The text a list shows for an entry: its title, or `null` when there is none and the window
 * shows the placeholder `passwords.untitled` instead. The placeholder is a text of the window,
 * never a stored value (FR-001). */
export function displayTitle(title: string | null | undefined): string | null {
  const trimmed = title?.trim()
  return trimmed ? trimmed : null
}

/** `YYYY-MM-DD` of a date in the local time zone. */
export function localDay(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${date.getFullYear()}-${month}-${day}`
}

/** True when the expiry day (`YYYY-MM-DD`) lies before `today` (FR-008). An entry expires at the
 * end of its day; an unreadable value does not mark the entry. */
export function isExpired(
  expiresAt: string | null | undefined,
  today: string,
): boolean {
  if (!expiresAt || !/^\d{4}-\d{2}-\d{2}$/.test(expiresAt)) return false
  return expiresAt < today
}

/** The schemes a stored address may open with their system handler. Keep in step with the
 * `opener:allow-open-url` scope in `src-tauri/capabilities/default.json`. */
const OPENABLE_SCHEMES = [
  'http',
  'https',
  'ftp',
  'ftps',
  'sftp',
  'ssh',
  'file',
  'smb',
  'vnc',
  'rdp',
  'telnet',
]

/** The address the system opens for a stored URL: `https://` is added when no scheme is written,
 * and `null` when the scheme is not one of `OPENABLE_SCHEMES` or nothing is left to open. */
export function openableUrl(value: string | null | undefined): string | null {
  const trimmed = value?.trim()
  if (!trimmed) return null
  const schemeMatch = /^([a-z][a-z0-9+.-]*):(\/\/)?/i.exec(trimmed)
  if (!schemeMatch) return `https://${trimmed}`

  const schemePart = schemeMatch[1]
  if (!schemePart) return `https://${trimmed}`
  const scheme = schemePart.toLowerCase()
  if (!schemeMatch[2]) {
    // A hostname with a numeric port is the one useful no-scheme form that also matches the
    // URI scheme grammar. Other colon-prefixed values are schemes and must be rejected.
    const rest = trimmed.slice(schemeMatch[0].length)
    const isHostPort =
      /^\d+(?:[/?#]|$)/.test(rest) &&
      (scheme === 'localhost' ||
        scheme.includes('.') ||
        /^[\d.]+$/.test(scheme))
    return isHostPort ? `https://${trimmed}` : null
  }

  if (!OPENABLE_SCHEMES.includes(scheme)) return null
  return `${scheme}${trimmed.slice(schemePart.length)}`
}

/** `1.50 KiB`, `3.00 MiB`, … with two decimals; plain bytes below 1 KiB. */
export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '0 Bytes'
  const units = ['Bytes', 'KiB', 'MiB', 'GiB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return unit === 0 ? `${value} Bytes` : `${value.toFixed(2)} ${units[unit]}`
}

/** A file name as text, never as a path (FR-021): path separators and control characters become
 * `_`, at most 255 characters. */
export function safeFileName(name: string): string {
  const cleaned = Array.from(name)
    .map((char) => {
      const code = char.codePointAt(0) ?? 0
      return char === '/' || char === '\\' || code < 32 || code === 127
        ? '_'
        : char
    })
    .join('')
  return Array.from(cleaned).slice(0, 255).join('')
}

const IMAGE_MIME: Record<string, string> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
}

/** The mime type of an attachment that can be previewed, derived from its file name (the same five
 * extensions the backend previews); `null` for everything else, which only offers a download. */
export function imageMime(fileName: string): string | null {
  const dot = fileName.lastIndexOf('.')
  if (dot < 0) return null
  return IMAGE_MIME[fileName.slice(dot + 1).toLowerCase()] ?? null
}

/** How an attachment is shown (spec 036, FR-037; spec 058 FR-001): an image gets a thumbnail and the
 * lightbox, text and PDF open the attachment view, the others a type icon and "Speichern unter".
 * SVG counts as other: it can carry script. */
export type FileKind = 'image' | 'pdf' | 'text' | 'other'

/** The kind of an attachment by its file name. Text and PDF follow the file viewer (spec 058
 * FR-002), so both apps agree on what is text. */
export function fileKind(fileName: string): FileKind {
  if (imageMime(fileName)) return 'image'
  const kind = viewerKind(fileName)
  return kind === 'pdf' || kind === 'text' ? kind : 'other'
}

const RELATIVE_STEPS: readonly [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

/** A past time as text in the largest fitting unit ("vor 2 Tagen", "yesterday", "now"; spec 036
 * FR-007), in the language `locale`. A time that is not in the past reads as now. */
export function relativeTime(then: Date, now: Date, locale: string): string {
  const seconds = Math.max(
    0,
    Math.round((now.getTime() - then.getTime()) / 1000),
  )
  const format = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
  for (const [unit, length] of RELATIVE_STEPS) {
    if (seconds >= length)
      return format.format(-Math.floor(seconds / length), unit)
  }
  return format.format(0, 'second')
}
