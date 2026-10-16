// Flows for scenarios with several devices of one vault (spec 024, T078): each device is its own
// application process with its own data, all pointed at one Nostr test relay. A device is made the
// way a user does it: the first by creating a vault, the others by linking with a code (user story 5).
import type { Page } from './page.ts'
import type { ScenarioContext } from './scenario.ts'
import { unwrap, waitForWorkspace } from './flows.ts'
import type { FlowInstance } from './flows.ts'

/** Devices on one machine find each other by address; the iroh relays would need a network, so they
 * point at a closed port, which refuses at once (as in `tests/common/sync_fixture.rs`). A run with a
 * phone passes the local iroh relay instead (spec 043): the phone is behind the emulator's own
 * network, which the other devices reach only through a relay. */
const NO_IROH_RELAY = ['https://127.0.0.1:1']

interface Thread {
  id: string
  title: string
}

/** Anything with a page to call backend commands on: a device of a group. */
export interface ThreadDevice {
  instance: Pick<Page, 'invoke' | 'exec'>
}

/** Only the test relay: the built-in servers, which would need a network, are switched off. */
export async function onlyServers(
  page: Page,
  relayUrl: string,
  irohRelays: string[] = NO_IROH_RELAY,
) {
  const defaults = unwrap<{ nostrRelays: string[]; irohRelays: string[] }>(
    'sync_servers_defaults',
    await page.invoke('sync_servers_defaults'),
  )
  return {
    nostrRelays: [relayUrl],
    irohRelays,
    disabled: [...defaults.nostrRelays, ...defaults.irohRelays],
  }
}

/** What the steps below need of a scenario: its timeline and its waits. */
export type WaitContext = Pick<ScenarioContext, 'waitFor' | 'step'>

/** Opens the vault in a started application and shows its workspace. */
export async function openVault(
  page: FlowInstance,
  vaultName: string,
  passphrase: string,
): Promise<void> {
  unwrap(
    'open_instance',
    await page.invoke('open_instance', {
      args: { name: vaultName, passphrase },
    }),
  )
  await page.navigate(`tauri://localhost/workspace/${vaultName}`)
  await waitForWorkspace(page)
}

/** Creates the vault and points it at the test relay only (the servers apply at the next opening). */
export async function createVaultOnRelay(
  page: Page,
  relayUrl: string,
  vaultName: string,
  passphrase: string,
  irohRelays?: string[],
): Promise<void> {
  unwrap(
    'create_instance',
    await page.invoke('create_instance', {
      args: { name: vaultName, passphrase },
    }),
  )
  // The existing scenarios assert German labels; seed the new vault explicitly so they do not
  // depend on the runner's system locale.
  unwrap(
    'set_pref',
    await page.invoke('set_pref', {
      args: { scope: { kind: 'vault' }, key: 'general.language', value: 'de' },
    }),
  )
  unwrap(
    'sync_servers_set',
    await page.invoke('sync_servers_set', {
      args: await onlyServers(page, relayUrl, irohRelays),
    }),
  )
}

/** Starts an instance, optionally reusing its data root, and opens the vault's workspace. */
/**
 * The steps of linking through commands: the host shows a code, the new installation enters it with
 * the servers of the vault, the host agrees, and the new installation reports it is done. The new
 * installation still has to be reopened over its data to open the vault it was given.
 */
export async function runLink(
  ctx: WaitContext,
  host: Page,
  fresh: Page,
  link: {
    vaultName: string
    deviceName: string
    passphrase: string
    relayUrl: string
    irohRelays?: string[]
    asMainDevice?: boolean
    device?: string
  },
): Promise<void> {
  const { code } = unwrap<{ code: string }>(
    'link_code_create',
    await host.invoke('link_code_create'),
  )
  unwrap(
    'link_join_start',
    await fresh.invoke('link_join_start', {
      args: {
        code,
        vaultName: link.vaultName,
        deviceName: link.deviceName,
        passphrase: link.passphrase,
        servers: await onlyServers(fresh, link.relayUrl, link.irohRelays),
      },
    }),
  )
  await ctx.waitFor(
    `the main device to ask about "${link.deviceName}"`,
    async () => {
      const status = unwrap<{
        linking: { stage: string; newDeviceName?: string } | null
      }>('sync_status', await host.invoke('sync_status'))
      return (
        status.linking?.stage === 'awaiting_confirmation' &&
        status.linking.newDeviceName === link.deviceName
      )
    },
    { timeoutMs: 40_000, fixed: true },
  )
  unwrap(
    'link_confirm',
    await host.invoke('link_confirm', {
      args: { asMainDevice: link.asMainDevice === true },
    }),
  )
  await ctx.waitFor(
    `the link of "${link.deviceName}" to finish`,
    async () => {
      const state = unwrap<{ state: string; reason?: unknown }>(
        'link_join_status',
        await fresh.invoke('link_join_status'),
      )
      if (state.state === 'failed') {
        throw new Error(`the link failed: ${JSON.stringify(state.reason)}`)
      }
      return state.state === 'done'
    },
    { timeoutMs: 40_000, fixed: true },
  )
  ctx.step('linked', link.deviceName, link.device)
}

