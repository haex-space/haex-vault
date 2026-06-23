import {
  DesktopIconSizePreset,
  iconSizePresetValues,
} from '~/stores/vault/settings'

export const ICON_PADDING = 30

export function useDesktopGrid() {
  const settingsStore = useVaultSettingsStore()

  const iconSizePreset = ref<DesktopIconSizePreset>(
    DesktopIconSizePreset.medium,
  )

  const syncDesktopIconSizeAsync = async () => {
    const preset = await settingsStore.syncDesktopIconSizeAsync()
    iconSizePreset.value = preset
  }

  const updateDesktopIconSizeAsync = async (preset: DesktopIconSizePreset) => {
    await settingsStore.updateDesktopIconSizeAsync(preset)
    iconSizePreset.value = preset
  }

  const effectiveIconSize = computed(() => {
    return iconSizePresetValues[iconSizePreset.value]
  })

  const gridCellSize = computed(() => {
    return effectiveIconSize.value + ICON_PADDING
  })

  const snapToGrid = (
    x: number,
    y: number,
    iconWidth?: number,
    iconHeight?: number,
  ) => {
    const cellSize = gridCellSize.value
    const halfCell = cellSize / 2

    const actualIconWidth = iconWidth || effectiveIconSize.value
    const actualIconHeight = iconHeight || effectiveIconSize.value

    const centerX = x + actualIconWidth / 2
    const centerY = y + actualIconHeight / 2

    const col = Math.round((centerX - halfCell) / cellSize)
    const row = Math.round((centerY - halfCell) / cellSize)

    const gridCenterX = halfCell + col * cellSize
    const gridCenterY = halfCell + row * cellSize

    const snappedX = gridCenterX - actualIconWidth / 2
    const snappedY = gridCenterY - actualIconHeight / 2

    return {
      x: snappedX,
      y: snappedY,
    }
  }

  return {
    iconSizePreset,
    syncDesktopIconSizeAsync,
    updateDesktopIconSizeAsync,
    effectiveIconSize,
    gridCellSize,
    snapToGrid,
  }
}

export type DesktopGrid = ReturnType<typeof useDesktopGrid>
