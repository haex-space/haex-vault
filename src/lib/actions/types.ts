// Action types (spec 020-tab-navigation, T011, data-model.md). Every state-changing control of the
// window manager and its apps is an action: the UI, keyboard shortcuts and — from spec 021 on — agents call
// the same definitions. "Action", never "command": in holzi "command" means Tauri commands
// (research R8). Pure types, relative sibling imports only.
import type { ActionScopeId } from './scopes.ts'

/** The JSON-schema subset actions use (research R8) — the same format the built-in agent's
 * `ToolRegistry` and MCP `tools/list` expect. Objects with declared properties are strict;
 * an object without `properties` accepts any object shape. */
export type JsonSchema = {
  type: 'object' | 'string' | 'number' | 'integer' | 'boolean' | 'array'
  description?: string
  properties?: Record<string, JsonSchema>
  required?: readonly string[]
  items?: JsonSchema
  enum?: readonly (string | number)[]
  nullable?: boolean
}

/** What an action acts on. For anything but `none`, the input schema declares the matching id
 * field (`tabId`, `windowId`, `workspaceId`): optional for a user (focus fills it in), mandatory for
 * agents (FR-030). */
export type ActionTarget = 'none' | 'tab' | 'window' | 'workspace'

export type ActionEffect = 'read' | 'write' | 'destructive'

/** `KeyboardEvent.code`-based chord in fixed modifier order: `Ctrl+Alt+Shift+Meta+<code>`. */
export type KeyChord = string

export type ActionDefinition = {
  id: string
  titleKey: string
  /** English, for machine callers (it becomes the tool description in spec 021). */
  description: string
  input: JsonSchema
  result: JsonSchema
  target: ActionTarget
  scope: ActionScopeId
  effect: ActionEffect
  /** `false` for every `guardrails` action (FR-032). */
  agentCallable: boolean
  /** `false`: external agents (spec 021) may call the action, the built-in agent may not, because
   * it would re-enter the running chat turn (spec 032, ADR-0006). Absent means `true`. */
  builtinAgentCallable?: boolean
  /** `native`: runs in Rust without a window (ADR 0011); the frontend registers no handler, the
   * runner of the window never calls it. Absent: a handler of the window runs it. */
  runner?: 'native'
  /** `global`: handler registered at startup; `tab`: registered by the mounted app instance
   * (research R19), and the runner opens `appId` first if needed. */
  binding: 'global' | 'tab'
  appId?: string
  defaultKeys?: { default?: readonly KeyChord[]; mac?: readonly KeyChord[] }
  /** Chords that are not intercepted while an editable element has focus, per platform (on macOS
   * Alt+Arrow jumps words in text, FR-017). */
  yieldToTextInput?: {
    default?: readonly KeyChord[]
    mac?: readonly KeyChord[]
  }
}

export type ActionCaller =
  | { kind: 'user' }
  | { kind: 'builtinAgent' }
  | { kind: 'externalAgent'; agentId: string }

export type ActionErrorCode =
  | 'unknown_action'
  | 'invalid_input'
  | 'target_required'
  | 'target_not_found'
  | 'forbidden_for_agents'
  | 'app_unavailable'
  | 'needs_choice'
  | 'failed'

/** A candidate of a `needs_choice` outcome (spec 046): what the user sees and what the action is
 * run with again when the user picks it. */
export type ChoiceOption = {
  value: string
  label: string
  /** Why it cannot be picked right now; the choice shows it disabled. */
  unavailable?: string
}

export type ActionOutcome =
  | { ok: true; result: unknown }
  | {
      ok: false
      code: ActionErrorCode
      message: string
      field?: string
      /** The candidates of a `needs_choice` outcome. */
      options?: ChoiceOption[]
      /** The raw error a handler threw (`failed` only), for in-process callers that parse
       * structured backend errors; never meant for display or for agents. */
      error?: unknown
    }

export type ActionTargetIds = {
  tabId?: string
  windowId?: string
  workspaceId?: string
}
