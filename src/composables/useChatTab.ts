import type { Ref } from 'vue'
import type { Message, Thread, useChat } from '~/composables/useChat'
import type { ChoiceAnswer, PendingPrompt } from '~/lib/chat/prompts'
import { respondChoiceAsync } from '~/composables/useChatChoices'
import { useChatNavigation } from '~/composables/useChatNavigation'
import type { WmTabApi } from '~/composables/useWmTab'
import type { TabRouter } from '~/composables/useTabRouter'
import { agentSafeMessages } from '~/lib/actions/agentTools'
import type { ActionCaller, ActionOutcome } from '~/lib/actions/types'

type PermissionMode = 'manual' | 'auto' | 'plan'
type RunAction = (
  id: string,
  input?: Record<string, unknown>,
  caller?: ActionCaller,
  originTabId?: string,
) => Promise<ActionOutcome>

/**
 * The chat's side of the window manager (spec 020-tab-navigation, T050): tab history
 * (`useChatNavigation`), the tab-bound chat actions (research R19,
 * `lib/actions/chatActions.ts`), the approval response and the close guard
 * (015 FR-014) — kept out of `ChatApp.vue` for the 500-line limit.
 *
 * Handlers reuse the chat's existing functions, so a call by an agent behaves
 * exactly like the click (FR-029); the returned `ui` callbacks are what the
 * template binds, and each of them runs its catalog action (FR-024).
 */
