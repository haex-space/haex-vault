import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { translations } from '../lib/translations.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { runAction, waitForLocation } from '../lib/settings.ts'

// Spec 043 FR-028 (quickstart §8 step 1, T079), on the desktop and on the phone: before a model
// download the question names the size and the free space; nothing is asked of the model source
// before the person confirms, and a model too large for the free space gets a warning and a button
// that says so. The models are stand-ins beside the catalog, and a local server stands in for
// HuggingFace (debug builds only, `models/stand_in.rs`): the fake provider records every request.

const MB = 1024 * 1024

/** A catalog entry as the catalog file writes it. */
function standIn(id: string, sizeBytes: number) {
  return {
    id,
    name: `Stand-in ${id}`,
    family: 'stand-in',
    parameters: '1M',
    quantization: 'Q4_K_M',
    hf_repo: `e2e/${id}`,
    hf_filename: `${id}.gguf`,
    tokenizer_repo: `e2e/${id}`,
    approx_size_bytes: sizeBytes,
    context_window: 4096,
    license: 'none',
  }
}

/** The text of "Trotzdem laden" in every language holzi speaks. */
const START_ANYWAY = translations('models.downloadConfirm.startAnyway')

scenario('models-download-confirm', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'models-vault' })
  const huggingFace = await ctx.provider()
  const small = standIn('e2e-small', 50 * MB)
  // A petabyte: more than any test device has free.
  const huge = standIn('e2e-huge', 1e15)
  unwrap(
    'e2e_stand_in_models',
    await instance.invoke('e2e_stand_in_models', {
      baseUrl: huggingFace.baseUrl,
      entries: [small, huge],
    }),
  )
  const opened = await runAction(instance, 'wm.app.open', {
    appId: 'system.settings',
    at: '/ai/download',
  })
  assert.ok(opened.ok, JSON.stringify(opened))
  await waitForLocation(instance, 'ai.download')
  await instance.waitForDisplayed(`models-download-${small.id}`)

  const text = (hook: string) =>
    instance.exec<string>(
      `return document.querySelector('[data-testid="' + arguments[0] + '"]')?.textContent?.trim() ?? ''`,
      [hook],
    )
  const shown = (hook: string) =>
    instance.exec<boolean>(
      `return document.querySelector('[data-testid="' + arguments[0] + '"]') !== null`,
      [hook],
    )
  /** Opens the question for `id` and waits until it knows the free space. */
  async function ask(id: string): Promise<void> {
    await instance.click(`models-download-${id}`)
    await instance.waitForDisplayed('models-download-confirm')
    await ctx.waitFor('the free space', async () =>
      /\d/.test(await text('models-download-free')),
    )
  }
  async function cancel(): Promise<void> {
    await instance.click('models-download-cancel')
    await ctx.waitFor(
      'the question closed',
      async () => !(await shown('models-download-confirm')),
    )
  }

  await ask(small.id)
  assert.match(await text('models-download-size'), /\b50 MB\b/)
  assert.equal(await shown('models-download-short'), false, 'a warning')
  assert.ok(
    !START_ANYWAY.includes(await text('models-download-start')),
    'the button says "load anyway" for a model that fits',
  )
  await cancel()
  assert.deepEqual(huggingFace.requests(), [], 'a request before confirming')
  ctx.step(
    'the size and the free space before the download; cancel loads nothing',
  )

  await ask(huge.id)
  await instance.waitForDisplayed('models-download-short')
  assert.ok(
    START_ANYWAY.includes(await text('models-download-start')),
    'the button does not say "load anyway"',
  )
  await cancel()
  assert.deepEqual(huggingFace.requests(), [], 'a request before confirming')
  ctx.step('a model too large for the free space gets a warning')

  await ask(small.id)
  await instance.click('models-download-start')
  await ctx.waitFor('the download asking the model source', async () =>
    huggingFace
      .requests()
      .some((request) => request.path.includes(small.hf_repo)),
  )
  ctx.step('the download starts after confirming')
})
