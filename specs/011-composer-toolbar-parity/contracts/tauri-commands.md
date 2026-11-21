# Contracts: New/Changed Tauri Commands and Events

## Commands

### `get_effort_levels` (new)

```ts
getEffortLevelsAsync(modelId: string): Promise<string[]>
```

- `modelId` is the same composite id (`"<provider-uuid>:<remote-id>"` or a
  bare local catalog id) already used by `loadModelAsync`/`activeModelInfoAsync`.
- Resolves the provider row via `storage::providers::get_provider` (existing
  function) when the id contains `:`; a bare id (local model) always
  resolves to `[]`.
- Returns `[]` for: local models, the Codex delegate, and any Anthropic
  model not in `effort::anthropic_supported_levels`'s table.
- Returns the `EffortLevel::as_str()` list otherwise — for a direct-API
  model, exactly what that model supports (e.g. `["low","medium","high","max"]`
  for `claude-opus-4-6`, plus `"xhigh"` for `claude-sonnet-5`); for the
  Claude Code delegate, always the full five values (research.md §1 — the
  CLI self-clamps, so haex-vault always offers the full set for it).
- Frontend prepends `"auto"` itself whenever the returned list is non-empty
  (data-model.md) — this command never returns `"auto"`.
- Pure/synchronous on the backend (no network call) — safe to call on every
  `activeModel` change without debouncing.

### `inspect_attachment` (new)

```ts
inspectAttachmentAsync(path: string): Promise<AttachmentInfo>

interface AttachmentInfo {
  name: string
  sizeBytes: number
  kind: 'image' | 'document' | 'text'
  usable: boolean
  reason: string | null   // set when usable is false, or on a size/type rejection
}
```

- Called immediately after the file picker resolves a path (Story 3,
  Acceptance Scenario 5) — before the file is added to the composer's
  attachment list, so the chip can render its usability state right away.
- Rejects (throws) only for a file that cannot be read/stat'd at all (e.g.
  vanished between picking and inspecting); an oversized or wrong-type file
  still resolves normally with `usable: false` and a specific `reason`, so
  the composer can show _why_ rather than a generic failure.
- Usability (`usable`) is evaluated against the **currently active**
  model/backend at the moment of the call — re-evaluated by the frontend
  (a fresh call per attachment) whenever the active model changes while
  attachments are already staged (spec.md Edge Cases).

### `send_message` (changed)

`SendMessageArgs` gains two fields, both optional and both following the
existing non-persistence convention `autonomyMode` already established:

```ts
interface SendMessageArgs {
  // ...existing fields unchanged...
  effortLevel?: 'low' | 'medium' | 'high' | 'xhigh' | 'max' | null
  attachments?: { path: string }[]
}
```

- `effortLevel: null`/omitted → no override (data-model.md "Auto").
- `attachments`: the same paths already validated via `inspect_attachment`;
  `send_message` re-validates (`chat/attachments.rs::read_attachment_content`)
  rather than trusting the earlier inspection, since time has passed
  (FR-018). A path that fails re-validation is dropped from the request and
  reported back via the existing error-surfacing path (`lastError`), not a
  new event — the send itself still proceeds with whichever attachments did
  read successfully, consistent with "exclude only that attachment" (FR-018).

## Events

### `chat-agent-activity` (new)

```ts
interface AgentActivityEvent {
  messageId: string
  activeCount: number
  batchSize: number | null // set only on the update where a new batch started
}
```

- Emitted from `chat/turn/step.rs::consume_stream`'s existing `StreamChunk`
  match, the same emission path `chat-token` already uses — same ordering
  guarantees relative to other per-turn events.
- Only ever emitted while a Claude Code delegate response is streaming;
  never emitted by any other backend (structural — see data-model.md).
- The frontend does not need a separate "cleared" event: the existing
  `chat-turn-complete`/`chat-message-error`/abort paths already reset all
  other per-turn UI state, and this feature's `activeAgentCount` reset is
  wired into that exact same reset point (FR-011) — `activeCount` reaching
  `0` mid-stream (last sub-agent in a batch finishes, more text follows)
  and the turn _ending_ are different things and both are handled, but only
  the latter needs a new code path (the existing one), not this event.
