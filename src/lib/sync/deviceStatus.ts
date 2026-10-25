// What the device list says about a device (spec 024, user story 4, FR-033 to FR-035). Pure — the
// components turn these structured values into localized text, so the logic is testable alone.

/** The fields of a device this module reads; `VaultDevice` has them. */
export type DeviceFacts = {
  online: boolean
  /** Milliseconds since the epoch; `null` when this device knows of no time. */
  lastSeen: number | null
  alias: string | null
  isCurrent: boolean
}

export type DeviceStatus =
  { kind: 'online' } | { kind: 'neverSeen' } | { kind: 'lastSeen'; at: number }

/** FR-033: connected now is "online", else the last time this device knows of, else "never seen". */
export function deviceStatus(device: DeviceFacts): DeviceStatus {
  if (device.online) return { kind: 'online' }
  if (device.lastSeen === null) return { kind: 'neverSeen' }
  return { kind: 'lastSeen', at: device.lastSeen }
}

export type Elapsed =
  { unit: 'justNow' } | { unit: 'minutes' | 'hours' | 'days'; value: number }

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR

/** How long ago `at` was, in the largest whole unit. A time in the future (a device with a wrong
 * clock) reads as just now. */
export function elapsedSince(at: number, now: number): Elapsed {
  const ago = Math.max(0, now - at)
  if (ago < MINUTE) return { unit: 'justNow' }
  if (ago < HOUR) return { unit: 'minutes', value: Math.floor(ago / MINUTE) }
  if (ago < DAY) return { unit: 'hours', value: Math.floor(ago / HOUR) }
  return { unit: 'days', value: Math.floor(ago / DAY) }
}

/** The i18n key of a role's label. */
export function roleLabelKey(role: 'main' | 'linked'): string {
  return role === 'main'
    ? 'settings.federation.role.main'
    : 'settings.federation.role.linked'
}

/** The i18n key of the reason sync with a device is halted (FR-029, FR-030). */
export function problemLabelKey(
  problem: 'incompatible_version' | 'duplicate',
): string {
  return `settings.federation.problem.${problem}`
}

/** The three values two devices compare before they sync (spec 059); `VaultDevice.schema` has them. */
export type SchemaFacts = {
  protocol: number
  holziMigration: number
  crdtTrigger: number
}

export type SchemaField = keyof SchemaFacts

/** The values in the order the list shows them. */
export const SCHEMA_FIELDS: readonly SchemaField[] = [
  'protocol',
  'holziMigration',
  'crdtTrigger',
]

/** How a device's value stands against this device's: `lower` means it is the smaller one. */
export type SchemaOrder = 'equal' | 'lower' | 'higher'

export type SchemaDiff = Record<SchemaField, SchemaOrder>

/** Each value of `other` against `own` (spec 059 FR-003); `null` while either state is unknown. */
export function schemaDiff(
  own: SchemaFacts | null,
  other: SchemaFacts | null,
): SchemaDiff | null {
  if (own === null || other === null) return null
  const order = (field: SchemaField): SchemaOrder =>
    other[field] === own[field]
      ? 'equal'
      : other[field] < own[field]
        ? 'lower'
        : 'higher'
  return {
    protocol: order('protocol'),
    holziMigration: order('holziMigration'),
    crdtTrigger: order('crdtTrigger'),
  }
}

/** The i18n key of the reason sync with a device is halted. A version problem names the device to
 * update when every differing value points the same way (spec 059 FR-012): the other device when
 * all of its values are lower, this one when all are higher; else the plain text. */
export function problemMessageKey(
  problem: 'incompatible_version' | 'duplicate',
  diff: SchemaDiff | null,
): string {
  const plain = problemLabelKey(problem)
  if (problem !== 'incompatible_version' || diff === null) return plain
  const orders = SCHEMA_FIELDS.map((field) => diff[field]).filter(
    (order) => order !== 'equal',
  )
  if (orders.length === 0) return plain
  if (orders.every((order) => order === 'lower')) return `${plain}_other`
  if (orders.every((order) => order === 'higher')) return `${plain}_this`
  return plain
}

/** This device first, then the others by name ignoring case, devices without a name last. The
 * backend sorts the same way; this keeps a list sorted after a local update. */
export function sortDevices<T extends DeviceFacts>(devices: readonly T[]): T[] {
  const key = (device: T) =>
    [
      device.isCurrent ? 0 : 1,
      device.alias === null || device.alias.trim() === '' ? 1 : 0,
      (device.alias ?? '').toLowerCase(),
    ] as const
  return [...devices].sort((a, b) => {
    const [a0, a1, a2] = key(a)
    const [b0, b1, b2] = key(b)
    return a0 - b0 || a1 - b1 || (a2 < b2 ? -1 : a2 > b2 ? 1 : 0)
  })
}

/** FR-035: only a main device links, admits and removes devices. */
export function canManageDevices(
  thisDevice: 'main' | 'linked' | 'awaiting_admission' | 'removed' | null,
): boolean {
  return thisDevice === 'main'
}
