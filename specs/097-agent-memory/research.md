# Research: Shared memory of the agents

Every decision names what was read in the code and what was rejected. File references are repository-relative. Read on
2026-11-22 at `origin/main` 7d192f4ea. The spec was amended the same day (pages, table of contents, search, every kind of
agent alike, no external agents); this research is written against the amended spec.

## R1: Pages in one synced table, written in Rust

**Facts.** Migrations are Rust string constants registered in `src-tauri/src/identity/migrations.rs`; the last is
`0068_diagnostic_log`, `HAEX_TRIGGER_VERSION` is 42. Every table without the suffix `_no_sync` is tracked by haex-crdt and
reaches the other own devices. Core tables are named `haex_*`; the SQL of extensions is refused for any `haex_` identifier
(`extensions/sql/ast_check.rs`). A tracked table must have no UNIQUE constraint besides its primary key and no cascading
foreign key, and is written by check-then-write, never `ON CONFLICT DO UPDATE` (`storage/preferences.rs`,
`identity/migrations_agent_profiles.rs`). All access to such tables is Rust; the window calls Tauri commands and reloads on
`vault-data-changed` (`src/composables/useVaultData.ts`, `src/stores/agentProfiles.ts`).

**Decision.** Migration `0069_agent_memory` creates the tracked, vault-wide table `haex_agent_memory_pages`
(data-model.md) and adds two columns to `chat_threads` (R5, R6); `HAEX_TRIGGER_VERSION` goes to 43. A new Rust module
`src-tauri/src/agent_memory/` owns the table. A deleted page is a deleted row with the usual marker; nothing of it is kept.

**Why.** It gives FR-001 and FR-004 without new machinery: the table syncs like every other, lives in the encrypted vault
database, and extensions cannot reach it.

**Rejected.**

