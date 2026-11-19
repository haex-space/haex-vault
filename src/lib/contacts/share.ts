// The choice of the share dialog (spec 060 FR-009 to FR-012, FR-015, FR-017): everything chosen
// when it opens, each row on its own, "all" and "none", and the arguments of the backend call. The
// choice is never stored (FR-010). Pure, so `scripts/check-contacts-share.ts` runs it without vue.

// The shapes of the generated bindings (`ContactShareOptions`, `ContactShareSelection`) as far as
// the choice uses them; declared here so the check script loads this file without the bindings.
export type ShareRow = { id: string }

export const HEAD_ROWS = [
  'displayName',
  'nickname',
  'organization',
  'jobTitle',
  'note',
] as const
export type HeadRow = (typeof HEAD_ROWS)[number]

export type ShareDevice = { devicePubkey: string }

export type ShareHaex =
  | { kind: 'none' }
  | { kind: 'card' }
  | { kind: 'own'; canSign: boolean; devices: ShareDevice[]; relays: string[] }

export type ShareOptions = {
  rows: ShareRow[]
  head: Partial<Record<HeadRow, string | null>>
  haex: ShareHaex
  name: string
  canShare: boolean
}

export type HaexChoice =
  | { kind: 'none' }
  | { kind: 'card' }
  | { kind: 'own'; devices: string[]; relays: string[] }

export type ShareSelection = {
  values: string[]
  displayName: boolean
  nickname: boolean
  organization: boolean
  jobTitle: boolean
  note: boolean
  haex: HaexChoice
}

function hasHead(options: ShareOptions, row: HeadRow): boolean {
  return Boolean(options.head[row])
}

/** Everything the dialog offers, chosen (FR-009). The own vault's identity always goes along, so
 * the own choice exists even with no device and no relay. */
export function selectAll(options: ShareOptions): ShareSelection {
  const haex = options.haex
  return {
    values: options.rows.map((row) => row.id),
    displayName: hasHead(options, 'displayName'),
    nickname: hasHead(options, 'nickname'),
    organization: hasHead(options, 'organization'),
    jobTitle: hasHead(options, 'jobTitle'),
    note: hasHead(options, 'note'),
    haex:
      haex.kind === 'own'
        ? {
            kind: 'own',
            devices: haex.devices.map((device) => device.devicePubkey),
            relays: [...haex.relays],
          }
        : haex.kind === 'card'
          ? { kind: 'card' }
          : { kind: 'none' },
  }
}

/** Nothing of the rows chosen; the own identity stays (FR-012). */
export function selectNone(options: ShareOptions): ShareSelection {
  return {
    values: [],
    displayName: false,
    nickname: false,
    organization: false,
    jobTitle: false,
    note: false,
    haex:
      options.haex.kind === 'own'
        ? { kind: 'own', devices: [], relays: [] }
        : { kind: 'none' },
  }
}

function toggled(list: readonly string[], item: string): string[] {
  return list.includes(item)
    ? list.filter((other) => other !== item)
    : [...list, item]
}

export function toggleValue(
  selection: ShareSelection,
  id: string,
): ShareSelection {
  return { ...selection, values: toggled(selection.values, id) }
}

export function toggleHead(
  selection: ShareSelection,
  row: HeadRow,
): ShareSelection {
  return { ...selection, [row]: !selection[row] }
}

/** The one choice of a foreign contact's confirmed addresses: the original proof goes or not,
 * devices and relays never separately (FR-013). */
export function toggleCard(selection: ShareSelection): ShareSelection {
  if (selection.haex.kind === 'own') return selection
  return {
    ...selection,
    haex: selection.haex.kind === 'card' ? { kind: 'none' } : { kind: 'card' },
  }
}

export function toggleDevice(
  selection: ShareSelection,
  devicePubkey: string,
): ShareSelection {
  if (selection.haex.kind !== 'own') return selection
  return {
    ...selection,
    haex: {
      ...selection.haex,
      devices: toggled(selection.haex.devices, devicePubkey),
    },
  }
}

export function toggleRelay(
  selection: ShareSelection,
  relay: string,
): ShareSelection {
  if (selection.haex.kind !== 'own') return selection
  return {
    ...selection,
    haex: {
      ...selection.haex,
      relays: toggled(selection.haex.relays, relay),
    },
  }
}

/** Whether the choice gives anything to share (FR-017): a name, a detail, a head field or a card
 * that can go (the own one only where this device can sign it, FR-016). */
export function canBuild(
  options: ShareOptions,
  selection: ShareSelection,
): boolean {
  if (options.name) return true
  if (selection.values.length > 0) return true
  if (HEAD_ROWS.some((row) => selection[row])) return true
  const haex = selection.haex
  if (haex.kind === 'card') return true
  return (
    haex.kind === 'own' && options.haex.kind === 'own' && options.haex.canSign
  )
}

/** The arguments of the backend call. A foreign contact's choice never names devices or relays. */
export function toArgs(selection: ShareSelection): ShareSelection {
  return {
    ...selection,
    values: [...selection.values],
    haex:
      selection.haex.kind === 'own'
        ? {
            kind: 'own',
            devices: [...selection.haex.devices],
            relays: [...selection.haex.relays],
          }
        : { kind: selection.haex.kind },
  }
}
