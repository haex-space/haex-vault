export type IconDescriptor =
  | { kind: 'iconify'; name: string }
  | { kind: 'binary'; hash: string }

// Legacy haex-pass icon identifiers mapped to Iconify names (rendered via <UIcon>).
// Preserves the source-of-truth names stored in haex_passwords_item_details.icon
// so existing vault data keeps working.
const iconMap: Record<string, string> = {
  'key': 'i-lucide-key',
  'folder': 'i-lucide-folder',
  'folder-lock': 'i-lucide-folder-lock',
  'mail': 'i-lucide-mail',
  'shield': 'i-lucide-shield',
  'lock': 'i-lucide-lock',
  'credit-card': 'i-lucide-credit-card',
  'shopping-cart': 'i-lucide-shopping-cart',
  'message': 'i-lucide-message-square',
  'message-square': 'i-lucide-message-square',
  'message-circle': 'i-lucide-message-circle',
  'laptop': 'i-lucide-laptop',
  'briefcase': 'i-lucide-briefcase',
  'home': 'i-lucide-home',
  'star': 'i-lucide-star',
  'heart': 'i-lucide-heart',
  'tag': 'i-lucide-tag',
  'bookmark': 'i-lucide-bookmark',
  'calendar': 'i-lucide-calendar',
  'clock': 'i-lucide-clock',
  'file': 'i-lucide-file-text',
  'gift': 'i-lucide-gift',
  'phone': 'i-lucide-phone',
  'user': 'i-lucide-user',
  'camera': 'i-lucide-camera',
  'video': 'i-lucide-video',
  'music': 'i-lucide-music',
  'headphones': 'i-lucide-headphones',
  'map-pin': 'i-lucide-map-pin',
  'plane': 'i-lucide-plane',
  'car': 'i-lucide-car',
  'ticket': 'i-lucide-ticket',
  'github': 'i-lucide-github',
  'wrench': 'i-lucide-wrench',
  'code': 'i-lucide-code',
  'server': 'i-lucide-server',
  'database': 'i-lucide-database',
  'cloud': 'i-lucide-cloud',
  'wifi': 'i-lucide-wifi',
  'lightbulb': 'i-lucide-lightbulb',
  'rocket': 'i-lucide-rocket',
  'twitter': 'i-lucide-twitter',
  'facebook': 'i-lucide-facebook',
  'linkedin': 'i-lucide-linkedin',
  'instagram': 'i-lucide-instagram',
  'youtube': 'i-lucide-youtube',
  'tv': 'i-lucide-tv',
  'apple': 'i-lucide-apple',
  'globe': 'i-lucide-globe',
}

export const useIconComponents = () => {
  const icons = Object.keys(iconMap)

  const getIconDescriptor = (
    iconName: string | null | undefined,
    fallback: string = 'i-lucide-key',
  ): IconDescriptor => {
    if (!iconName) return { kind: 'iconify', name: fallback }

    if (iconName.startsWith('binary:')) {
      return { kind: 'binary', hash: iconName.slice('binary:'.length) }
    }

    // Full Iconify identifier (e.g. "mdi:github" or "i-lucide-globe")
    if (iconName.includes(':') || iconName.startsWith('i-')) {
      return { kind: 'iconify', name: iconName }
    }

    return { kind: 'iconify', name: iconMap[iconName] ?? fallback }
  }

  const getTextColor = (backgroundColor: string): string => {
    const hex = backgroundColor.replace('#', '')
    const r = parseInt(hex.substring(0, 2), 16)
    const g = parseInt(hex.substring(2, 4), 16)
    const b = parseInt(hex.substring(4, 6), 16)
    const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255
    return luminance > 0.5 ? '#000000' : '#ffffff'
  }

  return {
    iconMap,
    icons,
    getIconDescriptor,
    getTextColor,
  }
}
