import assert from 'node:assert/strict'
import { randomBytes, randomUUID } from 'node:crypto'
import { scenario } from '../lib/scenario.ts'
import { openChat, unwrap, type FlowInstance } from '../lib/flows.ts'
import { acceptCard } from '../lib/peers.ts'
import { deviceFiles } from '../lib/extension-files.ts'
import { ownCardText } from '../lib/share.ts'

// Spec 072 US3 (quickstart A13, tasks T071, T070, T106): a contact conversation offers files only
// with a Blossom-Server and otherwise shows the hint to the chat settings; a file sent with a message
// arrives on the other vault, below the automatic download limit by itself and above it after a tap on
// "Herunterladen". The native file dialog cannot be driven, so the files are handed to the same
// command the window calls with the dialog's choice.

const SMALL = 5 * 1024 * 1024
/** Above the automatic limit of 25 MB (FR-036). */
const LARGE = 26_000_000

interface Conversation {
  id: string
  kind: string
}

interface Attachment {
  id: string
  name: string | null
  size: number
  inline: boolean
  state: string | null
}

interface Message {
  own: boolean
  attachments: Attachment[]
}

const conversations = async (page: FlowInstance) =>
  unwrap<Conversation[]>(
    'chat_conversations_list',
    await page.invoke('chat_conversations_list'),
  )

const filesOf = async (page: FlowInstance, threadId: string) =>
  unwrap<Message[]>(
    'chat_peer_messages_list',
    await page.invoke('chat_peer_messages_list', { args: { threadId } }),
  )
    .filter((m) => !m.own)
    .flatMap((m) => m.attachments)
    .filter((a) => !a.inline)

/** How many elements of this hook the page shows. */
const count = (page: FlowInstance, hook: string) =>
  page.exec<number>(
    `return document.querySelectorAll('[data-testid="' + arguments[0] + '"]').length`,
    [hook],
  )

scenario('chat-files', { timeoutMs: 420_000 }, async (ctx) => {
  const blossom = await ctx.blossomServer()
  const group = await ctx.group({
    users: { anna: ['laptop'], ben: ['desktop'] },
    irohRelay: true,
  })
  const anna = group.device('anna/laptop').page
  const ben = group.device('ben/desktop').page
  const [annaCard, benCard] = [
    await ownCardText(anna, 'Anna'),
    await ownCardText(ben, 'Ben'),
  ]
  const benOnAnna = await acceptCard(anna, benCard)
  await acceptCard(ben, annaCard)
  const annaThread = unwrap<{ threadId: string }>(
    'chat_conversation_open_direct',
    await anna.invoke('chat_conversation_open_direct', {
      args: { contactId: benOnAnna },
    }),
  ).threadId

  await openChat(anna)
  await anna.click('chat-peer')
  await anna.waitForDisplayed('chat-peer-no-server-hint')
  assert.equal(await count(anna, 'chat-peer-attach'), 0)
  ctx.step(
    'without a Blossom-Server the conversation shows the hint, not the attach button',
  )

  unwrap(
    'set_pref',
    await anna.invoke('set_pref', {
      args: {
        scope: { kind: 'vault' },
        key: 'chat.blossom.server',
        value: blossom.url,
      },
    }),
  )
  unwrap(
    'set_pref',
    await ben.invoke('set_pref', {
      args: {
        scope: { kind: 'vault' },
        key: 'chat.blossom.server',
        value: blossom.url,
      },
    }),
  )
  await anna.waitForDisplayed('chat-peer-attach')
  assert.equal(await count(anna, 'chat-peer-no-server-hint'), 0)
  ctx.step('choosing the server swaps the hint for the attach button')

  const folder = deviceFiles('haex-chat-files-')
  ctx.onTeardown(() => folder.remove())
  const small = folder.path('klein.bin')
  const large = folder.path('gross.bin')
  folder.write('klein.bin', randomBytes(SMALL))
  folder.write('gross.bin', randomBytes(LARGE))
  for (const path of [small, large]) {
    unwrap(
      'chat_peer_send',
      await anna.invoke('chat_peer_send', {
        args: {
          threadId: annaThread,
          text: '',
          idempotencyKey: randomUUID(),
          files: [path],
        },
      }),
    )
  }

  let benThread = ''
  await ctx.waitFor(
    'both files to reach Ben',
    async () => {
      const found = (await conversations(ben)).find((c) => c.kind === 'direct')
      if (found === undefined) return false
      benThread = found.id
      return (await filesOf(ben, benThread)).length === 2
    },
    { timeoutMs: 120_000, fixed: true },
  )
  const stateOf = async (size: number) =>
    (await filesOf(ben, benThread)).find((a) => a.size === size)
  await ctx.waitFor(
    'the 5 MB file to be stored by itself',
    async () => (await stateOf(SMALL))?.state === 'stored',
    { timeoutMs: 120_000, fixed: true },
  )
  await ctx.waitFor(
    'the 26 MB file to wait for a tap',
    async () => (await stateOf(LARGE))?.state === 'pending',
    { timeoutMs: 120_000, fixed: true },
  )
  assert.equal(
    (await stateOf(LARGE))?.state,
    'pending',
    'a file above the automatic limit waits for a tap',
  )
  ctx.step(
    'the small file is stored, the large one waits above the automatic limit',
  )

  await openChat(ben)
  await ben.click('chat-peer')
  await ben.waitForDisplayed('chat-attachment-download')
  assert.equal(await count(ben, 'chat-attachment-bubble'), 2)
  await ben.click('chat-attachment-download')
  await ctx.waitFor(
    'the large file to be stored after the tap',
    async () => (await stateOf(LARGE))?.state === 'stored',
    { timeoutMs: 180_000, fixed: true },
  )
  assert.equal(await count(ben, 'chat-attachment-download'), 0)
  ctx.step('a tap on "Herunterladen" fetches the large file')

  // The first time a decrypted copy goes to another app, the notice about it comes first; the
  // app itself is not started here.
  await ben.waitForDisplayed('chat-attachment-open-system')
  await ben.click('chat-attachment-open-system')
  await ben.waitForDisplayed('files-open-system-notice')
  ctx.step('opening with the system app asks for the notice first')
})
