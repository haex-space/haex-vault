import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openChat, type FlowInstance } from '../lib/flows.ts'

// Spec 072 FR-012 and FR-012a: a note ("Notiz an mich selber") takes up to 1,000,000 characters (it was
// 4,000) and a long text is shown folded to 12 lines with "Weiterlesen". One vault, no network: a note
// never leaves the vault, so this needs no relay and no file server. The file-backed path of a direct
// message (inline versus `message.txt`, the receiver's download) is covered by `long_text_tests.rs`.

const MAX_TEXT_CHARS = 1_000_000

/** About 20,000 characters in 400 lines: more than the old limit and far more than 12 lines. */
const LONG_TEXT = Array.from(
  { length: 400 },
  (_, i) => `Zeile ${String(i + 1).padStart(3, '0')}: ${'x'.repeat(40)}`,
).join('\n')

/** Puts the text into the composer in one step; typing 20,000 keys through the driver is too slow. */
const setDraft = (instance: FlowInstance, text: string) =>
  instance.exec(
    `const input = document.querySelector('[data-testid="chat-peer-input"]')
     input.value = arguments[0]
     input.dispatchEvent(new Event('input', { bubbles: true }))
     return true`,
    [text],
  )

const sendDisabled = (instance: FlowInstance) =>
  instance.exec<boolean>(
    `return document.querySelector('[data-testid="chat-peer-send"]').disabled`,
  )

/** The folded state of the shown text, read from the page. */
const shownText = (instance: FlowInstance) =>
  instance.exec<{ text: string; clamped: boolean } | null>(
    `const p = document.querySelector('[data-testid="chat-peer-text"]')
     if (!p) return null
     return { text: p.textContent.trim(), clamped: p.classList.contains('line-clamp-12') }`,
  )

scenario('chat-long-note', {}, async (ctx) => {
  assert.ok(LONG_TEXT.length > 4_000 && LONG_TEXT.length < 30_000)

  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-long-note' })
  await openChat(instance)

  await instance.click('chat-notes')
  await instance.waitForDisplayed('chat-peer-input')
  ctx.step('the note conversation is open')

  await setDraft(instance, LONG_TEXT)
  assert.equal(
    await sendDisabled(instance),
    false,
    `a note of ${LONG_TEXT.length} characters must not be locked`,
  )
  await instance.click('chat-peer-send')
  await ctx.waitFor(
    'the long note to appear in the conversation',
    async () => (await shownText(instance)) !== null,
  )
  ctx.step(
    'a note over the old 4,000 limit is sent',
    `${LONG_TEXT.length} characters`,
  )

  const folded = await shownText(instance)
  assert.ok(folded?.clamped, 'the long note must be folded to 12 lines')
  await instance.waitForDisplayed('chat-peer-read-more')
  ctx.step('it is folded, with "Weiterlesen"')

  await instance.click('chat-peer-read-more')
  await ctx.waitFor('the note to unfold', async () => {
    const state = await shownText(instance)
    return state !== null && !state.clamped
  })
  const unfolded = await shownText(instance)
  assert.equal(
    unfolded?.text,
    LONG_TEXT.trim(),
    'the unfolded note must be the full text',
  )
  ctx.step('"Weiterlesen" shows the full text')

  await instance.click('chat-peer-read-more')
  await ctx.waitFor('the note to fold again', async () => {
    const state = await shownText(instance)
    return state !== null && state.clamped
  })
  ctx.step('"Weniger anzeigen" folds it again')

  await setDraft(instance, 'a'.repeat(MAX_TEXT_CHARS + 1))
  await instance.waitForDisplayed('chat-peer-counter')
  assert.equal(
    await sendDisabled(instance),
    true,
    'a note over the upper bound must be locked',
  )
  ctx.step('over 1,000,000 characters the counter shows and sending is locked')

  await setDraft(instance, 'a'.repeat(MAX_TEXT_CHARS))
  await ctx.waitFor(
    'sending to unlock at exactly the upper bound',
    async () => !(await sendDisabled(instance)),
  )
  ctx.step('exactly 1,000,000 characters are accepted')
})
