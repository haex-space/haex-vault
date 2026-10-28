/**
 * What a click on a drive does (spec 064, contracts/ui.md): a mounted drive opens, an unmounted one
 * is mounted first (FR-006). A cancelled system prompt or a failure shows a short notice, never a
 * dialog (FR-007).
 */
import { toast } from 'vue-sonner'
import type { Drive } from '@bindings/Drive'
import { asFilesError } from '~/composables/useFiles'
import { driveLabel } from '~/lib/files/drives'
import { formatFileSize } from '~/lib/passwords/format'

export function useFilesDriveActions() {
  const { t } = useI18n()
  const { driveOpenAsync } = useFiles()
  const { busy, setBusy } = useFilesDrives()

  function nameOf(drive: Drive): string {
    return driveLabel(drive, (size) =>
      t('files.drives.unnamed', {
        size: size === null ? '?' : formatFileSize(size),
      }),
    )
  }

  /** The path to open, or `null` when the drive did not open (the notice says why). */
  async function openDriveAsync(drive: Drive): Promise<string | null> {
    if (drive.state === 'mounted' && drive.path !== null) return drive.path
    if (busy.value.has(drive.id)) return null
    setBusy(drive.id, true)
    try {
      const opened = await driveOpenAsync(drive.id)
      if (opened.kind === 'opened') return opened.path
      // Unlocking comes with spec 064 US3.
      toast(t('files.drives.unsupported', { name: nameOf(drive) }))
      return null
    } catch (cause) {
      notifyFailure(drive, cause)
      return null
    } finally {
      setBusy(drive.id, false)
    }
  }

  function notifyFailure(drive: Drive, cause: unknown) {
    const error = asFilesError(cause)
    const name = nameOf(drive)
    switch (error?.code) {
      case 'driveCancelled':
        toast(t('files.drives.cancelled'))
        return
      case 'driveGone':
        toast(t('files.drives.gone', { name }))
        return
      case 'driveUnsupported':
        toast(t('files.drives.unsupported', { name }))
        return
      default:
        toast.error(
          t('files.drives.failed', {
            name,
            message: error?.message ?? String(cause),
          }),
        )
    }
  }

  return { nameOf, openDriveAsync }
}
