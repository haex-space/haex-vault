// Where an agent's call is asked (spec 021 US3, US6, FR-025): a delegate's call goes to its chat
// while a chat shows that thread, every other call to the call dialog. Pure, relative `.ts`
// imports only, so the Node check scripts can load it.

/** What routing needs of a call (`CallPromptEvent`, whose binding the scripts cannot load). */
export interface RoutedCall {
  question: { delegateThreadId?: string }
}

/** The chat threads shown now, counted per chat that shows them. */
export type OpenThreads = Readonly<Record<string, number>>

/** The calls the call dialog asks: all but those of a delegate whose chat is open. */
export function outsideCalls<T extends RoutedCall>(
  calls: readonly T[],
  open: OpenThreads,
): T[] {
  return calls.filter((call) => {
    const thread = call.question.delegateThreadId
    return !thread || !open[thread]
  })
}

/** The oldest call of a delegate in this chat thread. */
export function callFor<T extends RoutedCall>(
  calls: readonly T[],
  threadId: string,
): T | undefined {
  return calls.find((call) => call.question.delegateThreadId === threadId)
}

/** One more chat shows `threadId`. */
export function openThread(open: OpenThreads, threadId: string): OpenThreads {
  return { ...open, [threadId]: (open[threadId] ?? 0) + 1 }
}

/** One chat no longer shows `threadId`; the last one hands its calls back to the dialog. */
export function closeThread(open: OpenThreads, threadId: string): OpenThreads {
  const { [threadId]: count = 0, ...rest } = open
  return count > 1 ? { ...rest, [threadId]: count - 1 } : rest
}
