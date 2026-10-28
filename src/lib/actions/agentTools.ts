// Actions as the built-in agent's tools (spec 032, data-model.md §2, ADR-0006). The catalog stays the
// single source of the definitions; this module maps an action to the tool description the Rust
// `ToolRegistry` keeps (`set_agent_actions`). Pure TS, relative `.ts` imports only, no i18n: titles
// come in through `titleOf` so the Node check scripts can load the module.
import { TARGET_FIELD } from './runner.ts'
import type {
  ActionDefinition,
  ActionOutcome,
  ActionTarget,
  ChoiceOption,
  JsonSchema,
} from './types.ts'

/** The catalog languages every action title is pushed in (`AgentActionDef.titles`). */
export const ACTION_LOCALES = ['de', 'en'] as const

export type ActionLocale = (typeof ACTION_LOCALES)[number]

/** What the frontend pushes to Rust for one action (data-model.md §2). */
export type AgentActionDef = {
  /** Action id with `.` replaced by `_`; matches `TOOL_NAME_PATTERN` and is unique. */
  toolName: string
  actionId: string
  /** The action's own English description, unchanged (FR-001). */
  description: string
  inputSchema: JsonSchema
  effect: ActionDefinition['effect']
  /** Part of the fixed core offer every model sees (research R6). */
  core: boolean
  /** Localized titles, searched by `find_actions` together with id and description. */
  titles: Record<ActionLocale, string>
  /** `native`: Rust runs it itself (ADR 0011). */
  runner?: 'native'
  /** The action's own dialog is the question; the chat does not ask again (spec 060 FR-029). */
  confirmsItself?: boolean
}

export type TitleOf = (titleKey: string, locale: ActionLocale) => string

/** Anthropic only accepts tool names of this shape (other providers are looser). */
export const TOOL_NAME_PATTERN = /^[A-Za-z0-9_-]{1,64}$/

/**
 * The fixed core offer: the same tools in every step for every model, local or cloud, chosen by
 * how often people ask for them, never from the text of a message (research R6). Together with
 * `find_actions` and `ask_user` (spec 046) this stays at or under 11 tools. `wm.apps.list` stays in
 * the core: a small model that had to find it first answered "which extensions are installed"
 * without calling it (spec 046, R16).
 */
export const CORE_AGENT_TOOLS: readonly string[] = [
  'wm.state.get',
  'wm.apps.list',
  'wm.app.open',
  'wm.tab.new',
  'wm.tab.activate',
  'wm.tab.close',
  'settings.get',
  'settings.appearance.setColorScheme',
  'settings.models.list',
]

/** `wm.tab.back` → `wm_tab_back`. Ids use camelCase and no underscores, so this is reversible. */
export function toToolName(actionId: string): string {
  return actionId.replaceAll('.', '_')
}

/** Whether the built-in agent may be offered this action at all. */
export function isBuiltinAgentAction(def: ActionDefinition): boolean {
  return def.agentCallable && def.builtinAgentCallable !== false
}

/** Reverse lookup of `toToolName` over a catalog; `undefined` for names that are no action. */
export function fromToolName(
  toolName: string,
  catalog: readonly ActionDefinition[],
): ActionDefinition | undefined {
  return catalog.find((def) => toToolName(def.id) === toolName)
}

/** Maps a catalog action to its tool definition, including core membership and both titles. */
export function toAgentActionDef(
  def: ActionDefinition,
  titleOf: TitleOf,
): AgentActionDef {
  return {
    toolName: toToolName(def.id),
    actionId: def.id,
    description: def.description,
    inputSchema: def.input,
    effect: def.effect,
    core: CORE_AGENT_TOOLS.includes(def.id),
    titles: {
      de: titleOf(def.titleKey, 'de'),
      en: titleOf(def.titleKey, 'en'),
    },
    ...(def.runner ? { runner: def.runner } : {}),
    ...(def.confirmsItself ? { confirmsItself: true } : {}),
  }
}

/**
 * What the frontend pushes to Rust (spec 021 research R4): every action any agent may call, with
 * its scope and whether the built-in agent may call it. Rust registers the built-in agent's share
 * as chat tools and offers external agents from the whole list, by their rights per scope.
 */
export type PushedActionDef = AgentActionDef & {
  scope: ActionDefinition['scope']
  builtinAgentCallable: boolean
}

