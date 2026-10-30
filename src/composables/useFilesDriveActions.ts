/**
 * What a click on a drive does (spec 064, contracts/ui.md): a mounted drive opens, an unmounted one
 * is mounted first (FR-006), a locked one opens with its saved password or asks for one (FR-013,
 * FR-014). A cancelled system prompt or a failure shows a short notice, never a dialog (FR-007).
 * Its button unmounts or ejects it (FR-008 to FR-012).
 */
import { toast } from 'vue-sonner'
import type { Drive } from '@bindings/Drive'
import type { DriveOpened } from '@bindings/DriveOpened'
import type { DriveRemember } from '@bindings/DriveRemember'
import type { DriveSecretInput } from '@bindings/DriveSecretInput'
import { asFilesError } from '~/composables/useFiles'
import { driveLabel } from '~/lib/files/drives'
import { formatFileSize } from '~/lib/passwords/format'

export function useFilesDriveActions() {
  const { t } = useI18n()
  const { driveOpenAsync, driveUnlockAsync, driveReleaseAsync } = useFiles()
  const { busy, setBusy } = useFilesDrives()

  function nameOf(drive: Drive): string {
    return driveLabel(drive, (size) =>
      t('files.drives.unnamed', {
        size: size === null ? '?' : formatFileSize(size),
      }),
    )
  }

  /**
   * What opening `drive` led to: `opened` with the path to show, or `needsPassword` for the unlock
   * dialog. `null` when it did not open (the notice says why).
   */
  async function openDriveAsync(
    drive: Drive,
  ): Promise<Exclude<DriveOpened, { kind: 'systemDialog' }> | null> {
    if (drive.state === 'mounted' && drive.path !== null)
      return { kind: 'opened', path: drive.path, rememberFailed: null }
    if (busy.value.has(drive.id)) return null
    setBusy(drive.id, true)
    try {
      const opened = await driveOpenAsync(drive.id)
      if (opened.kind !== 'systemDialog') return opened
      toast(t('files.drives.systemDialog', { name: nameOf(drive) }))
      return null
    } catch (cause) {
      notifyFailure(drive, cause)
      return null
    } finally {
      setBusy(drive.id, false)
    }
  }

  /**
   * Unlocks `drive` with the dialog's password. A wrong password comes back as `unlockFailed` with
   * the system's text for the dialog (FR-015); anything else shows a notice and is `null`.
   */
  async function unlockDriveAsync(
    drive: Drive,
    secret: DriveSecretInput,
    remember: DriveRemember,
  ): Promise<
    | {
        kind: 'opened'
        path: string
        rememberFailed: 'urlTaken' | 'failed' | null
      }
    | { kind: 'unlockFailed'; message: string }
    | null
  > {
    if (busy.value.has(drive.id)) return null
    setBusy(drive.id, true)
    try {
      const opened = await driveUnlockAsync(
        drive.id,
        secret,
        remember,
        t('files.drives.entryTitle', { name: nameOf(drive) }),
      )
      return opened.kind === 'opened' ? opened : null
    } catch (cause) {
      const error = asFilesError(cause)
      if (error?.code === 'unlockFailed')
        return { kind: 'unlockFailed', message: error.message }
      notifyFailure(drive, cause)
      return null
    } finally {
      setBusy(drive.id, false)
    }
  }

  /**
   * Unmounts or ejects `drive`. Returns the number of transfers still running on it when it was
   * left alone because of them (only without `cancelTransfers`), otherwise `null`.
   */
  async function releaseDriveAsync(
    drive: Drive,
    cancelTransfers: boolean,
  ): Promise<number | null> {
    if (busy.value.has(drive.id)) return null
    setBusy(drive.id, true)
    try {
      const released = await driveReleaseAsync(drive.id, cancelTransfers)
      if (released.kind === 'transfersRunning') return released.count
      const name = nameOf(drive)
      toast.success(
        released.ejected
          ? t('files.drives.ejected', { name })
          : t('files.drives.unmounted', { name }),
      )
      return null
    } catch (cause) {
      notifyFailure(drive, cause, 'files.drives.releaseCancelled')
      return null
    } finally {
      setBusy(drive.id, false)
    }
  }

  function notifyFailure(
    drive: Drive,
    cause: unknown,
    cancelled = 'files.drives.cancelled',
  ) {
    const error = asFilesError(cause)
    const name = nameOf(drive)
    switch (error?.code) {
      case 'driveCancelled':
        toast(t(cancelled))
        return
      case 'driveBusy':
        toast.error(t('files.drives.busy', { name }))
        return
      case 'driveGone':
        toast(t('files.drives.gone', { name }))
        return
      case 'driveUnsupported':
        toast(t('files.drives.unsupported', { name }))
        return
      case 'driveNoFilesystem':
        toast.error(t('files.drives.noFilesystem', { name }))
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

  return { nameOf, openDriveAsync, unlockDriveAsync, releaseDriveAsync }
}
