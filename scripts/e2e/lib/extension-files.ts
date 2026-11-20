// The device side of the files scene (spec 017, US9): a folder of the device the probe reads,
// writes and watches, outside haex-vault's own places.
import { deviceFiles as platformDeviceFiles } from './platform/device-files.ts'
import type { DeviceFiles } from './platform/device-files.ts'

/** A fresh folder of the device under test for files the app reads and writes (see `platform/device-files.ts`). */
export function deviceFiles(prefix = 'haex-ext-files-'): DeviceFiles {
  return platformDeviceFiles(prefix)
}
