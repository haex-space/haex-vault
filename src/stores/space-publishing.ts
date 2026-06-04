/**
 * Trigger state for the Space-Publishing dialog.
 *
 * The dialog is opened from two places:
 * - After a fresh device row was created via the Reconciliation flow
 *   (`mode: 'new-device'`), so the user can pick which of their spaces this
 *   device should publish into.
 * - After joining/creating a space (`mode: 'new-space'`, `targetSpaceId` set),
 *   so the user can pick which of their devices should publish into the new
 *   space.
 *
 * The dialog component lives in `HaexDeviceReconciliationSpacePublishingDialog`
 * and reads `isOpen` / `mode` from this store.
 */
export type SpacePublishingMode = 'new-device' | 'new-space'

export const useSpacePublishingStore = defineStore('spacePublishingStore', () => {
  const mode = ref<SpacePublishingMode | null>(null)
  const targetSpaceId = ref<string | null>(null)

  const isOpen = computed(() => mode.value !== null)

  /**
   * Open the dialog after a fresh device row was created via the Welcome flow.
   * Only meaningful when the user belongs to at least one space they do NOT
   * own — owned spaces (personal/default + self-created) already know this
   * device's endpoint. Centralising the gate here means every caller (welcome
   * flow today, settings re-run / recovery flows later) gets it for free.
   * Returns true when the dialog actually opened.
   */
  const openForNewDevice = (): boolean => {
    const spacesStore = useSpacesStore()
    if (spacesStore.foreignSpaces.length === 0) return false
    mode.value = 'new-device'
    targetSpaceId.value = null
    return true
  }

  const openForNewSpace = (spaceId: string) => {
    mode.value = 'new-space'
    targetSpaceId.value = spaceId
  }

  const close = () => {
    mode.value = null
    targetSpaceId.value = null
  }

  return {
    mode,
    targetSpaceId,
    isOpen,
    openForNewDevice,
    openForNewSpace,
    close,
  }
})
