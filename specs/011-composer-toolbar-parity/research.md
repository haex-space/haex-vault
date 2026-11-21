# Phase 0 Research: Composer Toolbar Parity

All three findings below were fetched from Anthropic's/Claude Code's public
documentation on 2026-09-19, specifically to close the load-bearing unknowns
before writing FRs that presuppose them (spec.md Assumptions references this
file).

## §1: Real, controllable effort — two independent mechanisms

**Direct Anthropic API** (`docs.claude.com`/`platform.claude.com`, "Effort"
page): a top-level request field `output_config.effort`, values `low` /
`medium` / `high` / `xhigh` / `max`. Available with no beta header on:
`claude-fable-5-1`, `claude-mythos-5-1`, `claude-fable-5`, `claude-mythos-5`,
`claude-mythos-preview`, `claude-opus-5`, `claude-opus-4-8`,
`claude-opus-4-7`, `claude-opus-4-6`, `claude-opus-4-5-20251101`,
`claude-sonnet-5`, `claude-sonnet-4-6`. `xhigh` is a _strict subset_ of that
list (fable-5.1, mythos-5.1, fable-5, mythos-5, opus-5, opus-4-8, opus-4-7,
sonnet-5 — notably **not** opus-4-6/sonnet-4.6, which support `max` but not
`xhigh`; the two knobs are not simply nested tiers). Setting `high` is
documented as byte-identical to omitting the field (the API's own default).
`effort` is orthogonal to the `thinking` parameter — "effort works with or
without thinking" — so this feature adds `output_config.effort` as a new,
independent field alongside the existing `thinking`/`reasoning_requested`
wiring in `request.rs`, rather than replacing it.

**Claude Code CLI delegate**: a real, documented `--effort <level>` flag
(same five values), confirmed to apply in non-interactive mode ("Applies to
non-interactive mode: Yes") — i.e. it works with the exact `-p` invocation
`claude.rs::build_command` already constructs. Critically, Claude Code's own
docs state: "Unsupported levels fall back to highest supported level at or
below the requested one" — the CLI self-clamps per the model it's actually
running. This means haex-vault does **not** need to maintain a second copy of
Claude Code's own per-model level table for the delegate path (unlike the
direct-API path, where Anthropic's HTTP API is not documented to do this and
an unsupported value is safer to simply never send).

Source pages: `code.claude.com/docs/en/model-config` (settings.json
`effortLevel`, `--effort` CLI flag, `/effort` slash command, per-model
support table), `platform.claude.com/docs/en/build-with-claude/effort` (API
`output_config.effort` field, value table, defaults, effort-vs-thinking
relationship).

## §2: Sub-agent activity — `parent_tool_use_id`, not a distinct event type

`code.claude.com/docs/en/headless` ("Follow subagent messages"): every
`assistant`/`user` message in `--output-format stream-json` output carries a
`parent_tool_use_id` field. `null` means "main conversation"; a non-null
value is "the ID of the [Agent tool call](https://code.claude.com/docs/en/sub-agents) that spawned
it". Multiple sub-agents can be dispatched together from one assistant turn
(several `tool_use` blocks in one `assistant` message) and run concurrently
— this is exactly spec.md's "batch" concept, and it falls out for free: the
set of sub-agent-spawning `tool_use` blocks inside one `assistant` line
_is_ one batch, with no extra correlation bookkeeping needed beyond "which
line did these ids come from." Nesting is supported too (a sub-agent's own
`parent_tool_use_id`-tagged messages can themselves parent further
sub-agents), which spec.md's Edge Cases explicitly scope to "counted in the
same overall total, not visualized as a tree."

No distinct `system`-typed event exists for "a sub-agent started/stopped" —
an earlier web-search summary suggesting a
`{"type":"system","subtype":"task_started"}` event could not be corroborated
against the official docs page and is treated as unverified/incorrect; the
`parent_tool_use_id` mechanism above is the only integration point actually
documented in Anthropic's own reference and is what this feature implements
against.

Today, `claude.rs::parse_line` only recognizes `stream_event` (partial-delta)
and `result` (final) line types (`parse_line`, `claude.rs:66-125`) — a
`{"type":"assistant",...}`/`{"type":"user",...}` line (which is what
actually carries `parent_tool_use_id` and `tool_use`/`tool_result` content
blocks) falls through to `LineOutcome::Ignore` unconditionally. This
confirms the gap FR-007 closes is real, not hypothetical.

## §3: Attachment delivery per backend

**Direct Anthropic API**: the Messages API already accepts `image` and
`document` content blocks (base64-encoded) inside a `user` message's
`content` array — this is standard, stable Messages API surface, not a new
capability; `request.rs::build_messages` already builds a `content` array
for non-trivial user turns (its `tool_result` branch), so adding
image/document blocks to the _current_ user message is additive there.

**Claude Code delegate**: no dedicated "attach a file" flag exists for `-p`
mode. `claude.rs` already creates a disposable per-invocation temp directory
(`TempDir`, used today for `CLAUDE_CONFIG_DIR`/`cwd`/the system-prompt file)
that Claude Code's own file tools (Read, etc.) can access, since the process
is spawned with `.current_dir(tmp.path())`. This feature writes each
attachment's bytes into that same directory and mentions its filename in
`build_transcript_prompt`'s output — the same mechanism a human pasting a
file path into Claude Code would rely on — rather than inventing a
non-existent CLI attachment flag.

**Local (in-process) models and the Codex delegate**: no equivalent
mechanism verified or implemented by this feature (spec.md Assumptions/
Out-of-scope) — an attachment is classified `unusable` for these two
backends (FR-015), which is the correct, honest answer rather than a gap to
fill later under this feature.

## §4: Anthropic's own per-attachment limits (used as this feature's caps)

Referenced for the size caps in plan.md's Constraints: Anthropic's Messages
API documents a 5 MB per-image limit and a 32 MB per-PDF/document limit
(`docs.claude.com` vision/PDF-support pages, standard published limits, not
re-fetched separately for this feature since they are not in question the
way the effort/sub-agent mechanisms were — reused as sensible, already-
externally-validated defaults rather than an arbitrary internal number).
