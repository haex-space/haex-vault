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

  const openForNewDevice = () => {
    mode.value = 'new-device'
    targetSpaceId.value = null
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
