/**
 * "Zugriff auf alle Dateien" for the file browser (spec 044 US7, FR-039): checked when the browser
 * opens and again whenever the window gets the focus back, so a grant in the system settings shows
 * the files without a restart. Only Android asks; elsewhere the first answer is final. Until the
 * answer is there, `missing` is false, so nothing flickers on a desktop.
 */
export function useFilesAllAccess(onGranted: () => void) {
  const { allAccessAsync, requestAllAccessAsync } = useFiles()
  const missing = ref(false)
  let android = false

  function take(granted: boolean) {
    const was = missing.value
    missing.value = !granted
    if (was && granted) onGranted()
  }

  async function checkAsync() {
    try {
      const answer = await allAccessAsync()
      android = answer.platform === 'android'
      take(answer.granted)
    } catch (error: unknown) {
      // Then the folder's own error says what is wrong.
      console.warn('files_all_access failed', error)
    }
  }

  async function requestAsync() {
    take((await requestAllAccessAsync()).granted)
  }

  function onFocus() {
    if (android && document.visibilityState !== 'hidden') void checkAsync()
  }

  onMounted(() => {
    void checkAsync()
    window.addEventListener('focus', onFocus)
    document.addEventListener('visibilitychange', onFocus)
  })
  onBeforeUnmount(() => {
    window.removeEventListener('focus', onFocus)
    document.removeEventListener('visibilitychange', onFocus)
  })

  return { missing, requestAsync }
}
