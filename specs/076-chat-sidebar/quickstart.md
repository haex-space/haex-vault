# Quickstart: checking the slices by hand

Run the app as in the repository's usual dev loop (`nix develop --command pnpm tauri dev`, or the CI
way on Pop!_OS). Use a vault with an agent session, two contacts, one group and a few notes.

## A — rail (US1)

1. Open the chat. The sidebar shows "Chats", the sidebar button and an icon button; no logo, no
   instance name, no row above the chat.
2. Press the sidebar button: a strip of avatars (agent, "Ich", contacts, groups) stays; hover shows
   names; the open chat is marked; unread shows a badge.
3. Press the icon button in both states: the new-conversation dialog opens.
4. Drag the window below 672 px: the strip stays; the sidebar button opens the full sidebar over the
   chat; picking a chat or Escape closes it.
5. Switch to another tab and back: the collapsed state is as left.

## B1 — composer (US3)

1. Open a contact conversation: the input looks like the agent's, with a microphone and a send button,
   without model, effort or permission controls.
2. Dictate a sentence with "send directly" on: it is sent as one message; off: it lands in the field.
3. Type 4,001 characters: the counter turns red and send is refused.
4. Open the agent conversation: nothing changed.

## B2 — search (US2)

1. Type part of a contact's name: the list narrows. Clear: it returns.
2. Type a word from a message: "Nachrichten" lists hits with name, extract and time; pick one: its
   conversation (for the agent: its session) opens.
3. Type `%` or `_`: they match literally; one character searches names only.
4. Collapse the sidebar: no search field; expand it: the typed text is still there.

## Automated

`pnpm check:chat-state`, `pnpm typecheck`, `pnpm typecheck:scripts`,
`pnpm lint`, `pnpm format:check`, `cargo test chat::search` (10 GB scope, `-j 1`), and for the branch
`gh workflow run ci.yml --ref 076-chat-sidebar` (E2E runs on `main` only).