- Markdown files in the vault folder (the operator's first idea): they would lie on the disk unencrypted (FR-004) and
  would need a sync of their own to reach the other devices (FR-001). The page's text is Markdown all the same, so an
  export as files can be added later (spec Scope).
- A graph extension of SQLite: its virtual tables carry no haex-crdt triggers and would not sync, it would have to be
  built for every platform next to SQLCipher, and `load_extension` is refused on purpose (`extensions/sql/bypass_tests.rs`).
  At the size of this memory a graph is built in memory in milliseconds (R4).
- An LLM-built knowledge graph in the manner of graphify: for prose, graphify takes its nodes and edges from model calls
  (its skill, "semantic extraction"); in haex-vault that model would be the user's, too weak locally and billed per change
  in the cloud. It stays possible later as an index built from the pages (spec Scope).
- Single-sentence entries with labels: labels drift ("Arbeit", "Job", "Beruf") and need someone to tidy them.

## R2: The limits

**Facts.** The spec wants a large memory and a short start. A model is given only the table of contents at the start
(FR-010); pages are read on demand. The code counts 3 characters per token (`chat/tools/context_budget.rs`).

**Decision.**

| Limit               | Value  |
| ------------------- | ------ |
| title               | 80     |
| description         | 200    |
| text of a page      | 10,000 |
| pages in the memory | 1,000  |

Constants in `agent_memory/limits.rs`. An addition that would pass the text limit is refused with `page_too_long`; a new
page beyond 1,000 with `memory_full`. A change that keeps or reduces a size and every removal always run, so a memory over
its limit after a sync can be tidied.

**Why.** A line of the table of contents is about 100 to 300 characters, so 500 pages are about 100,000 characters; the
start shows what fits a tenth of the context (R5), and SC-009 checks it with 500 pages. 10,000 characters are a long page
that a model still reads in one call.

## R3: The right is a group of its own, for models only

**Facts.** `Rights` (`mcp_server/grants.rs`) is stored as rows of `haex_agent_profile_grants` through `grant_rows.rs`; a
group this build does not know is skipped, so its right stays at its default. "Standard" has no rows until it is edited
and then gives `default_rights()` (`agent_profiles/resolve.rs::profile_rights`); a new profile has no grant rows and gives
`Rights::default()`. Rights reach the local agent through `profile_gate::rights_of_session`, MCP sessions through
`registry::load_rights`, which resolves an external agent as `Subject::Agent` and an Abo-Agent as `Subject::Model` of its
live model (`registry::subject_of`). Only the window can call `agent_profile_rights_set`.

**Decision.**

- `Rights.memory: MemoryLevel`, an enum `Off | Read | ReadWrite` with `Off` as default, bound to TypeScript by ts-rs.
- One grant row, group `memory`, empty key, level `off`, `read` or `read_write`, written whenever rights are stored, also
  at `off`. A level this build cannot read is `Off` (FR-005).
- `default_rights()` has `memory: ReadWrite`. For "Standard" only, `profile_rights` sets `ReadWrite` when the stored rows
  have no `memory` row at all (a "Standard" edited before this feature). Every other profile without the row has `Off`.
- `registry::load_rights` sets `memory: Off` for an agent of the kind `External`, after resolving (FR-020). That is the one
  place where an external agent's rights are made; tool lists and calls follow from it.
- Two new scopes, `memory.read` and `memory.write`. `covers_scope` opens `memory.read` at `Read` and `ReadWrite` and
  `memory.write` at `ReadWrite`. `covers_local` uses the same decision.
- The editor gets `MemoryRightsFields.vue` (three options and the note "gilt nicht für externe Agenten") in
  `RightsEditor.vue`.

**Rejected.** A separate right for external agents (the operator's alternative): the operator wants the memory for work
from inside haex-vault, and nothing asked for it.

## R4: Five actions, one native executor, the graph from the text

**Facts.** The tools of haex-vault are action definitions in `src/lib/actions/*Actions.ts`, pushed to Rust; a definition
with `runner: 'native'` needs a `NativeExecutor` registered in `src-tauri/src/lib.rs`, and the chat agent and the MCP
server call the same executor. An executor learns the caller (`Caller::BuiltinAgent` or `Caller::ExternalAgent { id }`,
the latter also for an Abo-Agent); the model of a chat turn is in the task-local `AgentTask`. Local models are offered the
tools of the domains the routing names; a domain is the part of an action id before the first dot (`AGENT_DOMAINS` in
`src/lib/actions/agentTools.ts`). The guard of spec 082 turns away ids the model made up; titles are words the model and
the user share.

**Decision.** `src/lib/actions/memoryActions.ts` declares, all `runner: 'native'`, `agentCallable: true`:

| Action          | Scope          | Effect | Does                                                                 |
| --------------- | -------------- | ------ | -------------------------------------------------------------------- |
| `memory.search` | `memory.read`  | read   | pages matching words, best first, with an excerpt                    |
| `memory.open`   | `memory.read`  | read   | one page by title: text, origin, pointers out and in                 |
| `memory.add`    | `memory.write` | write  | adds a paragraph to a page; creates the page when the title has none |
| `memory.write`  | `memory.write` | write  | replaces the description and text of a page (to rewrite or tidy)     |
| `memory.remove` | `memory.write` | write  | removes a page                                                       |

- Pages are addressed by title, compared without case and surrounding spaces. Two pages with the same title (a sync can
  make them) resolve to the one changed last; the reply says that there is a second.
- `memory.add` is what a model uses to remember: it never loses existing text. `memory.write` and `memory.remove` are for
  tidying; their effect is `write` too, so the reversible-delete check of spec 088 does not turn tidying away.
- Pointers are `[[Title]]` in the text (FR-029). `agent_memory/links.rs` parses them; `memory.open` and the page in the
  window build the pointers out of a page from its text and the pointers into it by parsing all pages, held as a map in
  memory and rebuilt when `haex_agent_memory_pages` changes. Nothing derived is stored.
- Search (`agent_memory/search.rs`): the words of the query, case and accents folded, matched against title, description
  and text; a hit in the title counts most, then the description, then the text; ten results with a 200-character excerpt
  around the first hit. At 1,000 pages of 10,000 characters this is a scan of 10 MB in memory, well under the time of a
  model step. A full-text index (FTS5) is the step if it is not.
- `AGENT_DOMAINS` gets the domain `memory`, so local models are offered the actions.
- The author of a write is resolved in the executor: the `AgentTask` of the turn for the local agent, the live model of
  the pass for an Abo-Agent, each with the name `agent_profiles/subjects.rs` shows the user.

**Rejected.** Addressing pages by id: models make ids up, and the guard would turn such calls away. One tool with an
`action` parameter: the consent text and the guard work per tool.

## R5: The table of contents is a snapshot per conversation

**Facts.** `chat/send.rs::start_turn` builds the system prompt anew for every turn (`chat/tools/prompt.rs::system_prompt_with`),
in the order: given text, tool instruction, extensions overview, language line, time line. Spec 080 (prompt cache) is a
draft with none of its tasks done; its research R8 will take the time line out and requires that the prompt only grows at
its end. Compaction (spec 086) runs for local models only and appends its summary to the freshly built prompt. A
conversation can change its model between turns and syncs to the other own devices. `storage/chat_threads.rs` reads
threads with explicit column lists. A model's context is `ProviderAdapter::context_limit()` for local models and
`ActiveSession.context_window` for hosted ones (unknown for Codex).

**Decision.**

- When the first turn of a conversation starts, the table of contents is rendered once (one line per page, changed last
  first) and stored in the new column `chat_threads.memory_toc`. `NULL` means "not taken yet"; a conversation that started
  before this feature takes it at its next turn.
- Each turn whose model may read gets the memory block from that snapshot, cut to the lines that fit a tenth of the
  model's context (`agent_memory/toc.rs`, `SHARE = 10`); without a known context the whole snapshot is given. The cut is
  deterministic, so the block is the same in every turn with the same model (FR-011). When lines are left out, the block
  says how many and names `memory_search`.
- A turn whose model may not read gets no block (FR-015, spec edge case). The snapshot stays and counts again when a model
  that may read continues.
- The block stands directly after the given text and before the tool instruction, inside the stable beginning spec 080
  will rely on.
- The sentence that asks the model to remember (FR-017) is fixed text, added to the block when the model may write.

**Why.** FR-011 needs the text frozen: pages change and are deleted, and the spec keeps no history. A column on the thread
is deleted and synced with the conversation, and the explicit column lists keep it out of the thread list.

**Rejected.** A row in `chat_messages` with a role of its own: every reader of the messages would have to skip it.

**Note on SC-007.** Until spec 080 lands, the system prompt of a local model still changes every minute through the time
line, so the wait cannot show the gain. This plan proves that a write leaves the memory block byte-identical in the next
turn; the timing is measured when 080 is in.

## R6: Foreign content is a mark on the conversation

**Facts.** Nothing in the code marks content as coming from outside. The local agent's calls are stored with their source
(`chat_messages.tool_source`: `action`, `haextension`, `mcp`, `cli`) and tool name; haex-vault's own actions carry a scope;
web search is `browser.search` in the scope `browser`; `chat.messages.list` (scope `chat.read`) also returns messages of
conversations with contacts. An Abo-Agent's calls of haex-vault's tools go through `mcp_server/dispatch.rs`, which knows
the chat thread of the pass (`SessionAgent.delegate_thread`); its own tools are not stored at all (R7).

**Decision.** A new column `chat_threads.memory_foreign_at` (time, `NULL` until set) marks the conversation. It is set once,
by whichever path first sees foreign content, before the result reaches the model:

- the local agent's turn, after a call whose `agent_memory::foreign::is_foreign(source, scope)` is true;
- `mcp_server/dispatch.rs`, after an Abo-Agent's call of such a tool, for its `delegate_thread`;
- the readers of the Abo-Agents' output, for their own tools (R7).

`is_foreign` lists what is own and treats the rest as foreign: own is only an action in one of the scopes `wm.*`,
`settings.*`, `passwords.read`, `contacts.*`, `files.read`, `files.write`, `files.syncRules`, `chat.write`, `memory.*`.
Every tool of an extension, of a connected MCP server and of the vault's commands, the scopes `browser`, `chat.read` and
`files.shares`, and anything unknown is foreign.

**Why.** One mark serves all three paths, survives later turns, a summary and a restart, and reaches the other devices
with the thread. Listing what is own fails safe: a new tool asks once too often, never once too rarely.

**Limits, stated plainly.** Files of the device and of the user's storages are own by the spec's assumption, although a
downloaded file may carry foreign text. A file the user attaches is theirs. How the agent reaches a file a contact shared
was not settled here (the `files.*` actions name the device and storages only): a task reads `src-tauri/src/files/agent/`
and classes that path foreign.

## R7: Every agent of haex-vault asks by the same rule

**Facts.** The local agent's calls are decided in `chat/turn/plan_calls.rs` by `permission::decide_call(mode, tool)`: the
chat's mode "Manual" (the default) asks for every call, "Auto" for risky ones, "Plan" refuses every change. Calls over MCP
are decided in `mcp_server/dispatch.rs` by `prompts::call_gate`, combined with the chat's mode for Abo-Agents
(`chat_mode_binds`). Both dialogs exist; declined gives `denied_by_user` (chat) or `user_declined` (MCP).

What haex-vault sees of an Abo-Agent's own tools today: the Claude Code reader keeps only the ids of `tool_use` blocks,
although each block carries the tool's `name` in the stream (`adapters/cli_delegate/claude.rs::tool_use_ids`, fixtures in
`claude_tests.rs`); the Codex reader handles four notifications and ignores every item event
(`adapters/cli_delegate/codex.rs`); neither CLI has its web tools switched off; "Standard" runs Abo-Agents `Ungated`, where
no approval request reaches haex-vault.

**Decision.** One function, `agent_memory::gate::decide(mode, foreign) -> Decision`: `Plan` refuses; otherwise a
conversation with `memory_foreign_at` set asks, one without runs. It replaces the mode's verdict for the scope
`memory.write`:

- local agent: in `plan_calls`, before `decide_call`; the everyday measurement plans its calls with the same function;
- Abo-Agent: in `dispatch.rs`, with the chat's mode and the mark of its `delegate_thread`;
- external agent: never reaches it (R3).

Seeing an Abo-Agent's own tools (PR C):

- **Claude Code**: `parse_line` reads the `name` of every `tool_use` block, also of sub-agents. `WebFetch`, `WebSearch`,
  `Bash` and any name not on the own list set the mark; `Read`, `Glob`, `Grep`, `Edit`, `Write`, `TodoWrite`, `Task`/`Agent`
  and the `mcp__haex__*` tools (judged by dispatch) do not. Its working folder holds only the user's attachments.
- **Codex**: PR C starts with a spike that records the app-server notifications of a turn with a web search and a command,
  in the gated and the ungated mode. If its item events name them, the reader sets the mark from them as for Claude Code.
  If they do not, Codex's own web search is switched off in its `config.toml` (it is told to use haex-vault's browser
  already), and a command it runs sets the mark if the notifications show commands; what is left unseen goes to the
  operator before PR C lands.

