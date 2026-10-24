// What a group of vaults and devices is going to be, decided before anything starts: who the users
// are, which devices they have, their names and folders, and the limit of devices (FR-012, FR-019).
// Pure, so the rules have display-free checks.
import { deviceFolder } from './device-folder.ts'

/** Default for `E2E_MAX_DEVICES`: each device is an application, a screen and a driver. */
export const DEFAULT_MAX_DEVICES = 6

/** A device as a scenario names it: a name, or a name with `main: true` for a second main device. */
export type DeviceSpec = string | { name: string; main?: boolean }

export interface GroupSpec {
  /** User name to the devices of that user's vault; the first device creates the vault. */
  users: Record<string, DeviceSpec[]>
  /**
   * Starts the group's own iroh relay and points every device at it, as a run with a phone does.
   * Vaults of different users meet only through a relay: a card names a device by its id and its
   * relay, and the devices of two vaults share no other way to find each other.
   */
  irohRelay?: boolean
}

export interface PlannedDevice {
  user: string
  name: string
  /** `<user>/<device>`. */
  address: string
  folder: string
  /** The device that creates the vault; the others are linked to it. */
  first: boolean
  /** Linked as a main device (the first device always is one). */
  main: boolean
}

export interface PlannedUser {
  name: string
  vaultName: string
  devices: PlannedDevice[]
}

/** A name that is safe in a vault name, a window and a folder. */
const NAME = /^[A-Za-z0-9][A-Za-z0-9_-]*$/

/** Whether a user or device name is safe in a vault name, a window and a folder. */
export function usableName(name: string): boolean {
  return NAME.test(name)
}

/** Reads E2E_MAX_DEVICES, using the default when unset or empty; rejects nonpositive integers and fractions. */
export function maxDevicesFrom(env: NodeJS.ProcessEnv): number {
  const value = env.E2E_MAX_DEVICES
  if (value === undefined || value === '') return DEFAULT_MAX_DEVICES
  const number = Number(value)
  if (!Number.isInteger(number) || number < 1) {
    throw new Error(
      `E2E_MAX_DEVICES must be a whole number of 1 or more, got "${value}"`,
    )
  }
  return number
}

/** Describes an exceeded device limit and the environment variable that changes it. */
export function limitMessage(count: number, max: number): string {
  return `a group of ${count} devices is more than the limit of ${max}; set E2E_MAX_DEVICES to change it`
}

/** Checks a group and returns what to start; throws before anything is started. */
export function planGroup(spec: GroupSpec, maxDevices: number): PlannedUser[] {
  const users = Object.entries(spec.users)
  if (users.length === 0) throw new Error('a group needs at least one user')
  const count = users.reduce((sum, [, devices]) => sum + devices.length, 0)
  if (count > maxDevices) throw new Error(limitMessage(count, maxDevices))
  const owner = new Map<string, string>()
  return users.map(([user, devices]) => {
    if (!usableName(user))
      throw new Error(`"${user}" is not a usable user name`)
    if (devices.length === 0) {
      throw new Error(`user "${user}" needs at least one device`)
    }
    return {
      name: user,
      vaultName: `e2e-${user}`,
      devices: devices.map((device, index) => {
        const name = typeof device === 'string' ? device : device.name
        if (!usableName(name)) {
          throw new Error(`"${name}" is not a usable device name`)
        }
        const other = owner.get(name)
        if (other !== undefined) {
          throw new Error(
            other === user
              ? `device "${name}" appears twice for user "${user}"`
              : `device "${name}" is used by users "${other}" and "${user}"; device names are unique in a group`,
          )
        }
        owner.set(name, user)
        const main =
          index === 0 || (typeof device !== 'string' && device.main === true)
        return {
          user,
          name,
          address: `${user}/${name}`,
          folder: deviceFolder(user, name),
          first: index === 0,
          main,
        }
      }),
    }
  })
}

export type DeviceState = 'stopped' | 'running' | 'offline' | 'killed'
export type DeviceOperation =
  'start' | 'stop' | 'kill' | 'goOffline' | 'goOnline'

/**
 * The state after an operation, or an error. A device that was taken offline starts offline again,
 * because its servers are stored none (research R2).
 */
export function nextState(
  state: DeviceState,
  operation: DeviceOperation,
  offlineMode: boolean,
): DeviceState {
  const refuse = (): never => {
    throw new Error(`cannot ${operation} a device that is ${state}`)
  }
  switch (operation) {
    case 'start':
      return state === 'stopped' || state === 'killed'
        ? offlineMode
          ? 'offline'
          : 'running'
        : refuse()
    case 'stop':
      return state === 'running' || state === 'offline' ? 'stopped' : refuse()
    case 'kill':
      return state === 'running' || state === 'offline' ? 'killed' : refuse()
    case 'goOffline':
      return state === 'running' ? 'offline' : refuse()
    case 'goOnline':
      return state === 'offline' ? 'running' : refuse()
  }
}

/** What decides where the next device of a group runs in a run with a phone. */
export interface PhonePlace {
  /** A device of the group already runs on the phone. */
  taken: boolean
  /** The group was made with a device named `phone`. */
  named: boolean
  /** No device of the group exists yet. */
  empty: boolean
}

/**
 * Whether a device runs on the phone of an Android run (spec 043, contract e2e-android.md): the device
 * named `phone`, else the first device of a group made without one; the others run on Linux. A group
 * has at most one device on the phone, since the phone runs one app at a time.
 */
export function runsOnPhone(name: string, place: PhonePlace): boolean {
  if (place.taken) return false
  return name === 'phone' || (!place.named && place.empty)
}
