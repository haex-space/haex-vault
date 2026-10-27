// Helpers for the scene of channels between extensions (spec 057): exchanging Visitenkarten, and
// calling the test extension `peers` inside its frame (`window.peersProbe`, see
// `src-tauri/tests/fixtures/extension_e2e/peers/main.js`).
import { unwrap } from './flows.ts'
import { inFrame } from './extensions.ts'
import type { InstalledExtension } from './extensions.ts'
import type { Page } from './page.ts'

/** An answer of the test extension: what its call returned, or the failure of the SDK. */
export interface ProbeAnswer<T = unknown> {
  ok?: T
  error?: {
    code: number | null
    reason: string | null
    kind: string | null
    limit: number | null
    message: string
  }
}

/** An event the test extension kept. */
export interface ProbeEvent {
  type: string
  data: Record<string, unknown>
  at: number
}

/** A device of the scene: its page and the test extension installed on it. */
export interface PeerDevice {
  page: Page
  extension: InstalledExtension
}

/** Calls `window.peersProbe[method](…args)` in the frame of the test extension. */
async function callProbe<T>(
  device: PeerDevice,
  method: string,
  args: unknown[],
): Promise<T> {
  return inFrame<T>(
    device.page,
    device.extension,
    `return Promise.resolve(window.peersProbe[arguments[0]](...Array.prototype.slice.call(arguments, 1)))
      .then((answer) => JSON.parse(JSON.stringify(answer ?? null)))`,
    [method, ...args],
  )
}

/** A call that answers `{ ok }` or `{ error }`: the SDK's failure is data, not a rejection. */
export function probe<T = unknown>(
  device: PeerDevice,
  method: string,
  ...args: unknown[]
): Promise<ProbeAnswer<T>> {
  return callProbe<ProbeAnswer<T>>(device, method, args)
}

/** What the extension has kept (`events`, `channel`, `delays`). */
export function read<T>(
  device: PeerDevice,
  method: string,
  ...args: unknown[]
): Promise<T> {
  return callProbe<T>(device, method, args)
}

/** The call's result; a failed call is an error with the reason the SDK named. */
export async function succeeded<T>(
  device: PeerDevice,
  method: string,
  ...args: unknown[]
): Promise<T> {
  const answer = await probe<T>(device, method, ...args)
  if (answer.error !== undefined) {
    throw new Error(`${method} failed: ${JSON.stringify(answer.error)}`)
  }
  return answer.ok as T
}

/** The events of one type the test extension got so far. */
export async function eventsOf(
  device: PeerDevice,
  type: string,
): Promise<ProbeEvent[]> {
  return read<ProbeEvent[]>(device, 'events', type)
}

/** Reads a share or a Visitenkarte in as a new contact with the defaults; the contact's id. */
export async function acceptCard(page: Page, text: string): Promise<string> {
  return unwrap<{ id: string }>(
    'contacts_share_accept',
    await page.invoke('contacts_share_accept', {
      args: { text, target: { kind: 'new' }, decisions: [] },
    }),
  ).id
}
