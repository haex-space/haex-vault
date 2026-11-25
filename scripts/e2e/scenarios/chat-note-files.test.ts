import assert from 'node:assert/strict'
import { randomBytes, randomUUID } from 'node:crypto'
import { scenario } from '../lib/scenario.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import { deviceFiles } from '../lib/extension-files.ts'

// Spec 072 FR-002b (tasks T145): a note to oneself is sent like a message. The laptop writes a note
// with a file while the phone, the second device of the same vault, is closed; the phone then opens
// and shows the note and holds the file, which it fetched from the Blossom-Server. The native file
// dialog cannot be driven, so the file is handed to the command the window calls with its choice.

const SIZE = 3 * 1024 * 1024
const TEXT = 'Reifen wechseln'

interface Attachment {
  id: string
  size: number
  state: string | null
}

interface Message {
  own: boolean
  text: string | null
  delivery: string | null
  attachments: Attachment[]
}

const messagesOf = async (page: FlowInstance, threadId: string) =>
  unwrap<Message[]>(
    'chat_peer_messages_list',
    await page.invoke('chat_peer_messages_list', { args: { threadId } }),
  )

const openNote = async (page: FlowInstance) =>
  unwrap<{ threadId: string }>(
    'chat_conversation_open_note',
    await page.invoke('chat_conversation_open_note'),
  ).threadId

scenario('chat-note-files', { timeoutMs: 420_000 }, async (ctx) => {
  const blossom = await ctx.blossomServer()
  const group = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
  const laptop = group.device('anna/laptop')
  const phone = group.device('anna/phone')

  unwrap(
    'set_pref',
    await laptop.page.invoke('set_pref', {
      args: {
        scope: { kind: 'vault' },
        key: 'chat.blossom.server',
        value: blossom.url,
      },
    }),
  )
  await phone.stop()
  ctx.step('the phone is closed')

  const folder = deviceFiles('haex-chat-note-files-')
  ctx.onTeardown(() => folder.remove())
  folder.write('reifen.bin', randomBytes(SIZE))
  const noteThread = await openNote(laptop.page)
  unwrap(
    'chat_peer_send',
    await laptop.page.invoke('chat_peer_send', {
      args: {
        threadId: noteThread,
        text: TEXT,
        idempotencyKey: randomUUID(),
        files: [folder.path('reifen.bin')],
      },
    }),
  )
  const own = await messagesOf(laptop.page, noteThread)
  assert.equal(own.length, 1)
  assert.equal(own[0]?.delivery, null, 'a note shows no delivery state')
  ctx.step('the laptop sent a note with a file')

  await phone.start()
  const phoneThread = await openNote(phone.page)
  assert.equal(phoneThread, noteThread, 'both devices derive the same notes')
  await ctx.waitFor(
    'the note and its file to reach the phone',
    async () => {
      const found = await messagesOf(phone.page, phoneThread)
      const note = found.find((m) => m.text === TEXT)
      return note?.attachments.some((a) => a.state === 'stored') === true
    },
    { timeoutMs: 180_000, fixed: true },
  )
  const arrived = (await messagesOf(phone.page, phoneThread)).filter(
    (m) => m.text === TEXT,
  )
  assert.equal(arrived.length, 1, 'both ways land on the same row')
  assert.equal(arrived[0]?.attachments[0]?.size, SIZE)
  assert.equal(arrived[0]?.delivery, null)
  ctx.step('the phone shows the note and holds the file')
})
