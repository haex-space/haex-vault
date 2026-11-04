# Contract: domain declarations and the routing call

## `AGENT_DOMAINS` (new, `src/lib/actions/agentTools.ts`)

```ts
export interface AgentDomain {
  /** The first segment of the action ids it covers, e.g. "contacts" for "contacts.search". */
  key: string
  titles: { de: string; en: string }
  /** One line for the routing prompt: what the user asks for when they mean this domain. */
  description: string
  /** Tool names outside the domain that its chains need, e.g. ["wm_app_open"] for passwords. */
  alsoNeeds: string[]
}
```

- Every action's id prefix MUST have a declared domain; `pnpm check:agent-actions` fails otherwise.
- `scripts/export-eval-tools.ts` writes the domains into `src-tauri/src/chat/eval/tools.json` beside the tools.
- **Extensions**: an installed extension is one domain with key `x_<prefix>`, its manifest's title and its
  description; `alsoNeeds` is empty.

## The routing call (`chat/tools/route.rs`)

- **Request**:
  - System message: "Which area of the app holzi does the user's request need? Areas:", one line `<key>: <description>`
    per installed domain, `none: anything else: general knowledge, arithmetic, the time, small talk`, and "Answer
    with one area name, or two if the request could mean either, comma-separated, nothing else."
  - User message: the user's text.
  - No tools, thinking off, at most 12 new tokens, deterministic.
- **Answer**: the domain keys found in the reply, in order, at most two; `none` alone means no domain.
- **Failure**: an error, a timeout (3 s for local engines, 15 s for hosted providers), or no known key in the reply:
  the first offer falls back to today's.
- **Who runs it**: the first step of a chat turn, the voice assistant's turn and the everyday runner, through the
  session's adapter. Not for model jobs (they bring their own tools).
