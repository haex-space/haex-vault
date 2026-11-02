import { invoke } from '@tauri-apps/api/core'
import { computed, ref } from 'vue'
import type { ChatConversation } from '@bindings/ChatConversation'
import type { ChatConversationKind } from '@bindings/ChatConversationKind'
import type { ContactHeader } from '@bindings/ContactHeader'
import type { ChatPeerMessage } from '@bindings/ChatPeerMessage'

/** A contact or group conversation with the name and photo the list shows (spec 072 US1). */
export interface PeerConversationEntry extends ChatConversation {
  /** Contact name, else the name on the stored card, else the group title. */
  displayName: string
  /** Photo hash of the contact, if any. */
  photo: string | null
}

/** Thin `invoke` wrappers of the conversation commands (spec 072, contracts/ipc.md). */
export function useConversationCommands() {
  return {
    listAsync: (filter?: { kind?: ChatConversationKind; requests?: boolean }) =>
      invoke<ChatConversation[]>('chat_conversations_list', {
        args: filter ?? null,
      }),
    openDirectAsync: (contactId: string) =>
      invoke<{ threadId: string }>('chat_conversation_open_direct', {
        args: { contactId },
      }),
    openNoteAsync: () =>
      invoke<{ threadId: string }>('chat_conversation_open_note'),
    muteAsync: (threadId: string, muted: boolean) =>
      invoke<null>('chat_conversation_mute', { args: { threadId, muted } }),
    deleteAsync: (threadId: string) =>
      invoke<null>('chat_conversation_delete', { args: { threadId } }),
    // Spec 072 US2: messages of contact conversations.
    sendAsync: (threadId: string, text: string, idempotencyKey: string) =>
      invoke<{ messageId: string }>('chat_peer_send', {
        args: { threadId, text, idempotencyKey },
      }),
    resendAsync: (messageId: string) =>
      invoke<null>('chat_peer_resend', { args: { messageId } }),
    messagesAsync: (threadId: string, before?: number) =>
      invoke<ChatPeerMessage[]>('chat_peer_messages_list', {
        args: { threadId, before: before ?? null, limit: null },
      }),
    readAsync: (threadId: string) =>
      invoke<null>('chat_conversation_read', { args: { threadId } }),
    viewingAsync: (viewId: string, threadId: string | null) =>
      invoke<null>('chat_conversation_viewing', { args: { viewId, threadId } }),
  }
}

/**
 * The contact and group conversations of the chat's list (spec 072 FR-001): agent conversations
 * stay with `useThreadSidebar`. Names and photos come from the address book; a vault without a
 * contact shows the name on its card. Refreshes whenever chat or contact rows change, here or on
 * another own device.
 */
export function useConversations(errString: (error: unknown) => string) {
  const commands = useConversationCommands()
  const { listAsync: contactsAsync } = useContacts()
  const { t } = useI18n()

  const rows = ref<ChatConversation[]>([])
  const contacts = ref<ContactHeader[]>([])
  const error = ref<string | null>(null)

  const self = computed(() => contacts.value.find((c) => c.isSelf) ?? null)

  const conversations = computed<PeerConversationEntry[]>(() => {
    const byId = new Map(contacts.value.map((c) => [c.id, c]))
    return rows.value
      .filter((row) => row.kind !== 'agent')
      .map((row) => {
        // The notes read "Ich" with the own contact's picture, as in the address book (FR-002b).
        if (row.kind === 'note')
          return {
            ...row,
            displayName: t('contacts.self'),
            photo: self.value?.photo ?? null,
          }
        const contact = row.peerContactId ? byId.get(row.peerContactId) : null
        return {
          ...row,
          displayName:
            contact?.name ||
            row.peerCardName ||
            row.title ||
            t('chat.peer.unknownName'),
          photo: contact?.photo ?? null,
        }
      })
  })

  /** The "Ich" entry: the notes, or an entry without id before the first note (FR-002b). */
  const notes = computed<PeerConversationEntry>(
    () =>
      conversations.value.find((c) => c.kind === 'note') ?? {
        id: '',
        kind: 'note',
        state: 'active',
        title: '',
        peerVault: null,
        peerContactId: null,
        peerCardName: null,
        peerWithoutRelays: false,
        muted: false,
        unread: 0,
        lastText: null,
        lastAt: 0,
        displayName: t('contacts.self'),
        photo: self.value?.photo ?? null,
      },
  )

  async function refresh() {
    try {
      const [list, people] = await Promise.all([
        commands.listAsync(),
        contactsAsync(),
      ])
      rows.value = list
      contacts.value = people
      error.value = null
    } catch (e: unknown) {
      error.value = errString(e)
    }
  }

  onVaultTablesChanged(
    [
      'chat_threads',
      'chat_peer_messages',
      'chat_members',
      'chat_deliveries',
      'haex_contacts*',
    ],
    refresh,
  )

  return { conversations, notes, contacts, error, refresh, ...commands }
}
