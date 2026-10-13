// Files of a storage through the app's own commands and the file browser's transfers (spec 044,
// 048 scenarios): listing a folder, and copying device files in as a drop from the system does,
// without the clicks of a drop the rig cannot make.
import { unwrap, type FlowInstance } from './flows.ts'
import type { ScenarioContext } from './scenario.ts'
import { runAction } from './settings.ts'

const TRANSFER_TIMEOUT_MS = 60_000

export type Page = Pick<FlowInstance, 'invoke' | 'exec'>
export type Source = { kind: 'storage'; storageId: string }
export type Entry = {
  name: string
  path: string
  size: number | null
  encrypted: { role: string; state: string } | null
}

export const list = async (page: Page, source: Source, path: string) =>
  unwrap<Entry[]>(
    'files_list',
    await page.invoke('files_list', { source, path }),
  )

export const names = async (page: Page, source: Source, path: string) =>
  (await list(page, source, path)).map((entry) => entry.name).sort()

const TRANSFERS = `document.querySelector('#__nuxt').__vue_app__.config.globalProperties.$pinia._s.get('filesTransfers')`

/** Copies device files into `path` of `source` as a drop from the system does; does not wait. The
 * file browser must be open (its store holds the transfers, `openFiles`). */
export async function startImport(
  page: Page,
  files: string[],
  source: Source,
  path: string,
) {
  await page.exec(
    `return ${TRANSFERS}.importAsync(arguments[0], arguments[1], 'e2e').then(() => true)`,
    [files, { source, path }],
  )
}

/** Waits until no transfer of the file browser runs; fails on one that failed. */
export async function transfersDone(
  ctx: ScenarioContext,
  page: Page,
  what: string,
) {
  await ctx.waitFor(
    what,
    async () => {
      const state = await page.exec<{ running: number; failed: string[] }>(
        `const transfers = ${TRANSFERS}.transfers
         return {
           running: transfers.length,
           failed: transfers.filter((t) => t.state === 'failed').map((t) => JSON.stringify(t.error)),
         }`,
      )
      if (state.failed.length > 0) {
        throw new Error(`a transfer failed: ${state.failed.join(', ')}`)
      }
      return state.running === 0
    },
    { timeoutMs: TRANSFER_TIMEOUT_MS },
  )
}

/** Opens the file browser on the storage's top folder; the transfers live in its store. */
export async function openFiles(page: FlowInstance, storageId: string) {
  await runAction(page, 'wm.app.open', {
    appId: 'system.files',
    at: `/storage/${storageId}?p=${encodeURIComponent('/')}`,
  })
}
