import assert from 'node:assert/strict'
import { randomUUID } from 'node:crypto'
import { scenario } from '../lib/scenario.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import { acceptCard } from '../lib/peers.ts'
import { ownCardText } from '../lib/share.ts'

// Spec 072 (quickstart A13, task T106): two vaults that are contacts of each other talk through the
// scenario's Nostr-Relay and Blossom-Server. A text goes both ways, a text too long for one message
// goes as a file over the Blossom-Server and arrives whole, and the sender sees "read" once the
// other side opened the conversation. The conversation is driven through the commands the window
// uses; a native file dialog cannot be driven, so the file path is the one a long text takes.

/** More than one message carries (FR-012), so it goes as `message.txt` over the Blossom-Server. */
const LONG_TEXT = Array.from(
  { length: 3_000 },
  (_, i) => `Zeile ${String(i + 1).padStart(4, '0')}: ${'x'.repeat(44)}`,
).join('\n')

interface Conversation {
  id: string
  kind: string
  state: string
  unread: number
}

interface Attachment {
  id: string
  inline: boolean
  state: string | null
}

interface Message {
  id: string
  own: boolean
  text: string | null
  delivery: string | null
  attachments: Attachment[]
}

const conversations = async (page: FlowInstance) =>
  unwrap<Conversation[]>(
    'chat_conversations_list',
    await page.invoke('chat_conversations_list'),
  )

const messagesOf = async (page: FlowInstance, threadId: string) =>
  unwrap<Message[]>(
    'chat_peer_messages_list',
    await page.invoke('chat_peer_messages_list', { args: { threadId } }),
  )

const send = async (page: FlowInstance, threadId: string, text: string) =>
  unwrap<{ messageId: string }>(
    'chat_peer_send',
    await page.invoke('chat_peer_send', {
      args: { threadId, text, idempotencyKey: randomUUID() },
    }),
  ).messageId

scenario('chat-two-vaults', { timeoutMs: 420_000 }, async (ctx) => {
  const group = await ctx.group({
    users: { anna: ['laptop'], ben: ['desktop'] },
  })
  const anna = group.device('anna/laptop').page
  const ben = group.device('ben/desktop').page
  const blossom = await ctx.blossomServer()

  const [annaCard, benCard] = [
    await ownCardText(anna, 'Anna'),
    await ownCardText(ben, 'Ben'),
  ]
  const benOnAnna = await acceptCard(anna, benCard)
  await acceptCard(ben, annaCard)
  for (const page of [anna, ben]) {
    unwrap(
      'set_pref',
      await page.invoke('set_pref', {
        args: {
          scope: { kind: 'vault' },
          key: 'chat.blossom.server',
          value: blossom.url,
        },
      }),
    )
  }
  ctx.step('Anna and Ben are contacts and use the same Blossom-Server')

  const annaThread = unwrap<{ threadId: string }>(
    'chat_conversation_open_direct',
    await anna.invoke('chat_conversation_open_direct', {
      args: { contactId: benOnAnna },
    }),
  ).threadId
  await send(anna, annaThread, 'Hallo Ben')
  let benThread = ''
  await ctx.waitFor(
    'Ben to get the conversation with Anna’s message',
    async () => {
      const found = (await conversations(ben)).find((c) => c.kind === 'direct')
      if (found === undefined || found.unread < 1) return false
      benThread = found.id
      return true
    },
    { timeoutMs: 60_000, fixed: true },
  )
  const first = (await messagesOf(ben, benThread)).find((m) => !m.own)
  assert.equal(first?.text, 'Hallo Ben')
  await ctx.waitFor(
    'Anna’s message to be delivered',
    async () =>
      (await messagesOf(anna, annaThread)).some(
        (m) => m.own && m.delivery === 'delivered',
      ),
    { timeoutMs: 60_000, fixed: true },
  )
  ctx.step('Anna’s text reaches Ben and is delivered')

  await send(ben, benThread, 'Hallo Anna')
  await ctx.waitFor(
    'Anna to get Ben’s answer',
    async () =>
      (await messagesOf(anna, annaThread)).some(
        (m) => !m.own && m.text === 'Hallo Anna',
      ),
    { timeoutMs: 60_000, fixed: true },
  )
  ctx.step('Ben’s text reaches Anna')

  await send(anna, annaThread, LONG_TEXT)
  let attachmentId = ''
  await ctx.waitFor(
    'the long text to arrive as a file on Ben’s side',
    async () => {
      const file = (await messagesOf(ben, benThread))
        .flatMap((m) => m.attachments)
        .find((a) => a.inline)
      if (file === undefined || file.state !== 'stored') return false
      attachmentId = file.id
      return true
    },
    { timeoutMs: 90_000, fixed: true },
  )
  const full = unwrap<string>(
    'chat_attachment_text',
    await ben.invoke('chat_attachment_text', { args: { attachmentId } }),
  )
  assert.equal(full, LONG_TEXT, 'Ben must get the whole text')
  ctx.step(
    'a text too long for one message arrives whole over the Blossom-Server',
  )

  unwrap(
    'chat_conversation_read',
    await ben.invoke('chat_conversation_read', {
      args: { threadId: benThread },
    }),
  )
  await ctx.waitFor(
    'Anna to see her messages as read',
    async () => {
      const own = (await messagesOf(anna, annaThread)).filter((m) => m.own)
      return own.length === 2 && own.every((m) => m.delivery === 'read')
    },
    { timeoutMs: 60_000, fixed: true },
  )
  ctx.step('Anna sees both of her messages as read')
})
