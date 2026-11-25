import assert from 'node:assert/strict'
import { randomUUID } from 'node:crypto'
import { scenario } from '../lib/scenario.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import { acceptCard } from '../lib/peers.ts'
import { ownCardText } from '../lib/share.ts'

// Spec 099 (quickstart E1, task T025): only contacts may write. Ben and Clara have Anna's card,
// Anna has neither of theirs. Ben's messages leave Anna one request and nothing of their text; Anna
// accepts by reading the carried card in, and Ben's waiting messages arrive. Clara's request Anna
// declines: Clara is blocked, Clara's conversation shows the block, and Clara's next message leaves
// nothing on Anna. Driven through the commands the window uses.

interface Conversation {
  id: string
  kind: string
  state: string
  peerVault: string | null
  blockedByPeer: boolean
  requestLastAt: number | null
}

interface Message {
  own: boolean
  text: string | null
  delivery: string | null
  failedReason: string | null
}

const conversations = async (page: FlowInstance, requests = false) =>
  unwrap<Conversation[]>(
    'chat_conversations_list',
    await page.invoke('chat_conversations_list', {
      args: { requests },
    }),
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

const openWith = async (page: FlowInstance, contactId: string) =>
  unwrap<{ threadId: string }>(
    'chat_conversation_open_direct',
    await page.invoke('chat_conversation_open_direct', {
      args: { contactId },
    }),
  ).threadId

scenario('chat-contact-gate', { timeoutMs: 420_000 }, async (ctx) => {
  const group = await ctx.group({
    users: { anna: ['laptop'], ben: ['desktop'], clara: ['tablet'] },
    irohRelay: true,
  })
  const anna = group.device('anna/laptop').page
  const ben = group.device('ben/desktop').page
  const clara = group.device('clara/tablet').page

  const annaCard = await ownCardText(anna, 'Anna')
  await ownCardText(ben, 'Ben')
  await ownCardText(clara, 'Clara')
  const annaOnBen = await acceptCard(ben, annaCard)
  const annaOnClara = await acceptCard(clara, annaCard)
  ctx.step('Ben and Clara have Anna’s card, Anna has neither of theirs')

  const benThread = await openWith(ben, annaOnBen)
  await send(ben, benThread, 'Hallo Anna')
  await send(ben, benThread, 'Wir kennen uns vom Markt')
  let request: Conversation | undefined
  await ctx.waitFor(
    'Anna to get Ben’s request',
    async () => {
      request = (await conversations(anna, true))[0]
      return request?.requestLastAt != null
    },
    { timeoutMs: 60_000, fixed: true },
  )
  assert.equal(request?.state, 'request')
  assert.deepEqual(
    await messagesOf(anna, request!.id),
    [],
    'nothing of Ben’s messages on Anna',
  )
  ctx.step('a stranger leaves one request and nothing of its messages')

  const { cardText } = unwrap<{ cardText: string }>(
    'chat_request_card',
    await anna.invoke('chat_request_card', {
      args: { threadId: request!.id },
    }),
  )
  await acceptCard(anna, cardText)
  assert.deepEqual(await conversations(anna, true), [], 'the request ended')
  await ctx.waitFor(
    'Ben’s waiting messages to reach Anna',
    async () => {
      const texts = (await messagesOf(anna, request!.id)).map((m) => m.text)
      return texts.length === 2
    },
    { timeoutMs: 90_000, fixed: true },
  )
  assert.deepEqual(
    (await messagesOf(anna, request!.id)).map((m) => m.text),
    ['Hallo Anna', 'Wir kennen uns vom Markt'],
  )
  await ctx.waitFor(
    'Ben to see delivered',
    async () =>
      (await messagesOf(ben, benThread)).every(
        (m) => m.delivery === 'delivered' || m.delivery === 'read',
      ),
    { timeoutMs: 60_000, fixed: true },
  )
  ctx.step('accepting lets the waiting messages in')

  const claraThread = await openWith(clara, annaOnClara)
  await send(clara, claraThread, 'Kaufen Sie jetzt!')
  let claraRequest: Conversation | undefined
  await ctx.waitFor(
    'Anna to get Clara’s request',
    async () => {
      claraRequest = (await conversations(anna, true))[0]
      return claraRequest !== undefined
    },
    { timeoutMs: 60_000, fixed: true },
  )
  unwrap(
    'chat_request_answer',
    await anna.invoke('chat_request_answer', {
      args: { threadId: claraRequest!.id, answer: 'decline' },
    }),
  )
  assert.deepEqual(await conversations(anna, true), [])
  const blocked = unwrap<{ vaultPubkey: string }[]>(
    'contacts_blocked',
    await anna.invoke('contacts_blocked'),
  )
  assert.ok(
    blocked.some((entry) => entry.vaultPubkey === claraRequest!.peerVault),
    'Clara is blocked',
  )
  await ctx.waitFor(
    'Clara to learn of the block',
    async () =>
      (await conversations(clara)).some(
        (c) => c.id === claraThread && c.blockedByPeer,
      ),
    { timeoutMs: 60_000, fixed: true },
  )
  const claraMessages = await messagesOf(clara, claraThread)
  assert.equal(claraMessages[0]?.failedReason, 'blocked')
  await send(clara, claraThread, 'Noch ein Angebot')
  await new Promise((resolve) => setTimeout(resolve, 5_000))
  assert.deepEqual(await conversations(anna, true), [], 'no new request')
  ctx.step('declining blocks the stranger and tells it')
})
