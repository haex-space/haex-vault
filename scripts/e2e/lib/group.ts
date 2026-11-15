// Groups of vaults and devices for scenarios (contracts/group.md): users with their own vaults, each
// device an application over its own data, all pointed at one test relay. Everything a scenario asks of
// a platform goes through the driver layer (`platform/host.ts`); nothing here names a platform.
import { deviceFolder } from './device-folder.ts'
import { Device } from './device.ts'
import {
  DEFAULT_MAX_DEVICES,
  limitMessage,
  maxDevicesFrom,
  planGroup,
  runsOnPhone,
  usableName,
} from './group-plan.ts'
import type { GroupSpec, PlannedDevice } from './group-plan.ts'
import type { IrohRelay } from './iroh-relay.ts'
import type { NostrRelay } from './nostr-relay.ts'
import type { DeviceHost } from './platform/host.ts'
import { createVaultOnRelay, runLink } from './sync-flows.ts'
import type { WaitContext } from './sync-flows.ts'
import { readPageState } from './page-state.ts'
import type { PageState } from './page-state.ts'

export { Device, DEFAULT_MAX_DEVICES, maxDevicesFrom, planGroup }
export type { DeviceRow, SyncServers, ThisDevice } from './device.ts'
export type { DeviceState, GroupSpec } from './group-plan.ts'

/** What the failure material needs of a device (contracts/failure-material.md). */
export interface CaptureDevice {
  folder: string
  alive(): boolean
  screenshot(callLimitMs?: number): Promise<Uint8Array>
  keepData(folder: string): void
  /** What the device's page looks like (`page-state.txt`); a device that cannot say leaves it out. */
  pageState?(limitMs: number): Promise<PageState>
}

export interface GroupDeps extends WaitContext {
  host: DeviceHost
  /**
   * The phone of an Android run (spec 043): the device `runsOnPhone` picks runs there, the others on
   * `host`. Without it every device runs on `host`.
   */
  phoneHost?: DeviceHost
  relay: NostrRelay
  /** The iroh relays the devices use; none reachable unless the run has a phone (`sync-flows.ts`). */
  irohRelays?: string[]
  /** The group's own iroh relay, in a run with a phone. */
  irohRelay?: IrohRelay
  credentials(): { passphrase: string }
  onTeardown(action: () => Promise<void> | void): void
  /** Keep the data of passing scenarios too. */
  keep: boolean
  maxDevices: number
  /** Called once with a function that lists the devices for the failure material. */
  captureWith?: (devices: () => CaptureDevice[]) => void
}

export class Group {
  readonly deps: GroupDeps
  /** Every device by address (`<user>/<device>`). */
  readonly devices = new Map<string, Device>()
  /** The vault of each user. */
  readonly users = new Map<string, { vaultName: string }>()
  /** The group was made with a device named `phone` (it then runs on the phone, see `runsOnPhone`). */
  phoneNamed = false

  constructor(deps: GroupDeps) {
    this.deps = deps
  }

  get relay(): NostrRelay {
    return this.deps.relay
  }

  /**
   * The group's iroh relay, where it has one (a run with a phone): the devices also reach each other
   * there by an address they knew before, without any Nostr relay.
   */
  get irohRelay(): IrohRelay | undefined {
    return this.deps.irohRelay
  }

  device(address: string): Device {
    const device = this.devices.get(address)
    if (device === undefined) {
      throw new Error(
        `no device "${address}" in the group (${[...this.devices.keys()].join(', ')})`,
      )
    }
    return device
  }

  /**
   * A new device of an existing user that has no vault yet and does not run: the scenario links it,
   * for instance through the start page. It has a passphrase of its own, as a linked device does.
   */
  addDevice(user: string, name: string): Device {
    const entry = this.users.get(user)
    if (entry === undefined) {
      throw new Error(
        `no user "${user}" in the group (${[...this.users.keys()].join(', ')})`,
      )
    }
    return this.add(
      {
        user,
        name,
        address: `${user}/${name}`,
        folder: deviceFolder(user, name),
        first: false,
        main: false,
      },
      {
        vaultName: entry.vaultName,
        passphrase: this.deps.credentials().passphrase,
        role: 'linked',
      },
    )
  }

  /**
   * A new device of the host's user, linked the way a person does it with a code from the host (which
   * must be a running main device), and started. `main: true` makes it a main device too.
   */
  async link(
    host: Device,
    name: string,
    options: { main?: boolean } = {},
  ): Promise<Device> {
    if (this.devices.get(host.address) !== host) {
      throw new Error(`device ${host.address} is not a device of this group`)
    }
    if (host.role !== 'main') {
      throw new Error(
        `device ${host.address} is a linked device; only a main device can link another one`,
      )
    }
    if (host.state !== 'running') {
      throw new Error(
        `device ${host.address} is ${host.state}; it must be running to link another one`,
      )
    }
    return linkDeviceInto(this, host, {
      user: host.user,
      name,
      address: `${host.user}/${name}`,
      folder: deviceFolder(host.user, name),
      first: false,
      main: options.main === true,
    })
  }

