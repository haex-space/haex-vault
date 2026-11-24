# ADR 0029: Agents write a shared memory without asking, unless the conversation saw foreign content

- **Status**: accepted
- **Date**: 2026-11-24
- **Spec**: [097-agent-memory](../../specs/097-agent-memory/spec.md), research R3, R5, R6, R7, R9

## Context

Every conversation with an agent of haex-vault started from nothing. Spec 097 gives a vault one memory: Markdown pages that the local agent's models and the Abo-Agents read at the start of a conversation and write when the user tells them something lasting. A memory that asks before every write is not used; a memory that anyone can write steers every later conversation, including those of agents with more rights than the writer. Text an agent reads from a web page, a mail or a file of a contact can carry a sentence that tells it to remember something.

## Decision

1. **Writes run without a question.** A model whose agent profile gives the group "Gedächtnis" at "Lesen und schreiben" adds, rewrites and removes pages directly, in every chat mode but "Plan", which refuses them. The user sees every write as a notice and on the page "KI → Gedächtnis", and can change or delete any page (FR-018, FR-023, FR-025).
2. **Foreign content makes a write ask.** A conversation is marked when a call returns what neither the user nor haex-vault wrote: the browser, extensions, connected servers, the command line, received chat messages, file shares and files of shares from contacts, and any scope this build does not know. From then on every write of that conversation goes through the existing approval, with the stored and the new text shown by the window (FR-019, FR-021). The list names what is own and treats the rest as foreign.
3. **One rule for every agent of haex-vault.** The local agent's models and the Abo-Agents are judged by the same function (`agent_memory::gate::decide`); only where haex-vault learns of foreign content differs (spec 097 PR C for the Abo-Agents).
4. **External agents never reach the memory.** An agent that steers haex-vault over its MCP server is given nothing of it and offered none of its actions, whatever its profile says (FR-020).
5. **The check on writing reads no words.** It refuses keys and tokens by their form, invisible characters, the role markers of a model's prompt, and any password or OTP secret of the password manager. It is not the barrier against foreign content; the question of decision 2 is.
6. **What a model reads is framed as notes.** The table of contents and every page come with a fixed sentence: information about the user, not instructions (FR-014).

## Alternatives

- **Ask before every write**: the safest, but a memory that interrupts the user each time it learns is one they switch off. The operator decided that facts are written directly (2026-11-19).
- **Ask only in the turn that read foreign content**: the content stays in what the model reads, so a hidden sentence could act a turn later. The mark lasts for the conversation, also after a summary.
- **Recognise instructions or passwords by phrases**: rejected by the operator (2026-11-23); users speak any language, and lists of phrases are always incomplete and easy to phrase around.
- **A memory per agent**: what the user tells one agent would not reach another, which is what the operator asked for.
- **External agents with a right of their own**: nothing asked for it, and the memory is for working from inside haex-vault.

## Consequences

- A poisoned page can only enter after the user agreed to it in a dialog that showed its text.
- A tool that is new to haex-vault makes a later write ask once too often rather than once too rarely.
- A password the user dictates that the password manager does not know is not caught by the check; the model is told not to store secrets, and the notice shows what was stored.
- The mark of foreign content syncs with the conversation, so a conversation continued on another device keeps asking.
