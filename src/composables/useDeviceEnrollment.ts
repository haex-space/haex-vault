import { invoke } from '@tauri-apps/api/core'
import { eq, and } from 'drizzle-orm'
import { haexDeviceMlsEnrollments, haexSpaces } from '~/database/schemas'
import { createLogger } from '@/stores/logging'

const log = createLogger('DEVICE_ENROLLMENT')

function toBase64(data: Uint8Array): string {
  return btoa(String.fromCharCode(...data))
}

function fromBase64(b64: string): Uint8Array {
  const binary = atob(b64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return bytes
}

/**
 * Automatic device enrollment into MLS groups.
 *
 * Three operations:
 * 1. requestEnrollment: this device writes a pending enrollment for spaces it's not MLS-member of
 * 2. fulfillEnrollments: this device adds other devices to MLS groups it manages
 * 3. processEnrollments: this device processes Welcomes written by other devices
 */
export function useDeviceEnrollment() {
  const { currentVault } = storeToRefs(useVaultStore())
  const getDb = () => currentVault.value?.drizzle

  /**
   * Check all shared spaces and request MLS enrollment for ones this device isn't part of.
   */
  async function requestEnrollmentsAsync(deviceId: string) {
    const db = getDb()
    if (!db) return

    // Get all shared spaces (type != 'vault' and type != 'local-only')
    const spaces = await db.select({ id: haexSpaces.id, type: haexSpaces.type })
      .from(haexSpaces)
      .where(eq(haexSpaces.type, 'shared'))

    for (const space of spaces) {
      // Check if this device already has an MLS group
      const hasGroup: boolean = await invoke('mls_has_group', { spaceId: space.id })
      if (hasGroup) continue

      // Check if a pending enrollment already exists for this device + space
      const existing = await db.select({ id: haexDeviceMlsEnrollments.id })
        .from(haexDeviceMlsEnrollments)
        .where(and(
          eq(haexDeviceMlsEnrollments.spaceId, space.id),
          eq(haexDeviceMlsEnrollments.deviceId, deviceId),
        ))
        .limit(1)

      if (existing.length > 0) continue

      // Generate a KeyPackage and write pending enrollment
      const packages: number[][] = await invoke('mls_get_key_packages', { count: 1 })
      if (packages.length === 0) continue

      const keyPackageB64 = toBase64(new Uint8Array(packages[0]!))

      await db.insert(haexDeviceMlsEnrollments).values({
        spaceId: space.id,
        deviceId,
        keyPackage: keyPackageB64,
        status: 'pending',
      })

      log.info(`Requested MLS enrollment for space ${space.id}`)
    }
  }

  /**
   * Fulfill pending enrollments from other devices.
   * This device must be an MLS member of the space to add others.
   */
  async function fulfillEnrollmentsAsync(myDeviceDid: string) {
    const db = getDb()
    if (!db) return

    const pending = await db.select()
      .from(haexDeviceMlsEnrollments)
      .where(eq(haexDeviceMlsEnrollments.status, 'pending'))

    for (const enrollment of pending) {
      // Skip own enrollments
      if (enrollment.deviceId === myDeviceDid) continue

      // Check if we're an MLS member of this space
      const hasGroup: boolean = await invoke('mls_has_group', { spaceId: enrollment.spaceId })
      if (!hasGroup) continue

      try {
        // Add the device to the MLS group
        const keyPackage = fromBase64(enrollment.keyPackage)
        const bundle = await invoke<{ commit: number[]; welcome: number[] | null; groupInfo: number[] }>('mls_add_member', {
          spaceId: enrollment.spaceId,
          keyPackage: Array.from(keyPackage),
        })

        if (!bundle.welcome) {
          log.error(`No welcome generated for device ${enrollment.deviceId} in space ${enrollment.spaceId}`)
          continue
        }

        // Write Welcome back + mark as enrolled
        const welcomeB64 = toBase64(new Uint8Array(bundle.welcome))
        await db.update(haexDeviceMlsEnrollments)
          .set({ welcome: welcomeB64, status: 'enrolled' })
          .where(eq(haexDeviceMlsEnrollments.id, enrollment.id))

        // Export new epoch key (group state changed)
        await invoke('mls_export_epoch_key', { spaceId: enrollment.spaceId })

        log.info(`Enrolled device ${enrollment.deviceId} into MLS group for space ${enrollment.spaceId}`)
      } catch (err) {
        log.error(`Failed to fulfill enrollment for device ${enrollment.deviceId}:`, err)
      }
    }
  }

  /**
   * Process Welcomes for this device's enrollments.
   */
  async function processEnrollmentsAsync(myDeviceDid: string) {
    const db = getDb()
    if (!db) return

    const enrolled = await db.select()
      .from(haexDeviceMlsEnrollments)
      .where(and(
        eq(haexDeviceMlsEnrollments.deviceId, myDeviceDid),
        eq(haexDeviceMlsEnrollments.status, 'enrolled'),
      ))

    for (const enrollment of enrolled) {
      if (!enrollment.welcome) continue

      // Check if already processed (group exists)
      const hasGroup: boolean = await invoke('mls_has_group', { spaceId: enrollment.spaceId })
      if (hasGroup) {
        // Already processed, clean up
        await db.delete(haexDeviceMlsEnrollments)
          .where(eq(haexDeviceMlsEnrollments.id, enrollment.id))
        continue
      }

      try {
        const welcome = fromBase64(enrollment.welcome)
        await invoke('mls_process_message', {
          spaceId: enrollment.spaceId,
          message: Array.from(welcome),
        })

        // Clean up after successful processing
        await db.delete(haexDeviceMlsEnrollments)
          .where(eq(haexDeviceMlsEnrollments.id, enrollment.id))

        log.info(`Processed MLS welcome for space ${enrollment.spaceId}`)
      } catch (err) {
        log.error(`Failed to process enrollment welcome for space ${enrollment.spaceId}:`, err)
      }
    }
  }

  /**
   * Run all enrollment checks: request → fulfill → process.
   */
  async function syncEnrollmentsAsync(deviceId: string) {
    await processEnrollmentsAsync(deviceId)
    await fulfillEnrollmentsAsync(deviceId)
    await requestEnrollmentsAsync(deviceId)
  }

  return {
    requestEnrollmentsAsync,
    fulfillEnrollmentsAsync,
    processEnrollmentsAsync,
    syncEnrollmentsAsync,
  }
}
