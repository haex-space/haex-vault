import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openLauncher, unwrap } from '../lib/flows.ts'
import { PROCESS_END_LIMIT_MS } from '../lib/close-promises.ts'

// Spec 043 FR-006 on the phone: closing the vault ends the app (spec 013 keeps one vault session per
// process); the next start shows the vault picker with the vault in it, not the workspace.
scenario('android-close-ends-app', { needs: { phone: true } }, async (ctx) => {
  const first = await ctx.startInstance()
  await createAndUnlock(first, { name: 'phone-vault' })
  await openLauncher(first)
  await first.press('lock-instance')
  await first.waitForEnd(PROCESS_END_LIMIT_MS)
  assert.equal(first.alive(), false)
  // Ending the process with `exit` aborted in the system's UI renderer (libhwui): "FORTIFY:
  // pthread_mutex_lock called on a destroyed mutex".
  assert.deepEqual(
    first.phone?.crashLines(first.pid),
    [],
    'the app ended without an abort',
  )
  await first.stop()
  ctx.step('closing the vault ended the app')

  const next = await ctx.startInstance({ reusesRoot: first.root })
  await next.waitForDisplayed('landing-import')
  assert.equal(await next.exec<string>('return location.pathname'), '/')
  const names = unwrap<{ name: string }[]>(
    'list_instances',
    await next.invoke('list_instances'),
  ).map((instance) => instance.name)
  assert.deepEqual(names, ['phone-vault'])
  ctx.step('the next start shows the vault picker')
})
