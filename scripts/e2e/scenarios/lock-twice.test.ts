import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openChat, openLauncher } from '../lib/flows.ts'
import { PROCESS_END_LIMIT_MS } from '../lib/close-promises.ts'
import { ERROR_TOAST } from '../lib/toasts.ts'

// An error shows as a toast; an inline alert still counts in case one is left somewhere.
const ALERT_SCRIPT = `return document.querySelector('[role="alert"], ${ERROR_TOAST}') !== null`
const AFTER_END_WATCH_MS = 5_000

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

// Spec 013: pressing lock twice in a row (an impatient double press, with nothing else running) still
// ends the process exactly once, with no error shown, and never leaves more than one marked process
// running at a time - not even briefly, right after the end.
scenario('lock-twice', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-test' })
  await openChat(instance)

  const alertBefore = await instance.exec<boolean>(ALERT_SCRIPT)
  assert.equal(
    alertBefore,
    false,
    'an alert was already showing just before the press',
  )

  await openLauncher(instance)
  await instance.press('lock-instance', { times: 2 })

  const samples = await instance.sampleUntilEnd<boolean>(ALERT_SCRIPT, 50)
  assert.ok(
    samples.every((sample) => sample === false),
    `an alert appeared while the process was ending (samples: ${JSON.stringify(samples)})`,
  )

  await instance.waitForEnd(PROCESS_END_LIMIT_MS)

  const watchEnd = Date.now() + AFTER_END_WATCH_MS
  while (Date.now() < watchEnd) {
    const marked = instance.markedProcesses()
    assert.ok(
      marked.length <= 1,
      `more than one marked process at once: ${JSON.stringify(marked)}`,
    )
    await sleep(100)
  }
  ctx.step('lock-twice-checked')
})
