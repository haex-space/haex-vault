# Phase 1 Data Model: Composer Toolbar Parity

## EffortLevel

New enum, `src-tauri/src/adapters/effort.rs`.

```rust
pub enum EffortLevel { Low, Medium, High, XHigh, Max }
```

- `as_str(&self) -> &'static str`: `"low"` / `"medium"` / `"high"` /
  `"xhigh"` / `"max"` — the literal values both Anthropic's
  `output_config.effort` and Claude Code's `--effort` accept unchanged.
- `parse(s: &str) -> Option<Self>`: inverse, case-sensitive on the same
  literals (frontend always sends the canonical lowercase string).
- `anthropic_supported_levels(model_id: &str) -> &'static [EffortLevel]`: a
  curated static table (research.md §1) — empty slice for any model id not
  in Anthropic's documented effort-supporting list. Not a substring/version
  formula like `request.rs::supports_adaptive_thinking` — effort support
  and the `xhigh`/`max` split don't follow a clean numeric rule (opus-4-6/
  sonnet-4-6 support `max` but not `xhigh`), so this is an explicit,
  reviewable match table, same shape as `commands.rs`'s existing
  `EXACT_CAPABILITIES` array.
- `claude_delegate_levels() -> &'static [EffortLevel]`: always the full
  5-value set — Claude Code self-clamps per model (research.md §1), so
  haex-vault does not gate this by model id at all.
- `clamp(requested: EffortLevel, supported: &[EffortLevel]) -> Option<EffortLevel>`:
  `Some(requested)` if directly supported; otherwise the next lower
  supported level; `None` if `supported` is empty. Used by `request.rs`
  (direct API — never send an unsupported value) and by the
  `get_effort_levels` command (so a stale frontend selection re-clamps
  correctly, FR-002) — not needed on the delegate path (research.md §1).

`EffortLevel` also implements `serde::{Serialize, Deserialize}` as its
`as_str()`/`parse()` string form, so it can appear directly on
`SendMessageArgs`/`ChatRequest` and in the `get_effort_levels` command's
`Vec<String>` response without a separate DTO.

## `ChatRequest`/`SendMessageArgs` additions

`ChatRequest.effort_level: Option<EffortLevel>` (`types.rs`) — mirrors the
existing `autonomy_mode`/`reasoning_requested` pattern: every adapter
receives it, only the ones that understand it act on it. `None` means "no
override" — the direct-API adapter omits `output_config.effort` entirely
(true API default applies) and the delegate adapter omits `--effort`
entirely (true CLI default applies); this is what the composer's "Auto"
option sends.

`SendMessageArgs.effort_level: Option<EffortLevel>` (`commands.rs`) — same
semantics, threaded straight into the `ChatRequest` built for the turn.
Not persisted (same non-persistence as today's effort setting, spec.md Out
of scope).

## Sub-Agent Batch tracking

New `src-tauri/src/adapters/cli_delegate/subagents.rs`:

```rust
pub(super) struct Tracker {
    // tool_use id -> which batch (dispatch line) it came from
    pending: HashMap<String, usize>,
    // ids confirmed active (a later line referenced them as parent_tool_use_id)
    active: HashSet<String>,
    next_batch_id: usize,
}

pub(super) enum TrackerEvent {
    /// Active count unchanged in a way the UI needs to see, or nothing
    /// tracker-relevant happened on this line.
    None,
    /// `active_count` after this line's effect; `batch_size` is `Some(n)`
    /// only on the line where a new batch of `n` sub-agents was confirmed.
    Update { active_count: usize, batch_size: Option<usize> },
}
```

- `observe_top_level_tool_use(&mut self, ids: &[String])`: called when an
  `assistant` message with `parent_tool_use_id: null` carries one or more
  `tool_use` blocks — records each id as `pending` under one new
  `next_batch_id` (one call = one batch, since they arrived in the same
  stream-json line = the same assistant turn, research.md §2). Returns
  `TrackerEvent::None` — a pending id isn't confirmed as a sub-agent until
  something references it as a parent.
- `observe_parent_reference(&mut self, parent_tool_use_id: &str) -> TrackerEvent`:
  called for every message whose `parent_tool_use_id` is non-null. If that
  id is in `pending`, promotes it to `active` (removes from `pending`) and
  returns `Update` with the new `active.len()` and, if this is the first
  promotion for that id's batch, `batch_size` = how many pending ids shared
  that batch id (computed by counting, not stored redundantly). Already-
  active ids (a sub-agent's _own_ further messages also carry the same
  `parent_tool_use_id`) are a no-op past the first promotion.
- `observe_tool_result(&mut self, tool_use_id: &str) -> TrackerEvent`:
  called for a main-thread (`parent_tool_use_id: null`) `tool_result` block.
  If `tool_use_id` is in `active`, removes it and returns `Update` with the
  new (possibly zero) `active.len()`.

