import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { expectOnline } from '../lib/group-expect.ts'
import { openLauncher } from '../lib/flows.ts'
import {
  addThreads,
  threadCount,
  threadTitles,
  waitForThreadCount,
} from '../lib/sync-flows.ts'

/**
 * How many chats the first device has made while the second one is away. The second one takes them in
 * chunks, so it holds some but not all for a while: about 2.4 s at 10,000 (research.md, G3).
 */
const COUNT = Number(process.env.E2E_LOCK_SYNC_COUNT ?? 10_000)
const title = (n: number) => `Chat ${String(n).padStart(5, '0')}`

// Spec 024, user story 5 (M8 of its quickstart, FR-040): locking the vault while a large sync runs ends
// the application and its connections at once; the sync does not start over but goes on at the next
// opening and ends with every chat exactly once. The device that locks is the one receiving: it is the
// one with something half done (the sender's data is already on its way, research.md G3).
scenario('sync-lock-during-sync', { timeoutMs: 600_000 }, async (ctx) => {
  const g = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
  const a = g.device('anna/laptop')
  const b = g.device('anna/phone')
  const wanted = Array.from({ length: COUNT }, (_, n) => title(n))

  await b.pubkey() // read now: the device is not running when the other one looks it up
  await b.stop()
  await addThreads(a, wanted)
  ctx.step('chats made while the other device was away', String(COUNT))

  await b.start()
  await openLauncher(b.page)
  const startedAt = Date.now()
  let held = 0
  while (held === 0) {
    held = await threadCount(b)
    if (held >= COUNT) {
      assert.fail(
        `the sync finished before it could be caught half way (${COUNT} chats are too few)`,
      )
    }
    if (Date.now() - startedAt > 120_000) assert.fail('nothing synced')
  }
  const lockedIn = await b.lock()
  ctx.step(
    'locked during the sync',
    `${held} of ${COUNT} held, first seen ${Date.now() - startedAt - lockedIn} ms after the start, closed in ${lockedIn} ms`,
  )

  await expectOnline(ctx, a, b, false, 60_000)
  ctx.step('the other device sees the locked one gone')

  await b.start()
  await waitForThreadCount(
    ctx,
    b,
    COUNT,
    'the reopened device to hold every chat',
    180_000,
  )
  assert.deepEqual(await threadTitles(b), wanted, 'none missing, none twice')
  assert.equal(await threadCount(a), COUNT)
  ctx.step('every chat arrived exactly once')
})