/**
 * The titles of a device's threads, sorted. Taken from the threads on the page and carried through the
 * driver as one string: whole threads of a large list (10,000 in `sync-lock-during-sync`) are over a
 * megabyte, which the phone's driver lost as "Unable to receive message from renderer".
 */
export async function threadTitles(device: ThreadDevice): Promise<string[]> {
  const titles = await device.instance.exec<string>(
    `return window.__TAURI_INTERNALS__.invoke('list_threads')
       .then((threads) => JSON.stringify(threads.map((thread) => thread.title)))`,
  )
  return (JSON.parse(titles) as string[]).sort()
}

/** How many threads the device holds. Counted on the page, so a large list is not carried through the driver. */
export function threadCount(device: {
  instance: Pick<Page, 'exec'>
}): Promise<number> {
  return device.instance.exec<number>(
    `return window.__TAURI_INTERNALS__.invoke('list_threads').then((threads) => threads.length)`,
  )
}

/** Between two counts of `waitForThreadCount`; see there. */
const COUNT_PAUSE_MS = 1000

/**
 * Waits until the device holds at least `count` threads, counting once a second. Every count carries
 * the whole list through the page, and the wait itself polls every 50 ms: while thousands of threads
 * are still arriving on a phone (a few cores, the main thread already busy taking them in), counting
 * back to back left the renderer without a frame for seconds, and the driver lost it ("Unable to
 * receive message from renderer"). The pause leaves it the time to take the data in.
 */
export async function waitForThreadCount(
  ctx: WaitContext,
  device: { instance: Pick<Page, 'exec'> },
  count: number,
  description: string,
  timeoutMs: number,
): Promise<void> {
  await ctx.waitFor(
    description,
    async () => {
      if ((await threadCount(device)) >= count) return true
      await new Promise((resolve) => setTimeout(resolve, COUNT_PAUSE_MS))
      return false
    },
    { timeoutMs, fixed: true },
  )
}

/** A new thread with the title; returns its id. */
export async function addThread(
  device: ThreadDevice,
  title: string,
): Promise<string> {
  return unwrap<Thread>(
    'create_thread',
    await device.instance.invoke('create_thread', { args: { title } }),
  ).id
}

/**
 * Many new threads at once: the loop runs on the page, so a thousand do not cost a thousand driver round
 * trips. In chunks, so no single script runs long enough for the driver to give up on it.
 */
export async function addThreads(
  device: { instance: Pick<Page, 'exec'> },
  titles: string[],
  chunk = 500,
): Promise<void> {
  for (let from = 0; from < titles.length; from += chunk) {
    const outcome = await device.instance.exec<{
      ok: boolean
      error?: unknown
    }>(
      `const titles = arguments[0]
       return (async () => {
         for (const title of titles) {
           await window.__TAURI_INTERNALS__.invoke('create_thread', { args: { title } })
         }
         return { ok: true }
       })().catch((error) => ({ ok: false, error: String(error) }))`,
      [titles.slice(from, from + chunk)],
    )
    if (!outcome.ok) {
      throw new Error(`create_thread failed: ${JSON.stringify(outcome.error)}`)
    }
  }
}

export async function renameThread(
  device: ThreadDevice,
  id: string,
  title: string,
): Promise<void> {
  unwrap(
    'rename_thread',
    await device.instance.invoke('rename_thread', {
      args: { threadId: id, title },
    }),
  )
}

export async function removeThread(
  device: ThreadDevice,
  id: string,
): Promise<void> {
  unwrap(
    'delete_thread',
    await device.instance.invoke('delete_thread', { args: { threadId: id } }),
  )
}

/** Waits until the device's threads have exactly these titles. */
export async function expectThreads(
  ctx: WaitContext,
  device: ThreadDevice,
  titles: string[],
  description: string,
): Promise<void> {
  const wanted = [...titles].sort()
  await ctx.waitFor(
    description,
    async () => {
      const have = await threadTitles(device)
      return JSON.stringify(have) === JSON.stringify(wanted) ? have : false
    },
    { timeoutMs: 40_000, fixed: true },
  )
}
