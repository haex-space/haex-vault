// Part of `pnpm check:extensions`: how the frame relays the MCP messages of an extension's tools
// between its port and Rust (src/lib/extensions/mcpRelay.ts, spec 018, contracts/mcp-port.md).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  mcpPortMessage,
  readMcpMessage,
} from '../src/lib/extensions/mcpRelay.ts'

const call = {
  jsonrpc: '2.0',
  id: 3,
  method: 'tools/call',
  params: { name: 'count_entries', arguments: {} },
}

test('an MCP port message hands its JSON-RPC message on unchanged', () => {
  assert.deepEqual(
    readMcpMessage({ type: 'haexspace:mcp', message: call }),
    call,
  )
})

test('a bridge request on the same port is no MCP message', () => {
  assert.equal(
    readMcpMessage({ id: 'r1', method: 'extension_context_get', params: null }),
    null,
  )
})

test('malformed MCP port messages are dropped', () => {
  for (const data of [
    null,
    'haexspace:mcp',
    { type: 'haexspace:mcp' },
    { type: 'haexspace:mcp', message: null },
    { type: 'haexspace:mcp', message: 'text' },
    { type: 'haexspace:mcp', message: [call] },
    { type: 'haexspace:mcp', message: { id: 1, method: 'ping' } },
  ])
    assert.equal(readMcpMessage(data), null, JSON.stringify(data))
})

test('a message from Rust goes to the port wrapped as an MCP port message', () => {
  assert.deepEqual(mcpPortMessage(call), {
    type: 'haexspace:mcp',
    message: call,
  })
})
