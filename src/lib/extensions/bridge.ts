// The relay between an extension frame's port and Rust (spec 017, research R14,
// contracts/bridge.md §Weiterleitung). Pure: the frame component moves the messages,
// `scripts/check-extensions-bridge.ts` tests them. The relay decides nothing; Rust checks every
// request (ADR-0004).

export type BridgeRequest = {
  id: string | number
  method: string
  params: unknown
}

/** `extension-frame-event` as Rust sends it, already for one frame. */
export type FrameEvent = {
  frame: string
  type: string
  data: unknown
  timestamp: number
}

/** A request on the port, or `null` for anything else (the SDK's `port:ready`, garbage). */
export function readRequest(data: unknown): BridgeRequest | null {
  if (typeof data !== 'object' || data === null) return null
  const { id, method, params } = data as Record<string, unknown>
  if (typeof id !== 'string' && typeof id !== 'number') return null
  if (typeof method !== 'string' || method.length === 0) return null
  return { id, method, params: params ?? null }
}

const MAX_DEPTH = 64

function toBase64(bytes: Uint8Array): string {
  let binary = ''
  const chunk = 0x8000
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk))
  }
  return btoa(binary)
}

/**
 * Makes `params` JSON for the Tauri command: `Uint8Array` and `ArrayBuffer` become
 * `{"$bytes": "<base64>"}` (research R7), arrays and plain objects are walked, everything else
 * stays. Deeper than 64 levels becomes `null`; Rust refuses such input anyway.
 */
export function encodeBytes(value: unknown, depth = 0): unknown {
  if (depth > MAX_DEPTH) return null
  if (value instanceof Uint8Array) return { $bytes: toBase64(value) }
  if (value instanceof ArrayBuffer)
    return { $bytes: toBase64(new Uint8Array(value)) }
  if (Array.isArray(value))
    return value.map((item) => encodeBytes(item, depth + 1))
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [
        key,
        encodeBytes(item, depth + 1),
      ]),
    )
  }
  return value
}

/** Events the SDK reads flat, with their fields next to `type` (contracts/bridge.md §Meldungen). */
const FLAT_EVENTS = new Set([
  'filesync:file-changed',
  'shell:output',
  'shell:exit',
])

/**
 * The message on the port for an event: `{type, data, timestamp}` as the SDK reads it; a flat
 * event also carries the fields of its data next to `type`.
 */
export function eventMessage(event: FrameEvent): {
  type: string
  data: unknown
  timestamp: number
  [field: string]: unknown
} {
  const message = {
    type: event.type,
    data: event.data,
    timestamp: event.timestamp,
  }
  if (
    !FLAT_EVENTS.has(event.type) ||
    typeof event.data !== 'object' ||
    event.data === null
  )
    return message
  return { ...(event.data as Record<string, unknown>), ...message }
}

/** Events held for a frame whose SDK is not ready yet (8 KiB of shell output each at most). */
export const MAX_HELD_EVENTS = 256

/** A `peers:message` of the channel's reliable stream; losing one ends the channel (FR-038). */
function reliableChannel(event: FrameEvent): string | null {
  if (event.type !== 'peers:message') return null
  const data = event.data as { channelId?: unknown; reliable?: unknown } | null
  return data?.reliable === true && typeof data.channelId === 'string'
    ? data.channelId
    : null
}

/** Whether the queue may drop the event first: shell output and fast `peers:message`. */
function droppable(event: FrameEvent): boolean {
  if (event.type === 'shell:output') return true
  if (event.type !== 'peers:message') return false
  return (event.data as { reliable?: unknown } | null)?.reliable === false
}

/**
 * Events for one frame. Until the SDK confirms its port (`haexspace:port:ready`) they are held
 * (FR-042); a new port (the frame loaded again) starts holding again. At most
 * {@link MAX_HELD_EVENTS} are held: beyond that the oldest droppable event goes (`shell:output`,
 * a fast `peers:message`; holzi does not wait for a frame that never acknowledged, so a frame
 * that never gets ready would hold a flooding shell's whole output); only when none is held, the
 * oldest event of any type goes. Losing a reliable `peers:message` is reported once per channel
 * through `onLost`, so holzi ends that channel instead of delivering it with a gap (spec 057
 * R8).
 */
export class FrameEventQueue {
  private held: FrameEvent[] = []
  private open = false
  private readonly frame: string
  private readonly deliver: (event: FrameEvent) => void
  private readonly onLost: (channelId: string) => void
  private readonly reported = new Set<string>()

  constructor(
    frame: string,
    deliver: (event: FrameEvent) => void,
    onLost: (channelId: string) => void = () => {},
  ) {
    this.frame = frame
    this.deliver = deliver
    this.onLost = onLost
  }

  /** Takes an event of any frame; events of other frames are dropped. */
  push(event: FrameEvent): void {
    if (event.frame !== this.frame) return
    if (this.open) {
      this.deliver(event)
      return
    }
    if (this.held.length >= MAX_HELD_EVENTS) this.dropOne()
    this.held.push(event)
  }

  private dropOne(): void {
    const first = this.held.findIndex(droppable)
    const dropped = this.held.splice(first === -1 ? 0 : first, 1)[0]
    if (!dropped || droppable(dropped)) return
    console.warn(
      `extension frame ${this.frame}: not ready, dropped ${dropped.type}`,
    )
    const channel = reliableChannel(dropped)
    if (channel !== null && !this.reported.has(channel)) {
      this.reported.add(channel)
      this.onLost(channel)
    }
  }

  /** The port is confirmed: everything held goes out in order. */
  ready(): void {
    this.open = true
    const held = this.held
    this.held = []
    for (const event of held) this.deliver(event)
  }

  /** The frame loads again; hold until its new port is confirmed. */
  reset(): void {
    this.open = false
  }
}
