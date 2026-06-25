import type { SelectHaexDesktopItems } from '~/database/schemas'

export type DesktopItemType = 'extension' | 'file' | 'folder' | 'system'

export interface IDesktopItem extends SelectHaexDesktopItems {
  label?: string
  icon?: string
  referenceId: string // Computed: extensionId or systemWindowId
}