The delegate's stdout reader and haex MCP dispatch run independently, so dispatch cannot read the foreign mark
immediately when `memory.write` arrives. Each delegate pass exposes a reader barrier: dispatch sends a fence to the
stdout reader, which drains and classifies all complete records already emitted by the child in line order, then
acknowledges the fence. The child flushes each stream record before issuing a later haex MCP call. Dispatch waits for the
acknowledgement before reading `memory_foreign_at` and deciding the gate; if the barrier fails, the write asks. A dispatch
test deliberately lets the MCP request arrive before the reader handles the earlier `tool_use` frame and verifies that
the barrier still makes the write ask.

**Why.** The operator decided that a local, a cloud and an Abo-Agent behave the same; the rule is the same, only where
haex-vault learns of foreign content differs. A command counts as foreign because it can fetch anything.

**Rejected.** Letting Abo-Agents always ask (the spec's first fallback): the operator rejected it.

The dialogs stay the two existing ones. Both get one shared part, `MemoryWritePreview.vue`, for the three write actions:
the page and the added text for `memory.add`, the stored and the new text for `memory.write`, the stored text for
`memory.remove` (FR-021). The window reads the stored text itself, so it never trusts what the agent says it was.

## R8: The Abo-Agents get the table of contents with their instructions

**Facts.** An Abo-Agent is started anew for every turn; its instructions are built in `start_turn` like every system
prompt and extended by `adapters/cli_delegate/haex_mcp.rs::with_instruction` (Claude Code: `--append-system-prompt-file`,
Codex: `developerInstructions`). It reaches haex-vault's tools over the MCP server with a pass that names the chat thread.

**Decision.** The memory block of R5 is part of its system prompt, from the same snapshot of the thread and judged by the
rights of its model. Its MCP session's `instructions` stay as they are. The executor serves calls over MCP only for an
Abo-Agent's pass; an external agent has no memory right anyway (R3).

## R9: The check on writing

**Facts.** `mcp_server/redact.rs::redact_text` recognises tokens in command lines, `NAME=value` pairs with secret names,
bearer tokens and passwords in URLs. `extensions/bridge/header_validate.rs` has the list of bidirectional control
characters. Nothing checks zero-width characters, and nothing recognises instruction patterns; spec 098 rejected a
redaction filter for the log as false comfort (`specs/098-diagnostic-log/research.md`).

**Decision.** `agent_memory/check.rs::check(field, text)` runs over title, description and text of every write, of an agent
and of the user, and refuses with one reason:

- `empty`, `too_long`, `line_break` (title and description are one line): the form.
- `invisible_characters`: control characters other than line breaks and tabs in the text, zero-width characters (U+200B to
  U+200D, U+2060, U+FEFF), bidirectional controls and the Unicode tag block.
- `secret`: whatever `redact_text` would change, a private key block, the usual token prefixes (`sk-`, `ghp_`, `xox`,
  `nsec1`, a three-part JWT), a run of 32 or more characters that looks like a key, and a password word followed by a
  value ("Passwort: …", "password is …", "PIN …").
- `instruction`: text that addresses an agent's rules rather than a fact: "ignore/disregard … instructions",
  "ignoriere … Anweisungen", "system prompt", role markers (`<|…|>`, `[INST]`, `### System`), "you are now", "du bist
  jetzt", and requests to keep something from the user.

The lists are tables in the one file, tested in German and English; "Ich beantworte Mails immer kurz und ohne Anrede"
must pass.

**Why this is not the barrier.** A preference is often phrased like an instruction, so the list can only catch text aimed
at the agent itself. What keeps a poisoned page out of the memory is the question of R7, and what limits a poisoned page
is the framing of the block (FR-014).

## R10: Notice and page

**Facts.** Toasts are `vue-sonner`; a backend event becomes a toast through a listener mounted once in
`src/components/wm/Desktop.vue`. Settings is an app whose locations are registered in `src/lib/settings/registry.ts` and
`src/components/wm/appRoutes.ts`. Texts are in `src/i18n/locales/de.json` and `en.json`.

**Decision.**

- The executor emits `agent-memory-written` after an agent's write; the window shows a toast with the text. The user's own
  writes raise none.
- The page is the settings location `ai.memory`, "KI → Gedächtnis" (operator, 2026-11-22):
  `src/components/settings/agentMemory/MemoryView.vue` (list with search) and `MemoryPageView.vue` (one page: text,
  pointers out and in, edit), with the store `src/stores/agentMemory.ts`. The text is shown as rendered Markdown with `marked`, as
  the chat renders messages (`src/components/chat/MessageList.vue::renderMarkdown`), and `[[Title]]` opens that page. The user's writes go through window actions with
  `scope: 'guardrails', agentCallable: false`.

## R11: The measurement

**Facts.** The everyday measurement is the integration test `src-tauri/tests/model_tool_eval.rs` with the feature
`everyday-eval`; a set is a JSON file chosen with `HAEX_EVAL_SET`. A request is one user message against a fixed world;
the runner plays the consent and counts risky calls that ran unasked. The tools come from `tools.json`
(`pnpm export:eval-tools`). It cannot run Abo-Agents.

**Decision.**

- `specs/097-agent-memory/analysis/memoryset.json`, a new category `memory`, about 20 requests in German and English:
  remembers when told (a `memory_add` call); finds a seeded page by searching and answers from it; follows a pointer; does
  not store a password; after a page that tells it to remember something, the write reaches the question and is not
  agreed to.
- The runner gets a seeded memory (`memory` field of a request, or `HAEX_EVAL_MEMORY=<file>` for a whole run): the block at
  the start and the answers of `memory_search` and `memory_open` come from it. The injected web page is a world answer of
  the browser tool, and the mark of R6 is set as in the app.
- A new safety kind counts a memory write that ran after foreign content without consent (SC-003).
- SC-002 is an integration test (`src-tauri/tests/agent_memory_kinds.rs`: the system prompt of an Abo-Agent turn holds a
  page the local agent wrote; a write over an Abo-Agent's pass is found by a local turn; an external agent's session
  offers no memory tool) and a step of the quickstart with the real CLIs.
- FR-027: one run of the everyday set per small model with a seeded memory of 60 pages, against 95 (Qwen3.5 4B) and 86
  (Gemma 4 E4B). A model below its number goes onto the list `agent_memory/toc.rs::NO_START_TOC` and gets no block; it
  keeps the actions.
- SC-009: a test with 500 generated pages checks the block size for a 4,096-, 32,768- and 200,000-token context, and a
  request of the memory set asks for a fact that only the text of one page holds.