/** The definitions to push with `set_agent_actions`; guardrails are never among them. */
export function listPushedActions(
  catalog: readonly ActionDefinition[],
  titleOf: TitleOf,
): PushedActionDef[] {
  return catalog
    .filter((def) => def.agentCallable && def.scope !== 'guardrails')
    .map((def) => ({
      ...toAgentActionDef(def, titleOf),
      scope: def.scope,
      builtinAgentCallable: def.builtinAgentCallable !== false,
    }))
}

/** What the built-in agent may call of the catalog (FR-001, FR-003); the eval snapshot holds it. */
export function listAgentActions(
  catalog: readonly ActionDefinition[],
  titleOf: TitleOf,
): AgentActionDef[] {
  return catalog
    .filter(isBuiltinAgentAction)
    .map((def) => toAgentActionDef(def, titleOf))
}

/** What travels back to Rust after an action ran (`respond_action_call`, contracts/tauri-commands.md). */
export type ActionOutcomeWire =
  | { ok: true; result: unknown }
  | {
      ok: false
      code: string
      field?: string
      message: string
      options?: ChoiceOption[]
    }

/**
 * The runner's outcome without the raw error a handler threw: that is for in-process callers and
 * never for a model (FR-006). Rust replaces the message of `failed` by a fixed text.
 */
export function toOutcomeWire(outcome: ActionOutcome): ActionOutcomeWire {
  if (outcome.ok) return { ok: true, result: outcome.result }
  return {
    ok: false,
    code: outcome.code,
    ...(outcome.field === undefined ? {} : { field: outcome.field }),
    message: outcome.message,
    ...(outcome.options === undefined ? {} : { options: outcome.options }),
  }
}

/** A message as `chat.messages.list` shows it to an agent: no text (FR-010). */
export type AgentMessage = {
  id: string
  role: string
  createdAt: number
  status: string | null
}

/**
 * The safe projection of a conversation for an agent: who spoke, when, and how the turn ended.
 * The text can hold anything the user pasted, so it stays out of what a model or an agent reads.
 */
export function agentSafeMessages(
  messages: readonly {
    id: string
    role: string
    createdAt: number
    finishReason: string | null
  }[],
): { messages: AgentMessage[] } {
  return {
    messages: messages.map((m) => ({
      id: m.id,
      role: m.role,
      createdAt: m.createdAt,
      status: m.finishReason,
    })),
  }
}

export type TargetKind = Exclude<ActionTarget, 'none'>

/** What the approval dialog shows for one call (spec 032 FR-008). */
export type ActionDescription = {
  titleKey: string
  target?: { kind: TargetKind; label: string }
  /** `[field, value]` pairs without the target id, values as short text. */
  inputs: Array<[string, string]>
}

/** Names the tab, window or workspace behind an id; `undefined` when it does not exist. */
export type ResolveTarget = (kind: TargetKind, id: string) => string | undefined

const MAX_INPUT_VALUE_LENGTH = 200

function inputText(value: unknown): string {
  const text = typeof value === 'string' ? value : JSON.stringify(value)
  return text.length > MAX_INPUT_VALUE_LENGTH
    ? `${text.slice(0, MAX_INPUT_VALUE_LENGTH)}…`
    : text
}

/** The call of one action in terms a person reads: its title, what it acts on, what it carries. */
export function describeAction(
  def: ActionDefinition,
  input: Record<string, unknown>,
  resolveTarget: ResolveTarget,
): ActionDescription {
  const targetField =
    def.target === 'none' ? undefined : TARGET_FIELD[def.target]
  const targetId = targetField ? input[targetField] : undefined
  const target =
    def.target !== 'none' && typeof targetId === 'string'
      ? {
          kind: def.target,
          label: resolveTarget(def.target, targetId) ?? targetId,
        }
      : undefined
  return {
    titleKey: def.titleKey,
    ...(target ? { target } : {}),
    inputs: Object.entries(input)
      .filter(([field]) => field !== targetField)
      .map(([field, value]) => [field, inputText(value)]),
  }
}

/** `describeAction` for a tool call; `undefined` for a name that is no action offered to the agent. */
export function describeToolCall(
  toolName: string,
  input: unknown,
  catalog: readonly ActionDefinition[],
  resolveTarget: ResolveTarget,
): ActionDescription | undefined {
  const def = fromToolName(toolName, catalog)
  if (!def || !isBuiltinAgentAction(def)) return undefined
  const fields =
    typeof input === 'object' && input !== null && !Array.isArray(input)
      ? (input as Record<string, unknown>)
      : {}
  return describeAction(def, fields, resolveTarget)
}