export function useChatTab(deps: {
  wmTab: WmTabApi
  router: TabRouter
  runAction: RunAction
  chat: ReturnType<typeof useChat>
  errString: (error: unknown) => string
  newChatLabel: () => string
  state: {
    activeThreadId: Ref<string | null>
    threads: Ref<Thread[]>
    input: Ref<string>
    pendingPrompts: Ref<PendingPrompt[]>
    lastError: Ref<string | null>
    streamingMessageId: Ref<string | null>
    turnSetupPending: Ref<boolean>
    editingThreadId: Ref<string | null>
    draftTitle: Ref<string>
    editTitleError: Ref<string | null>
    deleteCandidate: Ref<Thread | null>
    deleteError: Ref<string | null>
  }
  selectThread: (id: string) => Promise<void> | void
  saveThreadTitle: () => Promise<void>
  confirmDelete: () => Promise<void>
  send: (retry?: boolean) => Promise<void>
  abort: () => Promise<void>
  newChat: () => Promise<void> | void
  updatePermissionMode: (mode: PermissionMode) => Promise<void> | void
  /** Spec 072: contact and group conversations next to the agent's. */
  peer: {
    activePeerId: Ref<string | null>
    conversations: Ref<{ id: string; displayName: string; muted: boolean }[]>
    openDirectAsync: (contactId: string) => Promise<{ threadId: string }>
    muteAsync: (threadId: string, muted: boolean) => Promise<unknown>
    deleteAsync: (threadId: string) => Promise<unknown>
    refresh: () => Promise<void>
  }
}) {
  const { wmTab, state, runAction } = deps

  const { peer } = deps
  const {
    chatTitle,
    openConversation,
    openPeerConversation,
    startNewConversation,
    syncFromLocation,
  } = useChatNavigation({
    router: deps.router,
    activeThreadId: state.activeThreadId,
    threads: state.threads,
    newChatLabel: deps.newChatLabel,
    setTitle: wmTab.setTitle,
    selectThread: deps.selectThread,
    newChat: deps.newChat,
    activePeerId: peer.activePeerId,
    peerTitle: (id) =>
      peer.conversations.value.find((c) => c.id === id)?.displayName ?? null,
  })

  async function respondToApproval(
    requestId: string,
    decision: 'allow' | 'deny',
  ) {
    try {
      await deps.chat.respondToolPermissionAsync(requestId, decision)
      state.pendingPrompts.value = state.pendingPrompts.value.filter(
        (a) => a.requestId !== requestId,
      )
      if (state.pendingPrompts.value.length === 0) wmTab.clearAttention()
    } catch (e: unknown) {
      state.lastError.value = deps.errString(e)
    }
  }

  /** Answers the agent's question (spec 046) and takes it off the queue. */
  async function respondToChoice(requestId: string, answer: ChoiceAnswer) {
    try {
      await respondChoiceAsync(requestId, answer)
      state.pendingPrompts.value = state.pendingPrompts.value.filter(
        (p) => p.requestId !== requestId,
      )
      if (state.pendingPrompts.value.length === 0) wmTab.clearAttention()
    } catch (e: unknown) {
      state.lastError.value = deps.errString(e)
    }
  }

  // FR-014: ask before closing with a reply running or a permission pending; confirmAsync reuses
  // abort (spec 003).
  wmTab.registerCloseGuard(() => {
    const hasActiveReply =
      state.streamingMessageId.value !== null ||
      state.turnSetupPending.value ||
      state.pendingPrompts.value.length > 0
    if (!hasActiveReply) return null
    return {
      reasonKey: 'wm.close.activeReply',
      confirmAsync: async () => {
        await deps.abort()
      },
    }
  })

  const done = { done: true }
  const handle = (
    id: string,
    handler: (input: Record<string, unknown>) => unknown,
  ) => wmTab.registerActionHandler(id, ({ input }) => handler(input))

  handle('chat.conversation.new', () => {
    startNewConversation()
    return done
  })
  handle('chat.conversation.open', (input) => {
    const threadId = String(input.threadId)
    if (!state.threads.value.some((thread) => thread.id === threadId))
      throw new Error(`no conversation ${threadId}`)
    openConversation(threadId)
    return done
  })
  handle('chat.conversation.rename', async (input) => {
    state.editingThreadId.value = String(input.threadId)
    state.draftTitle.value = String(input.title)
    await deps.saveThreadTitle()
    if (state.editTitleError.value) throw new Error(state.editTitleError.value)
    return done
  })
  wmTab.registerActionHandler(
    'chat.conversation.delete',
    async ({ input, caller }) => {
      const threadId = String(input.threadId)
      if (peer.conversations.value.some((c) => c.id === threadId)) {
        // FR-072: agents never change contact conversations; to them it is not there.
        if (caller.kind !== 'user')
          throw new Error(`no conversation ${threadId}`)
        await peer.deleteAsync(threadId)
        await peer.refresh()
        if (peer.activePeerId.value === threadId) startNewConversation()
        return done
      }
      const thread = state.threads.value.find((t) => t.id === threadId)
      if (!thread) throw new Error(`no conversation ${threadId}`)
      state.deleteCandidate.value = thread
      await deps.confirmDelete()
      if (state.deleteError.value) throw new Error(state.deleteError.value)
      return done
    },
  )
  handle('chat.conversations.list', () => ({
    conversations: state.threads.value.map((thread) => ({
      threadId: thread.id,
      title: thread.title,
      createdAt: thread.createdAt,
      updatedAt: thread.updatedAt,
    })),
  }))
  // Agents get who spoke, when and how the turn ended, never the text (spec 032 FR-010).
  wmTab.registerActionHandler(
    'chat.messages.list',
    async ({ input, caller }) => {
      const messages: Message[] = await deps.chat.listMessagesAsync(
        String(input.threadId),
      )
      if (caller.kind !== 'user') return agentSafeMessages(messages)
      return {
        messages: messages.map((m) => ({
          role: m.role,
          content: m.content,
          createdAt: m.createdAt,
        })),
      }
    },
  )
  handle('chat.message.send', async (input) => {
    state.input.value = String(input.text)
    await deps.send()
    return done
  })
  handle('chat.message.retry', async () => {
    await deps.send(true)
    return done
  })
  handle('chat.reply.cancel', async () => {
    await deps.abort()
    return done
  })
  handle('chat.approval.decide', async (input) => {
    await respondToApproval(
      String(input.requestId),
      input.decision === 'allow' ? 'allow' : 'deny',
    )
    return done
  })
  handle('chat.choice.answer', async (input) => {
    const value = String(input.value ?? '')
    await respondToChoice(
      String(input.requestId),
      input.kind === 'option'
        ? { kind: 'option', value }
        : input.kind === 'text'
          ? { kind: 'text', text: value }
          : { kind: 'cancel' },
    )
    return done
  })
  handle('chat.permissionMode.set', async (input) => {
    await deps.updatePermissionMode(input.mode as PermissionMode)
    return done
  })

  // Spec 072: contact and group conversations. Not callable by agents (FR-070: an agent reads them
  // only through its own grant, which comes with spec 072 US7).
  handle('chat.contact.open', (input) => {
    const threadId = String(input.threadId)
    if (!peer.conversations.value.some((c) => c.id === threadId))
      throw new Error(`no conversation ${threadId}`)
    openPeerConversation(threadId)
    return done
  })
  handle('chat.contact.openDirect', async (input) => {
    const { threadId } = await peer.openDirectAsync(String(input.contactId))
    await peer.refresh()
    openPeerConversation(threadId)
    return done
  })
  handle('chat.conversation.mute', async (input) => {
    await peer.muteAsync(String(input.threadId), Boolean(input.muted))
    return done
  })

  const run = (id: string, input: Record<string, unknown> = {}) => {
    void runAction(id, { ...input }, { kind: 'user' }, wmTab.tabId)
  }

  return {
    chatTitle,
    syncFromLocation,
    ui: {
      newConversation: () => run('chat.conversation.new'),
      openConversation: (threadId: string) =>
        run('chat.conversation.open', { threadId }),
      openPeerConversation: (threadId: string) =>
        run('chat.contact.open', { threadId }),
      openDirect: (contactId: string) =>
        run('chat.contact.openDirect', { contactId }),
      mute: (threadId: string, muted: boolean) =>
        run('chat.conversation.mute', { threadId, muted }),
      deletePeer: (threadId: string) =>
        run('chat.conversation.delete', { threadId }),
      saveTitle: () =>
        run('chat.conversation.rename', {
          threadId: state.editingThreadId.value ?? '',
          title: state.draftTitle.value,
        }),
      confirmDelete: () =>
        run('chat.conversation.delete', {
          threadId: state.deleteCandidate.value?.id ?? '',
        }),
      send: () => run('chat.message.send', { text: state.input.value }),
      retrySend: () => run('chat.message.retry'),
      abort: () => run('chat.reply.cancel'),
      respondApproval: (requestId: string, decision: 'allow' | 'deny') =>
        run('chat.approval.decide', { requestId, decision }),
      answerChoice: (requestId: string, answer: ChoiceAnswer) =>
        run('chat.choice.answer', {
          requestId,
          kind: answer.kind,
          value:
            answer.kind === 'option'
              ? answer.value
              : answer.kind === 'text'
                ? answer.text
                : '',
        }),
      setPermissionMode: (mode: PermissionMode) =>
        run('chat.permissionMode.set', { mode }),
    },
  }
}
