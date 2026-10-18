// Icons and colors of entries and folders (spec 034-password-manager): a fixed list the pickers
// offer, as literal `lucide:` names so the icon scan of the Nuxt build bundles them. An entry may
// also hold another stored name (an import maps KeePass icons to names of this list, research R12);
// `binary:<hash>` is a picture of the import, shown through the icon cache.

/** The icons of the picker, in the order it shows them. */
export const ENTRY_ICONS = [
  'lucide:key-round',
  'lucide:mail',
  'lucide:globe',
  'lucide:landmark',
  'lucide:credit-card',
  'lucide:shield',
  'lucide:server',
  'lucide:wifi',
  'lucide:shopping-cart',
  'lucide:user',
  'lucide:briefcase',
  'lucide:house',
  'lucide:phone',
  'lucide:laptop',
  'lucide:cloud',
  'lucide:database',
  'lucide:lock',
  'lucide:id-card',
  'lucide:file-text',
  'lucide:terminal',
  'lucide:gamepad-2',
  'lucide:plane',
  'lucide:heart-pulse',
  'lucide:graduation-cap',
] as const

/** The pictures the import maps the KeePass standard icons (`import/icons.rs`) and the icon names
 * of haex-vault 2.x (Legacy) (`import/legacy_haex_vault/icons.rs`, spec 037) to. They are not in the picker, but an
 * entry may hold them, so they are literals here for the icon scan. */
export const IMPORT_ICONS = [
  'lucide:app-window',
  'lucide:apple',
  'lucide:award',
  'lucide:banknote',
  'lucide:bike',
  'lucide:bitcoin',
  'lucide:bluetooth',
  'lucide:book-open',
  'lucide:bookmark',
  'lucide:bus',
  'lucide:calculator',
  'lucide:calendar',
  'lucide:camera',
  'lucide:car',
  'lucide:circle-check',
  'lucide:circle-user',
  'lucide:clipboard',
  'lucide:clipboard-check',
  'lucide:clock',
  'lucide:code',
  'lucide:contact',
  'lucide:dollar-sign',
  'lucide:facebook',
  'lucide:feather',
  'lucide:file-archive',
  'lucide:file-check',
  'lucide:file-lock',
  'lucide:file-pen',
  'lucide:file-plus',
  'lucide:files',
  'lucide:fingerprint',
  'lucide:folder',
  'lucide:folder-archive',
  'lucide:folder-check',
  'lucide:folder-key',
  'lucide:folder-lock',
  'lucide:folder-open',
  'lucide:gift',
  'lucide:github',
  'lucide:gitlab',
  'lucide:globe',
  'lucide:hard-drive',
  'lucide:headphones',
  'lucide:heart',
  'lucide:house',
  'lucide:id-card',
  'lucide:image',
  'lucide:instagram',
  'lucide:key',
  'lucide:key-round',
  'lucide:key-square',
  'lucide:lightbulb',
  'lucide:linkedin',
  'lucide:list',
  'lucide:lock-open',
  'lucide:mail',
  'lucide:mailbox',
  'lucide:map-pin',
  'lucide:message-circle',
  'lucide:message-square',
  'lucide:monitor',
  'lucide:music',
  'lucide:network',
  'lucide:nfc',
  'lucide:notebook',
  'lucide:notebook-text',
  'lucide:package',
  'lucide:pen',
  'lucide:piggy-bank',
  'lucide:plug',
  'lucide:printer',
  'lucide:puzzle',
  'lucide:rocket',
  'lucide:scan',
  'lucide:screen-share',
  'lucide:server',
  'lucide:settings',
  'lucide:shield',
  'lucide:shield-check',
  'lucide:shopping-bag',
  'lucide:smartphone',
  'lucide:star',
  'lucide:sticky-note',
  'lucide:store',
  'lucide:tablet',
  'lucide:tag',
  'lucide:terminal',
  'lucide:ticket',
  'lucide:timer',
  'lucide:train-front',
  'lucide:trash-2',
  'lucide:triangle-alert',
  'lucide:tv',
  'lucide:twitter',
  'lucide:user-key',
  'lucide:users',
  'lucide:video',
  'lucide:wallet',
  'lucide:wifi',
  'lucide:wrench',
  'lucide:youtube',
  'lucide:zap',
] as const

/** Shown when an entry has no icon or an unknown one. */
export const DEFAULT_ENTRY_ICON = 'lucide:key-round'

/** The colors of the picker (data, not theme: the user's choice for an entry). */
export const ENTRY_COLORS = [
  '#ef4444',
  '#f97316',
  '#eab308',
  '#22c55e',
  '#14b8a6',
  '#3b82f6',
  '#8b5cf6',
  '#ec4899',
  '#78716c',
] as const

export function isKnownIcon(name: string | null | undefined): boolean {
  return (
    typeof name === 'string' &&
    ((ENTRY_ICONS as readonly string[]).includes(name) ||
      (IMPORT_ICONS as readonly string[]).includes(name))
  )
}

/** What an `icon` value of an entry or folder shows: one of the pictures of the list, a picture of
 * its own that the import stored (`binary:<hash>`), or the default of the place. */
export type ResolvedIcon =
  | { kind: 'name'; name: string }
  | { kind: 'binary'; hash: string }
  | { kind: 'default' }

const BINARY_PREFIX = 'binary:'
const SHA256_HEX = /^[0-9a-f]{64}$/i

export function resolveIcon(value: string | null | undefined): ResolvedIcon {
  if (typeof value !== 'string' || value === '') return { kind: 'default' }
  if (value.startsWith(BINARY_PREFIX)) {
    const hash = value.slice(BINARY_PREFIX.length)
    return SHA256_HEX.test(hash)
      ? { kind: 'binary', hash: hash.toLowerCase() }
      : { kind: 'default' }
  }
  return isKnownIcon(value)
    ? { kind: 'name', name: value }
    : { kind: 'default' }
}

/** The mime type of a picture by its first bytes, so a blob URL shows it whatever the source called
 * it; `image/png` when nothing matches (the browser sniffs the rest). */
export function sniffImageType(bytes: Uint8Array): string {
  const startsWith = (...signature: number[]) =>
    signature.every((byte, index) => bytes[index] === byte)
  if (startsWith(0xff, 0xd8, 0xff)) return 'image/jpeg'
  if (startsWith(0x47, 0x49, 0x46, 0x38)) return 'image/gif'
  if (
    startsWith(0x52, 0x49, 0x46, 0x46) &&
    bytes[8] === 0x57 &&
    bytes[9] === 0x45 &&
    bytes[10] === 0x42 &&
    bytes[11] === 0x50
  )
    return 'image/webp'
  return 'image/png'
}
