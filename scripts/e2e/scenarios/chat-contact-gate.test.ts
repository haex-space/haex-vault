import assert from 'node:assert/strict'
import { randomUUID } from 'node:crypto'
import { scenario } from '../lib/scenario.ts'
import { openChat, unwrap, type FlowInstance } from '../lib/flows.ts'
import { acceptCard } from '../lib/peers.ts'
import { ownCardText } from '../lib/share.ts'
import { openSettings, waitForLocation } from '../lib/settings.ts'

// Spec 099 (quickstart E1, task T025): only contacts may write. Ben and Clara have Anna's card,
// Anna has neither of theirs. Ben's messages leave Anna one request and nothing of their text; Anna
// accepts by reading the carried card in, and Ben's waiting messages arrive. Clara's request Anna
// declines: Clara is blocked, Clara's conversation shows the block, and Clara's next message leaves
// nothing on Anna. Then Dave's request shows in the sidebar until Anna switches requests of
// strangers off (quickstart E2, task T031): the request and the requests area go, and Dave's next
// message leaves nothing. Driven through the commands the window uses.

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

/** How many elements of this hook the page shows. */
const count = (page: FlowInstance, hook: string) =>
  page.exec<number>(
    `return document.querySelectorAll('[data-testid="' + arguments[0] + '"]').length`,
    [hook],
  )

const openWith = async (page: FlowInstance, contactId: string) =>
  unwrap<{ threadId: string }>(
    'chat_conversation_open_direct',
    await page.invoke('chat_conversation_open_direct', {
      args: { contactId },
    }),
  ).threadId

scenario('chat-contact-gate', { timeoutMs: 420_000 }, async (ctx) => {
  const group = await ctx.group({
    users: {
      anna: ['laptop'],
      ben: ['desktop'],
      clara: ['tablet'],
      dave: ['pc'],
    },
    irohRelay: true,
  })
  const anna = group.device('anna/laptop').page
  const ben = group.device('ben/desktop').page
  const clara = group.device('clara/tablet').page
  const dave = group.device('dave/pc').page

  const annaCard = await ownCardText(anna, 'Anna')
  await ownCardText(ben, 'Ben')
  await ownCardText(clara, 'Clara')
  const annaOnBen = await acceptCard(ben, annaCard)
  const annaOnClara = await acceptCard(clara, annaCard)
  await ownCardText(dave, 'Dave')
  const annaOnDave = await acceptCard(dave, annaCard)
  ctx.step('Ben, Clara and Dave have Anna’s card, Anna has none of theirs')

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
  // SC-002 and SC-003 (task T045) are timed from the answer to the other side's view and printed
  // into the job log, to compare with 072 SC-002 (5 s).
  const acceptedAt = Date.now()
  assert.deepEqual(await conversations(anna, true), [], 'the request ended')
  await ctx.waitFor(
    'Ben’s waiting messages to reach Anna',
    async () => {
      const texts = (await messagesOf(anna, request!.id)).map((m) => m.text)
      return texts.length === 2
    },
    { timeoutMs: 90_000, fixed: true },
  )
  console.log(
    `SC-002: accept to Ben's messages on Anna ${Date.now() - acceptedAt} ms`,
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
  const declinedAt = Date.now()
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
  console.log(
    `SC-003: decline to the block on Clara ${Date.now() - declinedAt} ms`,
  )
  const claraMessages = await messagesOf(clara, claraThread)
  assert.equal(claraMessages[0]?.failedReason, 'blocked')
  await send(clara, claraThread, 'Noch ein Angebot')
  await new Promise((resolve) => setTimeout(resolve, 5_000))
  assert.deepEqual(await conversations(anna, true), [], 'no new request')
  ctx.step('declining blocks the stranger and tells it')

  const daveThread = await openWith(dave, annaOnDave)
  await send(dave, daveThread, 'Hallo, ich bin Dave')
  await ctx.waitFor(
    'Anna to get Dave’s request',
    async () => (await conversations(anna, true)).length === 1,
    { timeoutMs: 60_000, fixed: true },
  )
  await openChat(anna)
  await anna.waitForDisplayed('chat-requests')
  await openSettings(anna)
  await anna.click('settings-category-chat')
  await waitForLocation(anna, 'chat')
  await anna.waitForDisplayed(
    '[data-testid="settings-chat-allow-strangers"][data-state="checked"]',
  )
  await anna.click('settings-chat-allow-strangers')
  await anna.waitForDisplayed(
    '[data-testid="settings-chat-allow-strangers"][data-state="unchecked"]',
  )
  await openChat(anna)
  await ctx.waitFor(
    'Dave’s request to go',
    async () =>
      (await conversations(anna, true)).length === 0 &&
      (await count(anna, 'chat-requests')) === 0,
    { timeoutMs: 30_000, fixed: true },
  )
  ctx.step('switching requests off removes the request and the requests area')

  await send(dave, daveThread, 'Bist du da?')
  await ctx.waitFor(
    'Dave’s second message taken by the Nostr-Relay',
    async () =>
      (await messagesOf(dave, daveThread)).every((m) => m.delivery === 'sent'),
    { timeoutMs: 60_000, fixed: true },
  )
  await new Promise((resolve) => setTimeout(resolve, 5_000))
  assert.deepEqual(await conversations(anna, true), [], 'no request')
  assert.ok(
    !(await conversations(anna)).some((c) => c.id === daveThread),
    'no conversation with Dave',
  )
  assert.equal(await count(anna, 'chat-requests'), 0)
  ctx.step('with requests off a stranger’s message leaves nothing')
})
