# Contract: domain declarations and the routing call

## `AGENT_DOMAINS` (new, `src/lib/actions/agentTools.ts`)

```ts
export interface AgentDomain {
  /** The first segment of the action ids it covers, e.g. "contacts" for "contacts.search". */
  key: string
  /** One lower-case word the routing prompt names it by, e.g. "apps" for "wm". */
  name: string
  titles: { de: string; en: string }
  /** One line for the routing prompt: what the user asks for when they mean this domain. */
  description: string
  /** Tool names outside the domain that its chains need, e.g. ["wm_app_open"] for a domain with an app. */
  alsoNeeds: string[]
}
```

- Every action's id prefix MUST have a declared domain; `pnpm check:agent-actions` fails otherwise.
- `scripts/export-eval-tools.ts` writes the domains into `src-tauri/src/chat/eval/domains.json`, beside the tools'
  `tools.json`, whose format stays.
- They travel to Rust with the actions (`set_agent_actions`, `args.domains`), and the tool registry holds them.
- **Extensions**: an installed extension is one domain with key `x_<prefix>`, described as `<title>: <tool titles>`;
  `alsoNeeds` is `["wm_app_open"]`. It is named by what its tools are about, never by what the extension is called:
  the word most of its English tool titles share (singular and plural as one, written as most often; verbs and
  joining words do not count; at least two titles; the shorter spelling on a tie). haex-mail is `mail`, and so would be a mail client called
  Thunderbird. Without such a word, or when another domain already uses it, the name is the prefix, then the key.

## The routing call (`chat/tools/route.rs`)

- **Request**:
  - System message: "Which area of the app holzi does the user's request need? Areas:", one line `<name>: <description>`
    per installed domain, `none: anything else: general knowledge, arithmetic, the time, small talk`, and "Answer
    with one area name, or two if the request could mean either, comma-separated, nothing else."
  - User message: the user's text.
  - No tools, thinking off, at most 12 new tokens, deterministic.
- **Answer**: the domains named in the reply, by name or key in any case, in order, at most two; `none` alone means
  no domain.
- **Failure**: an error, a timeout (3 s for local engines, 15 s for hosted providers), or no known key in the reply:
  the first offer falls back to today's.
- **Who runs it**: the first step of a chat turn and the voice assistant's turn with a local model (`route::routes`;
  hosted models keep today's first offer, research R3), and the everyday runner, through the session's adapter. Not
  for model jobs (they bring their own tools).
