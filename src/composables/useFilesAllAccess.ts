/**
 * "Zugriff auf alle Dateien" on Android (spec 044 FR-039, research R11): checked at the start and
 * again whenever the window comes back to the front, so a grant in the system's settings shows
 * without a restart; `onGranted` runs once it turns granted. Other systems always have it, and
 * nothing is asked there.
 */
export function useFilesAllAccess(onGranted: () => void) {
  const { isAndroid } = useDeviceCapabilities()
  const { allAccessAsync, requestAllAccessAsync } = useFiles()
  /** `null` until the first answer on Android. */
  const granted = ref<boolean | null>(isAndroid.value ? null : true)

  async function checkAsync() {
    if (!isAndroid.value) return
    const before = granted.value
    granted.value = await allAccessAsync().catch(() => false)
    if (before === false && granted.value) onGranted()
  }

  async function requestAsync() {
    await requestAllAccessAsync().catch((error: unknown) => {
      console.error('[files] the setting for all files did not open', error)
    })
    await checkAsync()
  }

  onMounted(checkAsync)
  useEventListener(document, 'visibilitychange', () => {
    if (document.visibilityState === 'visible') void checkAsync()
  })
  useEventListener(window, 'focus', () => void checkAsync())

  return { granted, requestAsync }
}