Kept as its own struct (not folded into `claude.rs`'s existing per-call
state) specifically so `subagents_tests.rs` can drive it with a plain
sequence of synthetic ids/lines — no NDJSON parsing, no process, mirroring
`claude_tests.rs`'s existing test shape for `parse_line`.

`claude.rs::parse_line` gains a `tracker: &mut subagents::Tracker` parameter
and, for `assistant`/`user`-typed lines (new branches beside the existing
`stream_event`/`result` match arms), extracts `parent_tool_use_id` and any
`tool_use`/`tool_result` blocks, calls the three `Tracker` methods above as
appropriate, and — when any of them returns `Update` — returns a new
`LineOutcome::Chunk(StreamChunk::AgentActivity { active_count, batch_size })`
instead of `Ignore`.

`StreamChunk::AgentActivity { active_count: usize, batch_size: Option<usize> }`
(`types.rs`) — only ever produced by the Claude Code delegate adapter; every
other adapter's match on `StreamChunk` simply never receives this variant
(FR-010 is satisfied structurally, not by a runtime check).

## Message Attachment

New `src-tauri/src/chat/attachments.rs`:

```rust
pub enum AttachmentKind { Image, Document, Text }

pub struct AttachmentInfo {
    pub name: String,
    pub size_bytes: u64,
    pub kind: AttachmentKind,
    pub usable: bool,
    /// Set when `usable` is false, or when size/type is rejected outright.
    pub reason: Option<String>,
}

pub struct Attachment {
    pub name: String,
    pub kind: AttachmentKind,
    pub media_type: String,   // e.g. "image/png", "application/pdf", "text/plain"
    pub bytes: Vec<u8>,
}
```

- `classify_attachment(path: &Path) -> Result<AttachmentInfo, HaexError>`:
  extension-based `AttachmentKind` + media type detection, a `stat()` size
  check against the per-kind caps (research.md §4: 5 MB image / 32 MB
  document / 5 MB text), independent of which backend is active — this is
  "can this file even be attached at all" (FR-016), always run at attach
  time via the `inspect_attachment` command.
- `usability_for(kind: AttachmentKind, provider_kind: ProviderKind, adapter: Option<&str>) -> bool`:
  the per-backend policy (FR-015): `true` for the direct Anthropic API
  (`ProviderKind::ApiKey`, adapter `"anthropic"`) and the Claude Code
  delegate (`ProviderKind::CliDelegate`, adapter `"claude"`) for every
  `AttachmentKind`; `false` for `ProviderKind::Local` and the Codex delegate
  (research.md §3) regardless of kind.
- `read_attachment_content(path: &Path, kind: AttachmentKind) -> Result<Attachment, HaexError>`:
  re-stats and re-reads the file at send time (FR-018 — a file can vanish
  between attach and send), base64-irrelevant at this layer (returns raw
  bytes; each adapter encodes to whatever its wire format needs).

`ChatMessage.attachments: Vec<Attachment>` (`types.rs`) — only ever
populated on the _current_ turn's user message, built fresh in
`commands.rs::send_message` from `SendMessageArgs.attachments` (a
`Vec<AttachmentInput { path: String }>`, frontend sends just the picked
paths). `history_to_messages` (existing function, `commands.rs`) continues
to produce `attachments: vec![]` for every persisted historical row — the
field's default — since attachments are never persisted (spec.md Out of
scope).

`request.rs::build_messages`: the current user message's attachments
become extra content blocks (`{"type":"image",...}` / `{"type":"document",...}`
/ plain appended text for `AttachmentKind::Text`) alongside its existing
text, only for `AnthropicAdapter`.

`claude.rs::spawn_claude_invocation`: each attachment's bytes are written
into the existing per-invocation `TempDir` (already the process's `cwd`),
and `build_transcript_prompt`'s output gets one line per attachment naming
its (relative, in-sandbox) path — never the original host path (constitution
II's spirit: nothing host-specific crosses into what the delegate sees).

## Frontend composer state (`[instance].vue`)

- `effortLevel: Ref<EffortLevel | null>` — replaces today's
  `ref<'low'|'medium'|'high'>('medium')`; `null` is "Auto" and is the
  initial value (FR-005).
- `effortLevels: Ref<string[]>` — the capability list for the active
  model/backend (from `getEffortLevelsAsync`), re-fetched on every
  `activeModel` change; empty hides the effort section entirely (FR-003);
  a stale `effortLevel` not present in a new list is reset to `null`
  (FR-002 — "Auto" rather than silently guessing a nearest level on the
  frontend, since the backend's own `clamp()` is the authoritative
  re-clamp for whatever gets sent).
- `activeAgentCount: Ref<number>` and `lastAgentBatchSize: Ref<number | null>`
  — updated by `onAgentActivity`, reset to `0`/`null` at the same points
  `streamingMessageId`/`busy` already reset (turn complete, error, abort).
- `attachments: Ref<ComposerAttachment[]>` — cleared after a successful
  send, the same lifecycle point `input.value = ''` already uses.
