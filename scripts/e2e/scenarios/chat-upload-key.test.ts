import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { openSettings, waitForLocation } from '../lib/settings.ts'

// Spec 072 US8 scenario 2 (FR-081, task T099, T101): a Blossom upload key is made in the chat
// settings, kept as an entry of the password manager, shown as its public key to register at the
// server, and unset again after a question (the entry stays for pending deletions). Choosing an entry of the user and what uploads sign
// with are covered by `upload_key_tests.rs` and `transfers_tests.rs`.

interface UploadKey {
  status: string
  npub: string | null
  entry: { owned: boolean } | null
}

interface Candidate {
  id: string
  tagged: boolean
  owned: boolean
}

scenario('chat-upload-key', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-chat-upload-key' })
  await openSettings(instance)
  await instance.click('settings-category-chat')
  await waitForLocation(instance, 'chat')
  const status = async () =>
    unwrap<UploadKey>(
      'chat_blossom_key',
      await instance.invoke('chat_blossom_key'),
    )
  const candidates = async () =>
    unwrap<Candidate[]>(
      'chat_blossom_key_candidates',
      await instance.invoke('chat_blossom_key_candidates'),
    )

  await instance.waitForDisplayed('settings-chat-key-create')
  assert.equal((await status()).status, 'none')
  ctx.step('without a key the settings offer to make one or to choose an entry')

  await instance.click('settings-chat-key-create')
  await instance.waitForDisplayed('settings-chat-key-npub')
  const shown = await instance.exec<string>(
    `return document.querySelector('[data-testid="settings-chat-key-npub"]').textContent.trim()`,
  )
  const made = await status()
  assert.equal(made.status, 'present')
  assert.equal(shown, made.npub, 'the page shows the key it made')
  assert.match(shown, /^npub1[0-9a-z]{58}$/)
  const listed = await candidates()
  assert.equal(listed.length, 1, 'the key is an entry of the password manager')
  assert.ok(
    listed[0]?.owned && listed[0]?.tagged,
    'made by the chat and tagged',
  )
  ctx.step(
    'a made key shows its public key and is an entry of the password manager',
  )

  await instance.click('settings-chat-key-remove')
  await instance.waitForDisplayed('settings-chat-key-remove-confirm')
  assert.equal((await status()).status, 'present', 'asking does not remove')
  await instance.click('settings-chat-key-remove-confirm')
  await instance.waitForDisplayed('settings-chat-key-create')
  assert.equal((await status()).status, 'none')
  assert.equal(
    (await candidates()).length,
    1,
    'the entry stays so files uploaded with the key can still be deleted',
  )
  ctx.step('removing asks first and keeps the entry in the password manager')
})
