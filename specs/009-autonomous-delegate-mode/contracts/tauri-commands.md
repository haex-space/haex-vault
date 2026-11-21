# Contracts: Tauri Commands & Events (Autonomous Delegate Mode)

All commands are async, run in Tauri's `invoke_handler`, return `Result<T, HaexError>`. No new
dedicated commands are introduced by this feature (research.md §5) — it changes one existing
command's args and reuses the existing generic preference commands unchanged.

## Changed command: `send_message`

`SendMessageArgs` (`src-tauri/src/chat/commands.rs:50-66`) gains one new optional field. The public
`useChat` caller shape remains partially optional:

```typescript
{
  threadId?: string | null,
  content: string,
  systemPrompt?: string,
  maxNewTokens?: number,
  idempotencyKey?: string,
  autonomyMode?: 'standard' | 'ungated' | 'gated_permissive' | null   // NEW
}
```

This is the caller-facing shape, not a fully normalized payload. `sendMessageAsync` fills only an
omitted `idempotencyKey` before invoking Tauri; it forwards the other optional fields as supplied.

**Contract**:

- `autonomyMode: null` (or the field omitted, since it is `Option<AutonomyMode>` server-side)
  behaves identically to today's shipped `send_message` — `AutonomyMode::default()` is `Standard`
  (spec FR-002).
- A non-null value is threaded into the `ChatRequest` built for this single send (data-model.md) and
  applies only to this one request — `SendMessageArgs` carries no "remember this" flag, matching
  spec FR-008's never-persisted requirement structurally, not just by frontend convention.
- If `autonomyMode` is non-`standard` and the resolved backend for this request is **not** a
  `cli_delegate` provider, the value is ignored (not an error) — `local`/`api_key` adapters do not
  read this field at all (data-model.md). The frontend is expected not to surface the control in that
  case (plan.md's `DelegateAutonomyControl.vue` visibility rule), but the backend does not depend on
  the frontend enforcing that; an out-of-band or stale value is simply inert for those adapters.
- If `autonomyMode` requests `ungated` or `gated_permissive` for a delegate backend whose installed
  CLI version does not support the required native mechanism (spec edge case, FR-013), `send_message`
  returns this exact existing error shape (the command boundary maps
  `AdapterError::Unavailable` to `HaexError::InvalidInput`):

  ```json
  {
    "kind": "InvalidInput",
    "reason": "adapter start: backend unavailable: autonomy mode unavailable: vendor=<vendor>; mode=<mode>"
  }
  ```

  `<vendor>` is `claude` or `codex`, and `<mode>` is `ungated` or `gated_permissive`. The frontend
  recognizes the `reason` prefix `adapter start: backend unavailable: autonomy mode unavailable:`
  and maps it to the localized `errors.autonomyUnavailable` message; other `InvalidInput` errors
  keep their existing mapping. No new `HaexError` variant or generated binding is introduced.

**No change** to `SendMessageResult` — the response shape is identical; which autonomy mode a turn
used is discoverable from the persisted message record (data-model.md's new `autonomy_mode` column),
not from the command's return value. The existing `list_messages` history endpoint also remains
unchanged. Its existing row/DTO mappings must expose the additive field: nullable `autonomy_mode`
flows through storage `ChatMessage`, the history SQL projection, `row_to_message`, `MessagePayload`,
and the frontend `Message` type. Those mappings preserve `null` for legacy and non-delegate rows
rather than normalizing it to `standard`, so history rendering reflects only a mode actually
persisted for that turn.

## Unchanged commands, reused as-is

- `get_pref` / `set_pref` / `clear_pref` (`preferences_commands.rs:59-136`) — used unchanged for
  reading/writing the new `cli_delegate.deny_rules` device preference (data-model.md). No new
  command needed; `DelegateDenyRulesSetting.vue` calls these exactly like `DefaultModelSetting.vue`
  already does for its own preference.
- `respond_tool_permission` (`RespondToolPermissionArgs`, `commands.rs:532`) — **not used** by either
  new autonomy mode. `ungated` never creates a pending approval to respond to; `gated_permissive`'s
  `evaluate_deny_rules` (data-model.md) resolves synchronously inside `approval_bridge.rs` without
  ever emitting a `tool-permission-request` event or populating `pending_tool_approvals` for that
  call — there is nothing for this command to respond to in either mode. Confirming this at tasks
  time (a test that a `gated_permissive` run emits zero `tool-permission-request` events) is the
  cheapest way to prove the "no human pause" requirement (spec FR-003/FR-004) rather than only
  testing the end-to-end absence of a UI prompt.

## Events

No new event kind. The existing `tool-permission-request` event (`events.rs:28`, per 007's own
contract) is simply never emitted for a `gated_permissive` or `ungated` turn — its absence, not a new
payload shape, is the observable contract this feature adds (verified by the test above). Persisted
tool-call rows for a `gated_permissive` turn use the existing chat-history read path unchanged
(data-model.md's new `autonomy_mode` column is additive to an existing row shape, not a new query
surface).
