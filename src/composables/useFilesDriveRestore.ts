/**
 * A panel whose folder lies on a drive (spec 064 FR-025, research R10): when the folder is gone and
 * that drive is plugged in but not mounted, or locked, the panel says so (`DriveNotice.vue`) instead
 * of moving up. Once the drive is open, from the notice, the sidebar or the system, the panel shows
 * the same folder below its mount path, which may be a new one. A drive that is not there leaves the
 * panel to the nearest folder (`useFilesNearestFolder`). Nothing is mounted unasked.
 */
import type { Drive } from '@bindings/Drive'
import type { FilesError } from '@bindings/FilesError'
import type { FilesPanelContext } from '~/composables/useFilesSplit'
import { remap } from '~/lib/files/drives'

export function useFilesDriveRestore(
  context: FilesPanelContext,
  error: Ref<FilesError | null>,
  reload: () => unknown,
) {
  const place = context.place
  /** The folder that was gone while its drive was plugged in. */
  const waiting = ref<string | null>(null)

  const drive = computed((): Drive | null => {
    const key = place.value.drive
    if (!key) return null
    return (
      context.drives.value.find((candidate) => candidate.key === key) ?? null
    )
  })

  /** The folder of the place below `root`; `null` when the place names no drive above it. */
  function below(root: string): string | null {
    const { path, droot } = place.value
    return path && droot ? remap(path, droot, root) : null
  }

  /** The drive the panel waits for, while it is not open. */
  const notice = computed(() =>
    waiting.value !== null &&
    waiting.value === place.value.path &&
    drive.value?.state !== 'mounted'
      ? drive.value
      : null,
  )

  /** Shows the place's folder below `root`, the drive's mount path now. */
  function open(root: string) {
    const target = below(root)
    waiting.value = null
    if (target === null) return
    if (target === place.value.path) {
      void reload()
      return
    }
    context.go({ ...place.value, path: target, droot: root }, { replace: true })
  }

  // The drive opened while the panel waited for it.
  watch(
    () => [waiting.value, drive.value?.state, drive.value?.path] as const,
    ([gone, state, root]) => {
      if (
        gone !== null &&
        gone === place.value.path &&
        state === 'mounted' &&
        root
      )
        open(root)
    },
  )

  watch(
    () => place.value.path,
    () => {
      waiting.value = null
    },
  )
  watch(error, (now) => {
    if (now?.code !== 'notFound') waiting.value = null
  })

  /**
   * Whether the panel waits for the drive of the gone folder `gone` instead of moving up: the place
   * names a drive and that one is plugged in, except mounted where the place says (then the folder
   * itself is gone). Waits for the drive list first.
   */
  async function claimAsync(gone: string): Promise<boolean> {
    if (!place.value.drive) return false
    // A list that never comes leaves the panel to the walk.
    await until(context.drivesKnown).toBe(true, { timeout: 10_000 })
    const found = drive.value
    if (place.value.path !== gone || !found) return false
    if (found.state === 'mounted' && found.path && below(found.path) === gone)
      return false
    waiting.value = gone
    return true
  }

  return { notice, open, claimAsync }
}
