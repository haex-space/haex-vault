import type { Ref } from 'vue'

/**
 * A turn that keeps running in a conversation the person left (spec 068 research R1: several
 * conversations run turns side by side).
 *
 * The page's streaming refs (`streamingMessageId`, `streamingThreadId`, `streamingBuffer`,
 * `retryingMessageId`, `busy`) always describe the turn of the conversation that is open. When
 * another conversation is opened, the turn of the one left is parked here with what those refs held;
 * opening it again brings it back. Events of a parked turn are applied to its own conversation
 * (`useChatTranscript`), so nothing of it is lost while it is out of sight.
 */
export interface ParkedStream {
  threadId: string
  /** The assistant message the turn streams into (also the turn's id). */
  messageId: string
  buffer: string
  retryingMessageId: string | null
}

export interface StreamRefs {
  streamingMessageId: Ref<string | null>
  streamingThreadId: Ref<string | null>
  streamingBuffer: Ref<string>
  retryingMessageId: Ref<string | null>
  busy: Ref<boolean>
  turnSetupPending: Ref<boolean>
}

export function useChatStreams(refs: StreamRefs) {
  const parked = new Map<string, ParkedStream>()

  /**
   * Parks the turn of the open conversation unless `nextThreadId` is that same conversation. A turn
   * whose send is still being set up has no message to stream into yet and stays as it is.
   */
  function parkActive(nextThreadId: string | null) {
    const threadId = refs.streamingThreadId.value
    const messageId = refs.streamingMessageId.value
    if (!threadId || !messageId || threadId === nextThreadId) return
    parked.set(threadId, {
      threadId,
      messageId,
      buffer: refs.streamingBuffer.value,
      retryingMessageId: refs.retryingMessageId.value,
    })
    refs.streamingMessageId.value = null
    refs.streamingThreadId.value = null
    refs.streamingBuffer.value = ''
    refs.retryingMessageId.value = null
    refs.busy.value = false
  }

  /** Brings the parked turn of `threadId` back as the open conversation's, if it has one. */
  function restore(threadId: string | null) {
    if (!threadId) return
    const stream = parked.get(threadId)
    if (!stream) return
    parked.delete(threadId)
    refs.streamingMessageId.value = stream.messageId
    refs.streamingThreadId.value = stream.threadId
    refs.streamingBuffer.value = stream.buffer
    refs.retryingMessageId.value = stream.retryingMessageId
    refs.busy.value = true
  }

  /** Parks a turn that completed send setup after the user switched conversations. */
  function park(
    threadId: string,
    messageId: string,
    buffer = '',
    retryingMessageId: string | null = null,
  ) {
    parked.set(threadId, { threadId, messageId, buffer, retryingMessageId })
  }

  /** Opens `threadId` (or a new conversation, `null`): parks the turn left, restores the one met. */
  function switchTo(threadId: string | null) {
    parkActive(threadId)
    restore(threadId)
  }

  function parkedForMessage(messageId: string): ParkedStream | undefined {
    for (const stream of parked.values()) {
      if (stream.messageId === messageId) return stream
    }
    return undefined
  }

  function isParked(threadId: string): boolean {
    return parked.has(threadId)
  }

  /** The conversations whose turns run parked. */
  function parkedThreadIds(): string[] {
    return [...parked.keys()]
  }

  /** Whether `threadId` has a turn running, open or parked. */
  function isRunning(threadId: string): boolean {
    return refs.streamingThreadId.value === threadId || parked.has(threadId)
  }

  /** Whether any turn runs, in the open conversation or parked, or one is being set up. */
  function anyRunning(): boolean {
    return (
      refs.streamingMessageId.value !== null ||
      refs.turnSetupPending.value ||
      parked.size > 0
    )
  }

  /** Forgets the parked turn of `threadId` (it ended, or its conversation was deleted). */
  function forget(threadId: string) {
    parked.delete(threadId)
  }

  /** The reasoning entries that belong to parked turns, so clearing the rest keeps them. */
  function reasoningOfParked(
    reasoning: Record<string, string>,
  ): Record<string, string> {
    const kept: Record<string, string> = {}
    for (const stream of parked.values()) {
      const text = reasoning[stream.messageId]
      if (text !== undefined) kept[stream.messageId] = text
    }
    return kept
  }

  return {
    parkActive,
    restore,
    park,
    switchTo,
    parkedForMessage,
    isParked,
    parkedThreadIds,
    isRunning,
    anyRunning,
    forget,
    reasoningOfParked,
  }
}
