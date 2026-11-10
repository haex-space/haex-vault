import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import {
  createAndUnlock,
  openChat,
  connectProvider,
  startReply,
  openLauncher,
} from '../lib/flows.ts'
import {
  PROCESS_END_LIMIT_MS,
  PROVIDER_CLOSE_LIMIT_MS,
} from '../lib/close-promises.ts'
import { ERROR_TOAST } from '../lib/toasts.ts'

// An error shows as a toast; an inline alert still counts in case one is left somewhere.
const ALERT_SCRIPT = `return document.querySelector('[role="alert"], ${ERROR_TOAST}') !== null`

// Spec 013: pressing lock while a reply streams ends the process and cancels the reply, both within
// fixed deadlines (close-promises.ts), and shows no error while doing it.
scenario('lock-while-streaming', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  const provider = await ctx.provider({ kind: 'stream-forever' })

  await createAndUnlock(instance, { name: 'e2e-test' })
  await openChat(instance)
  await connectProvider(instance, provider)
  await startReply(instance, provider, 'hello')

  const connection = await provider.waitForOpen()
  await ctx.waitFor(
    "the provider's connection to have been open for 800 ms",
    () => Date.now() - connection.openedAt >= 800,
    { fixed: true, timeoutMs: 2000 },
  )

  const alertBefore = await instance.exec<boolean>(ALERT_SCRIPT)
  assert.equal(
    alertBefore,
    false,
    'an alert was already showing just before the press',
  )

  await openLauncher(instance)
  const pressedAt = await instance.press('lock-instance')

  const samples = await instance.sampleUntilEnd<boolean>(ALERT_SCRIPT, 50)
  assert.ok(
    samples.every((sample) => sample === false),
    `an alert appeared while the process was ending (samples: ${JSON.stringify(samples)})`,
  )

  await instance.waitForEnd(PROCESS_END_LIMIT_MS)

  // A process that ends within milliseconds of the press leaves the socket's own "close" event no real
  // time to reach this process's event loop yet; wait for it rather than reading closedAt once, right
  // after (research: the promise is 1 second from the press, not from whenever this line happens to run).
  await ctx.waitFor(
    "the provider's connection to close",
    () =>
      provider.connections().find((c) => c.id === connection.id)?.closedAt !==
      undefined,
    { fixed: true, timeoutMs: PROVIDER_CLOSE_LIMIT_MS },
  )
  const closedAt = provider
    .connections()
    .find((c) => c.id === connection.id)!.closedAt!
  const closeDelayMs = closedAt - pressedAt
  assert.ok(
    closeDelayMs <= PROVIDER_CLOSE_LIMIT_MS,
    `provider connection closed ${closeDelayMs} ms after the press, over the ${PROVIDER_CLOSE_LIMIT_MS} ms limit`,
  )
  ctx.step('provider-closed', `${closeDelayMs} ms`)
})
