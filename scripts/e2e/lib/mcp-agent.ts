// A stand-in for Claude Code or Codex (spec 021, T055): the application binary itself, started with
// `--mcp-agent <slug>` the way an agent starts it, with a minimal MCP client on its stdin and
// stdout. It runs in the environment of an instance, so it finds that instance's endpoint and keeps
// its credential in the instance's configuration folder. A scenario starts it, talks to it and stops
// it; it never outlives its scenario.
import { spawn } from 'node:child_process'
import { createWriteStream, mkdirSync } from 'node:fs'
import { dirname } from 'node:path'
import { createInterface } from 'node:readline'
import { stopGroup } from './processes.ts'

/** A JSON-RPC message from the server. */
type Message = {
  id?: number
  method?: string
  result?: unknown
  error?: { code: number; message: string }
} & Record<string, unknown>

/** The result of `tools/call` (contracts/mcp-surface.md, "Ergebnisse"). */
export interface ToolResult {
  isError?: boolean
  structuredContent?: Record<string, unknown>
}

export interface McpAgent {
  /** Sends `initialize` and `notifications/initialized`, as every MCP client does first. */
  initialize(): Promise<void>
  request<T>(method: string, params?: Record<string, unknown>): Promise<T>
  callTool(name: string, args?: Record<string, unknown>): Promise<ToolResult>
  /** Resolves when the server sent `method` (counting from the start). */
  notified(method: string): Promise<void>
  exited(): boolean
  /** Ends the process; safe to call twice. */
  stop(): Promise<void>
}

export function startMcpAgent(options: {
  app: string
  slug: string
  env: Record<string, string>
  logFile: string
}): McpAgent {
  mkdirSync(dirname(options.logFile), { recursive: true })
  const log = createWriteStream(options.logFile, { flags: 'a' })
  log.on('error', () => {
    // A log that cannot be written must not take the run down with it.
  })
  const child = spawn(options.app, ['--mcp-agent', options.slug], {
    env: options.env,
    detached: true,
    stdio: ['pipe', 'pipe', 'pipe'],
  })
  const write = (stream: string, line: string) =>
    log.write(`${new Date().toISOString()} ${stream} ${line}\n`)
  child.stderr?.on('data', (chunk: Buffer) =>
    write('stderr', chunk.toString().trimEnd()),
  )

  let nextId = 1
  const waiting = new Map<number, (message: Message) => void>()
  const seen: string[] = []
  const listeners: Array<() => void> = []
  createInterface({ input: child.stdout! }).on('line', (line) => {
    write('stdout', line)
    let message: Message
    try {
      message = JSON.parse(line) as Message
    } catch {
      return
    }
    if (message.id !== undefined && waiting.has(message.id)) {
      waiting.get(message.id)?.(message)
      waiting.delete(message.id)
    } else if (message.method) {
      seen.push(message.method)
      for (const listener of listeners.splice(0)) listener()
    }
  })

  const send = (message: Record<string, unknown>) => {
    const line = JSON.stringify({ jsonrpc: '2.0', ...message })
    write('stdin', line)
    child.stdin?.write(`${line}\n`)
  }

  const request = <T>(method: string, params: Record<string, unknown> = {}) =>
    new Promise<T>((resolve, reject) => {
      const id = nextId++
      waiting.set(id, (message) =>
        message.error
          ? reject(new Error(`${method}: ${message.error.message}`))
          : resolve(message.result as T),
      )
      send({ id, method, params })
    })

  const notified = (method: string) =>
    new Promise<void>((resolve) => {
      const check = () => {
        if (seen.includes(method)) resolve()
        else listeners.push(check)
      }
      check()
    })

  return {
    async initialize() {
      await request('initialize', {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: { name: 'e2e-agent', version: '1.0.0' },
      })
      send({ method: 'notifications/initialized' })
    },
    request,
    callTool: (name, args = {}) =>
      request<ToolResult>('tools/call', { name, arguments: args }),
    notified,
    exited: () => child.exitCode !== null,
    async stop() {
      child.stdin?.end()
      if (child.exitCode === null && child.pid !== undefined)
        await stopGroup(child.pid)
      log.end()
    },
  }
}
