# Quickstart: Composer Toolbar Parity

## Prerequisites

- HaexVault with an open vault.
- A connected Claude Code delegate (spec 007-cli-delegate) for the delegate-
  specific scenarios (2, and half of 1 and 3).
- An Anthropic `api_key` provider with at least one effort-capable model
  (e.g. `claude-sonnet-5`) and one non-effort-capable model (any model not
  in `effort::anthropic_supported_levels`'s table) for the direct-API
  scenarios.
- At least one installed local model, to exercise the "no effort control at
  all" / "attachments unusable" paths.

## 1. Effort control matches what's actually selected

1. Select the direct-API `claude-sonnet-5` model. Open the composer settings
   popover.
2. Check: the effort section shows "Auto" plus `low`/`medium`/`high`/
   `xhigh`/`max` — not a 3-stop slider.
3. Pick `xhigh`, send a message. Check (e.g. via a debug log or the request
   actually taking visibly longer/more thorough): the request was built
   with `output_config.effort: "xhigh"`.
4. Switch to a model not in the effort-support table (or a local model).
   Check: the effort section disappears from the popover entirely.
5. Switch to the Claude Code delegate. Check: the effort section reappears
   showing all five levels (delegate always offers the full set,
   research.md §1) regardless of which underlying model the delegate is
   configured to use.
6. Pick `low`, send a message to the delegate. Check: the spawned `claude`
   process's argv includes `--effort low` (visible via `ps`/logging during
   manual verification).
7. Switch back to `claude-sonnet-5` with `xhigh` already selected, then
   switch to a model that only supports up to `high`. Check: the shown
   selection resets to "Auto" rather than silently keeping an unsupported
   value selected.

## 2. Sub-agent activity

1. Select the Claude Code delegate. Send a request specifically worded to
   make Claude Code dispatch multiple sub-agents in parallel (e.g. "use
   several Task agents to look at X, Y, and Z at the same time").
2. Check: an "● N agents" indicator appears in the toolbar once the
   sub-agents start, with N matching how many were dispatched together.
3. Check: as sub-agents finish, N decreases live; once all are done, the
   indicator disappears while the delegate's own final response continues
   streaming.
4. Send an ordinary request that spawns no sub-agents. Check: the indicator
   never appears.
5. Send a request to the local model or the Anthropic direct-API model.
   Check: the indicator never appears for these, regardless of what the
   response does.
6. Start a sub-agent-spawning delegate request, then stop it mid-flight.
   Check: the indicator clears immediately along with the rest of the
   stopped response's in-progress state.

## 3. Attachments

1. In the composer, select the "+" control. Pick an image file and a PDF.
2. Check: both appear as removable chips before sending.
3. With the Anthropic direct-API model active, send the message. Check: the
   response reflects the attached content (e.g. ask "what's in the image
   I attached").
4. Switch to a local model with the same attachments still staged. Check:
   each chip now shows an "unusable for this model" state before you send.
5. Remove one attachment via its chip's remove control. Check: only that
   one is gone; the others remain.
6. Try attaching a file above the size cap (research.md §4) or an
   unsupported type. Check: a specific reason is shown, not a generic
   failure, and the file is not added to the list.
7. Send a message with a valid attachment, then send a second message.
   Check: the second message's composer starts with no attachments carried
   over from the first (FR-017).
8. Attach a file, then delete it from disk before sending. Check: send
   still succeeds for the rest of the message, with a clear notice that the
   one attachment was excluded.

## 4. Automated checks

```bash
cargo test --lib
cargo test --test cli_delegate_claude
pnpm typecheck
```

Additionally diff `en.json`/`de.json`'s key trees for the new
`chat.composer.attachments.*`/`chat.agentActivity.*` strings to confirm both
locales stay in sync.
