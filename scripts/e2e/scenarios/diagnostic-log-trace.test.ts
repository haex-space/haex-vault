import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import type { ScenarioContext } from '../lib/scenario.ts'
import { createAndUnlock, openLauncher, unwrap } from '../lib/flows.ts'
import { PROCESS_END_LIMIT_MS } from '../lib/close-promises.ts'
import { vaultFiles } from '../lib/vault-files.ts'

const VAULT = 'e2e-log-trace'
const LOG_TIMEOUT_MS = 15_000

type Instance = Awaited<ReturnType<ScenarioContext['startInstance']>>
type Level = 'standard' | 'verbose'

interface LogStatus {
  level: Level
  untilMs: number | null
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
    () => vaultFiles(instance.root).log(VAULT)?.includes(text) === true,
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
      () => vaultFiles(first.root).log(VAULT) !== null,
      { timeoutMs: LOG_TIMEOUT_MS },
    )
    ctx.step('the log exists after the unlock')

    assert.equal((await status(first)).level, 'standard')
    await consoleLine(first, 'error', 'probe-ui-error')
    await waitForLogLine(ctx, first, 'probe-ui-error')
    const log = vaultFiles(first.root).log(VAULT) ?? ''
    const line = log.split('\n').find((l) => l.includes('probe-ui-error'))
    assert.ok(line?.includes('[ui]'), `the UI line has the prefix: ${line}`)
    ctx.step('a console error of the interface stands in the log with [ui]')

    // At "Standard" a console.debug stays out. console.error after it is the barrier: once that
    // stands in the log, the debug line would have been written before it.
    await consoleLine(first, 'debug', 'probe-debug-standard')
    await consoleLine(first, 'error', 'probe-barrier-standard')
    await waitForLogLine(ctx, first, 'probe-barrier-standard')
    assert.ok(
      !(vaultFiles(first.root).log(VAULT) ?? '').includes(
        'probe-debug-standard',
      ),
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
    // The interface learns the new level from the status event, a moment after the command
    // returned (slower on a phone): a debug line written before that is dropped, so it is repeated.
    await ctx.waitFor(
      '"probe-debug-verbose" in the log',
      async () => {
        await consoleLine(first, 'debug', 'probe-debug-verbose')
        await new Promise((resolve) => setTimeout(resolve, 500))
        return (
          vaultFiles(first.root).log(VAULT)?.includes('probe-debug-verbose') ===
          true
        )
      },
      { timeoutMs: LOG_TIMEOUT_MS },
    )
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
