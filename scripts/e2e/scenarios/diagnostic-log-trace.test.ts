import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import type { ScenarioContext } from '../lib/scenario.ts'
import { createAndUnlock, openLauncher, unwrap } from '../lib/flows.ts'
import { PROCESS_END_LIMIT_MS } from '../lib/close-promises.ts'
import { createAdb } from '../lib/platform/adb.ts'
import { PACKAGE } from '../lib/platform/android.ts'
import { VAULT_DIR } from '../lib/platform/linux.ts'

const VAULT = 'e2e-log-trace'
const ANDROID_PREFIX = 'android:'
const LOG_TIMEOUT_MS = 15_000

type Instance = Awaited<ReturnType<ScenarioContext['startInstance']>>
type Level = 'standard' | 'verbose'

interface LogStatus {
  level: Level
  untilMs: number | null
}

/** The vault's `logs/haex.log` as text, or null while it does not exist (spec 098). */
function readLog(root: string): string | null {
  if (root.startsWith(ANDROID_PREFIX)) {
    const adb = createAdb(root.slice(ANDROID_PREFIX.length))
    const path = `vaults/${VAULT}/logs/haex.log`
    const found = adb.runAs(PACKAGE, `ls ${path} 2>&1`).toString()
    if (found.includes('No such file')) return null
    return adb.runAs(PACKAGE, `cat ${path}`).toString()
  }
  const file = join(root, ...VAULT_DIR, VAULT, 'logs', 'haex.log')
  return existsSync(file) ? readFileSync(file, 'utf8') : null
}

async function status(instance: Instance): Promise<LogStatus> {
  return unwrap<LogStatus>(
    'diagnostic_log_get',
    await instance.invoke('diagnostic_log_get'),
  )
}

/** Lets the interface write one console line; the plugin forwards it to the log. */
async function consoleLine(
  instance: Instance,
  method: 'error' | 'debug',
  text: string,
): Promise<void> {
  await instance.exec(`console.${method}(arguments[0]); return true`, [text])
}

async function waitForLogLine(
  ctx: ScenarioContext,
  instance: Instance,
  text: string,
): Promise<void> {
  await ctx.waitFor(
    `"${text}" in the log`,
    () => readLog(instance.root)?.includes(text) === true,
    { timeoutMs: LOG_TIMEOUT_MS },
  )
}

// Spec 098 quickstart steps 1, 4 and 5 against a running app, on the desktop and on Android: the log
// of a vault starts at the unlock, the interface writes into the same file with the prefix [ui],
// detail lines (console.debug) show only in "Ausführlich", and "Ausführlich" is "Standard" again after
// a restart (FR-003).
scenario(
  'diagnostic-log-trace',
  { needs: { closeBehavior: 'exit' } },
  async (ctx) => {
    const { passphrase } = ctx.credentials()
    const first = await ctx.startInstance()
    await createAndUnlock(first, { name: VAULT, passphrase })
    await ctx.waitFor(
      'the log of the vault to exist after the unlock',
      () => readLog(first.root) !== null,
      { timeoutMs: LOG_TIMEOUT_MS },
    )
    ctx.step('the log exists after the unlock')

    assert.equal((await status(first)).level, 'standard')
    await consoleLine(first, 'error', 'probe-ui-error')
    await waitForLogLine(ctx, first, 'probe-ui-error')
    const log = readLog(first.root) ?? ''
    const line = log.split('\n').find((l) => l.includes('probe-ui-error'))
    assert.ok(line?.includes('[ui]'), `the UI line has the prefix: ${line}`)
    ctx.step('a console error of the interface stands in the log with [ui]')

    // At "Standard" a console.debug stays out. console.error after it is the barrier: once that
    // stands in the log, the debug line would have been written before it.
    await consoleLine(first, 'debug', 'probe-debug-standard')
    await consoleLine(first, 'error', 'probe-barrier-standard')
    await waitForLogLine(ctx, first, 'probe-barrier-standard')
    assert.ok(
      !(readLog(first.root) ?? '').includes('probe-debug-standard'),
      'console.debug reached the log at "Standard"',
    )
    ctx.step('console.debug stays out at "Standard"')

    const verbose = unwrap<LogStatus>(
      'diagnostic_log_set',
      await first.invoke('diagnostic_log_set', {
        level: 'verbose' satisfies Level,
      }),
    )
    assert.equal(verbose.level, 'verbose')
    assert.ok(verbose.untilMs !== null, 'verbose has an end')
    await consoleLine(first, 'debug', 'probe-debug-verbose')
    await waitForLogLine(ctx, first, 'probe-debug-verbose')
    ctx.step('console.debug reaches the log at "Ausführlich"')

    // The end by restart: close the vault (the app ends), start again, open the same vault.
    await openLauncher(first)
    await first.press('lock-instance')
    await first.waitForEnd(PROCESS_END_LIMIT_MS)
    await first.stop()
    const next = await ctx.startInstance({ reusesRoot: first.root })
    unwrap(
      'open_instance',
      await next.invoke('open_instance', { args: { name: VAULT, passphrase } }),
    )
    const after = await status(next)
    assert.equal(after.level, 'standard', 'verbose survived the restart')
    assert.equal(after.untilMs, null)
    ctx.step('"Ausführlich" is "Standard" again after the restart')
  },
)