  /** Where a new device named `name` runs. */
  hostFor(name: string): DeviceHost {
    return hostOf(this, name)
  }

  /**
   * Registers a device that does not run yet; refuses an unusable name, a name another device of the
   * group has (as `planGroup` does) and one more device than the limit allows.
   */
  add(
    planned: PlannedDevice,
    vault: { vaultName: string; passphrase: string; role: 'main' | 'linked' },
  ): Device {
    if (!usableName(planned.name)) {
      throw new Error(`"${planned.name}" is not a usable device name`)
    }
    const other = [...this.devices.values()].find(
      (device) => device.name === planned.name,
    )
    if (other !== undefined) {
      throw new Error(
        other.user === planned.user
          ? `device "${planned.address}" already exists in the group`
          : `device "${planned.name}" is used by users "${other.user}" and "${planned.user}"; device names are unique in a group`,
      )
    }
    if (this.devices.size + 1 > this.deps.maxDevices) {
      throw new Error(limitMessage(this.devices.size + 1, this.deps.maxDevices))
    }
    const host = this.hostFor(planned.name)
    const device = new Device(this, planned, {
      ...vault,
      host,
      data: host.newData(planned.folder),
    })
    this.devices.set(planned.address, device)
    return device
  }
}

/** Where a new device of the group runs: on the phone of an Android run or on the group's host. */
function hostOf(group: Group, name: string): DeviceHost {
  const phone = group.deps.phoneHost
  if (phone === undefined) return group.deps.host
  const devices = [...group.devices.values()]
  const onPhone = runsOnPhone(name, {
    taken: devices.some((device) => device.host === phone),
    named: group.phoneNamed,
    empty: devices.length === 0,
  })
  return onPhone ? phone : group.deps.host
}

/** Makes the group the spec describes and ends all of it with the scenario. */
export async function createGroup(
  deps: GroupDeps,
  spec: GroupSpec,
): Promise<Group> {
  const plan = planGroup(spec, deps.maxDevices)
  const group = new Group(deps)
  group.phoneNamed = plan.some((user) =>
    user.devices.some((device) => device.name === 'phone'),
  )
  deps.onTeardown(async () => {
    for (const device of [...group.devices.values()].reverse()) {
      try {
        if (device.state === 'running' || device.state === 'offline') {
          await device.stop()
        }
      } finally {
        if (!deps.keep) device.data.dispose()
      }
    }
  })
  deps.captureWith?.(() =>
    [...group.devices.values()].map((device) => ({
      folder: device.folder,
      alive: () => device.state === 'running' || device.state === 'offline',
      screenshot: (callLimitMs) => device.page.screenshot(callLimitMs),
      keepData: (folder: string) => device.data.keep(folder),
      pageState: (limitMs) => readPageState(device.page, limitMs),
    })),
  )
  for (const user of plan) {
    group.users.set(user.name, { vaultName: user.vaultName })
    const { passphrase } = deps.credentials()
    const [first, ...others] = user.devices
    if (first === undefined) continue
    const origin = group.add(first, {
      vaultName: user.vaultName,
      passphrase,
      role: 'main',
    })
    await createFirstDevice(group, origin)
    for (const planned of others) {
      await linkDeviceInto(group, origin, planned)
    }
  }
  return group
}

/** The first device creates the vault, points it at the relay, and reopens it so the servers apply. */
async function createFirstDevice(group: Group, device: Device): Promise<void> {
  const { relay, step, irohRelays } = group.deps
  const fresh = await device.host.start({
    data: device.data,
    folder: device.folder,
    step: (name, detail) => step(name, detail, device.address),
  })
  try {
    await createVaultOnRelay(
      fresh,
      relay.url,
      device.vaultName,
      device.passphrase,
      irohRelays,
    )
  } finally {
    await fresh.stop()
  }
  await device.start()
}

/** A further device of a user is linked the way a person does it, with a code from a main device. */
async function linkDeviceInto(
  group: Group,
  origin: Device,
  planned: PlannedDevice,
): Promise<Device> {
  const { relay, step, credentials, irohRelays } = group.deps
  const passphrase = credentials().passphrase
  const device = group.add(planned, {
    vaultName: origin.vaultName,
    passphrase,
    role: planned.main ? 'main' : 'linked',
  })
  const fresh = await device.host.start({
    data: device.data,
    folder: device.folder,
    step: (name, detail) => step(name, detail, device.address),
  })
  try {
    await runLink(group.deps, origin.page, fresh, {
      vaultName: origin.vaultName,
      deviceName: planned.name,
      passphrase,
      relayUrl: relay.url,
      irohRelays,
      asMainDevice: planned.main,
      device: device.address,
    })
  } finally {
    await fresh.stop()
  }
  await device.start()
  return device
}
