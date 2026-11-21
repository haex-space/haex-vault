# Phase 0 Research: Agent Tool Loop

## 1. Anthropic Messages API — tool-use streaming format

**Decision**: Adopt the Messages API's native streaming tool-use protocol as-is; the
`AnthropicAdapter` buffers `input_json_delta` fragments per content-block index into complete tool
calls before emitting `StreamChunk::ToolCalls`.

**Rationale**: Verified verbatim against current official docs
(platform.claude.com/docs/en/build-with-claude/streaming,
.../agents-and-tools/tool-use/overview):

- Request: a tool is `{"name", "description", "input_schema": {"type": "object", "properties": {...},
"required": [...]}}`.
- `content_block_start` for a tool call: `{"type":"content_block_start","index":N,"content_block":
{"type":"tool_use","id":"toolu_01...","name":"...","input":{}}}` — `input` starts empty.
- `content_block_delta`: `{"type":"content_block_delta","index":N,"delta":{"type":"input_json_delta",
"partial_json":"..."}}` — field name is exactly `partial_json`; concatenate across deltas for one
  `index`, then `serde_json::from_str` once `content_block_stop` closes that index.
- `message_delta` carries `stop_reason: "tool_use"` when the model stopped specifically to call a
  tool.
- Round-trip: send back the prior assistant message unchanged (including its `tool_use` blocks),
  followed by a `user` message with `content: [{"type":"tool_result","tool_use_id":"...",
"content":"...","is_error":true}]` (`is_error` optional, omit when false).

**Alternatives considered**: None — this is Anthropic's only tool-use wire format for the Messages
API; no version negotiation needed.

## 2. mistralrs 0.8.1 — local-model tool calling

**Decision**: Use mistralrs's native Rust-API tool-calling surface (not its OpenAI-compatible HTTP
server) directly from `LocalAdapter`. Confirmed by extracting and inspecting the published 0.8.1
crate sources (`mistralrs`, `mistralrs-core`, `mistralrs-mcp` — docs.rs's 0.8.1 build is broken and
falls back to 0.7.0 docs, and no `v0.8.1` git tag exists, so crates.io tarballs were the only
reliable source):

- `mistralrs_core::Tool { tp: ToolType, function: Function }`, `Function { description: Option<String>,
name: String, parameters: Option<HashMap<String, Value>> }` (JSON-Schema-shaped parameters).
- `ToolChoice::{None, Auto, Tool(Tool)}`.
- `RequestBuilder::set_tools(Vec<Tool>)`, `::set_tool_choice(ToolChoice)`,
  `::add_message_with_tool_call(role, content, Vec<ToolCallResponse>)`, `::add_tool_message(content,
tool_id)`.
- `Model::send_chat_request`/`stream_chat_request`: `ChatCompletionResponse.choices[0].message.
tool_calls: Option<Vec<ToolCallResponse>>`; the streamed `Delta` struct carries the same field, so
  streaming tool calls are supported, not just the non-streaming path.
- `ToolCallResponse { index, id, tp, function: CalledFunction { name, arguments } }` — `arguments` is
  a JSON _string_, parsed with `serde_json::from_str` (matches how `ToolSpec`/`ToolCall` in
  `adapters/types.rs` are already planned to carry parsed `serde_json::Value`).

**Rationale**: Confirmed end-to-end against mistralrs's own bundled `examples/advanced/tools/main.rs`.
This is a real Rust-API capability, not something only exposed through the HTTP server — the
existing in-process `LocalAdapter` pattern (no sidecar) is preserved.

**Caveat, carried into data-model.md and out of this feature's guaranteed scope**: whether a call
actually results in a tool call depends on the _loaded model's_ chat template and mistral.rs's
per-architecture tool-call parser (confirmed working for Llama 3.1/3.2, Hermes, Mistral
function-calling variants). A model without tool-calling support simply never emits `tool_calls` —
this is the mechanism by which "local models get tool support where the model supports it, and plain
text otherwise" falls out naturally, with no special-casing needed in `commands.rs`.

**Alternatives considered**: Driving mistralrs's bundled OpenAI-compatible HTTP server as a loopback
sidecar instead of the in-process Rust API. Rejected — reintroduces exactly the sidecar-process
pattern already rejected once for local inference (`docs/plans/2026-09-11-agent-tool-loop-design.md`
§9), for no benefit since the native Rust API already exposes everything needed.

## 3. MCP client library

**Decision**: `rmcp` (crates.io), the official Rust SDK for the Model Context Protocol
(github.com/modelcontextprotocol/rust-sdk). Current stable `3.3.0`, actively maintained (25.7M
downloads, last published 2026-09-10). Pin to an exact version per Constitution Principle IV, the
same way `haex-crdt` is pinned to a specific revision.

**Rationale**: This repo has zero existing MCP code — building a hand-rolled JSON-RPC-over-stdio
client would duplicate what the official SDK already provides (transport, capability negotiation,
`tools/list`/`tools/call`), for a protocol haex-vault does not control the other side of.

**Alternatives considered**: Hand-written minimal stdio JSON-RPC client. Rejected — MCP's
capability-negotiation handshake and message framing are enough surface area that reimplementing them
for a personal-agent app is exactly the kind of wheel-reinvention already rejected for the cli_delegate
research track (design doc §9).

## Summary of resolved Technical Context unknowns

| Unknown from plan.md                 | Resolution                                                            |
| ------------------------------------ | --------------------------------------------------------------------- |
| Anthropic tool-use wire format       | §1 above — implement against the quoted JSON shapes                   |
| mistralrs 0.8.1 tool-calling support | §2 above — native Rust API, `RequestBuilder`/`ChatCompletionResponse` |
| MCP client dependency                | §3 above — `rmcp` 3.3.0, pinned                                       |

No unresolved `NEEDS CLARIFICATION` markers remain in `plan.md`'s Technical Context.
