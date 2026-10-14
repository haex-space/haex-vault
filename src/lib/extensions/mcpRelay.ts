// The MCP messages of an extension's tools on its frame's port (spec 018, contracts/mcp-port.md):
// holzi's client in Rust and the extension's server talk JSON-RPC 2.0, wrapped as
// `{type: "haexspace:mcp", message}` so they stay apart from the bridge's requests. Pure:
// `composables/useExtensionFrame.ts` relays them, `scripts/check-extensions-mcp-relay.ts` tests it.

export const MCP_PORT_TYPE = 'haexspace:mcp'

/** A JSON-RPC 2.0 message; holzi passes it on unchanged. */
export type McpMessage = Record<string, unknown> & { jsonrpc: '2.0' }

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/** The JSON-RPC message of an MCP port message; `null` for anything else or a malformed one. */
export function readMcpMessage(data: unknown): McpMessage | null {
  if (!isObject(data) || data.type !== MCP_PORT_TYPE) return null
  const { message } = data
  if (!isObject(message) || message.jsonrpc !== '2.0') return null
  return message as McpMessage
}

/** The port message for a message from Rust. */
export function mcpPortMessage(message: unknown): {
  type: typeof MCP_PORT_TYPE
  message: unknown
} {
  return { type: MCP_PORT_TYPE, message }
}
