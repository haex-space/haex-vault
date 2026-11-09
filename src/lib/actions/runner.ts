// The `runAction` pipeline (spec 020-tab-navigation, T013, data-model.md "Ablauf runAction"):
// look up → guardrail lock for agents → validate input → resolve target → wait for a tab-bound
// handler if needed → run. Pure: the store injects focus, target lookup and handler registries.
import { validate } from './schema.ts'
import type {
  ActionCaller,
  ActionErrorCode,
  ChoiceOption,
  ActionOutcome,
  ActionTarget,
  ActionTargetIds,
  ActionDefinition,
} from './types.ts'

export type ActionHandlerContext = {
  input: Record<string, unknown>
  caller: ActionCaller
  target: ActionTargetIds
}

export type ActionHandler = (context: ActionHandlerContext) => unknown

export type ActionRunnerDeps = {
  catalog: readonly ActionDefinition[]
  globalHandler: (actionId: string) => ActionHandler | undefined
  /** The id the UI focus implies for this target kind, or `null` if there is none. */
  resolveFocus: (target: Exclude<ActionTarget, 'none'>) => string | null
  targetExists: (target: Exclude<ActionTarget, 'none'>, id: string) => boolean
  /** Opens/activates `appId` and resolves its tab-bound handler, or `null` after the timeout. */
  awaitTabHandler: (
    appId: string,
    actionId: string,
    timeoutMs: number,
    originTabId?: string,
  ) => Promise<ActionHandler | null>
}

export const TAB_HANDLER_TIMEOUT_MS = 5000

/** The input field that names the target of an action, per target kind. */
export const TARGET_FIELD = {
  tab: 'tabId',
  window: 'windowId',
  workspace: 'workspaceId',
} as const

type FailureCode = Exclude<ActionErrorCode, 'needs_choice'>

/** What an agent should do after each error (spec 082 FR-014, research R7): one sentence beside
 * the message, which stays as it is for other callers. */
export function nextStep(code: FailureCode, field?: string): string {
  const named = field === undefined ? 'the input' : `\`${field}\``
  switch (code) {
    case 'unknown_action':
      return 'Call only actions the offer or find_actions named.'
    case 'invalid_input':
      return `Correct ${named} as the message says and call again.`
    case 'target_required':
      return `Name it in ${named}; wm_state_get lists what is open.`
    case 'target_not_found':
      return `${named} names nothing open: read the open ones with wm_state_get and call again.`
    case 'forbidden_for_agents':
      return 'Do not call it again; tell the user they can do it themselves.'
    case 'app_unavailable':
      return 'Tell the user the app was not ready in time; do not repeat the call now.'
    case 'failed':
      return 'Tell the user it did not work; do not repeat the same call.'
  }
}

function failure(
  code: FailureCode,
  message: string,
  field?: string,
): ActionOutcome {
  const next = nextStep(code, field)
  return field === undefined
    ? { ok: false, code, message, next }
    : { ok: false, code, message, field, next }
}

/** Thrown by a handler whose input passed the schema but names nothing that exists (an unknown app
 * id). Becomes `invalid_input`, so an agent sees the message and can correct its call instead of
 * getting the masked `failed`. */
export class ActionInputError extends Error {
  readonly field: string | undefined

  constructor(message: string, field?: string) {
    super(message)
    this.name = 'ActionInputError'
    this.field = field
  }
}

/** Thrown by a handler whose input fits several things, or none well enough (an ambiguous app
 * name, spec 046). Becomes `needs_choice` with the candidates; the built-in agent's tool round then
 * asks the user and runs the action again with the chosen value in `field`. */
export class ActionChoiceError extends Error {
  readonly field: string
  readonly options: ChoiceOption[]

  constructor(message: string, field: string, options: ChoiceOption[]) {
    super(message)
    this.name = 'ActionChoiceError'
    this.field = field
    this.options = options
  }
}

/** A readable message for any thrown value: `Error.message`, a backend error's `reason` or
 * `message`, else its JSON. */
function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  if (typeof error === 'object' && error !== null) {
    const record = error as Record<string, unknown>
    for (const key of ['reason', 'message']) {
      if (typeof record[key] === 'string') return record[key] as string
    }
    try {
      return JSON.stringify(error)
    } catch {
      return String(error)
    }
  }
  return String(error)
}

export function createActionRunner(deps: ActionRunnerDeps) {
  async function runAction(
    id: string,
    input: Record<string, unknown> = {},
    caller: ActionCaller = { kind: 'user' },
    originTabId?: string,
  ): Promise<ActionOutcome> {
    const action = deps.catalog.find((a) => a.id === id)
    if (!action) return failure('unknown_action', `unknown action ${id}`)

    if (caller.kind !== 'user' && !action.agentCallable)
      return failure(
        'forbidden_for_agents',
        `${id} can only be triggered by the user`,
      )

    if (caller.kind === 'builtinAgent' && action.builtinAgentCallable === false)
      return failure(
        'forbidden_for_agents',
        `${id} is not available to the built-in agent`,
      )

    const validation = validate(action.input, input)
    if (!validation.ok)
      return failure('invalid_input', validation.message, validation.field)

    const target: ActionTargetIds = {}
    if (action.target !== 'none') {
      const field = TARGET_FIELD[action.target]
      const explicit = input[field]
      if (typeof explicit === 'string') {
        if (!deps.targetExists(action.target, explicit))
          return failure(
            'target_not_found',
            `no ${action.target} ${explicit}`,
            field,
          )
        target[field] = explicit
      } else if (caller.kind === 'user') {
        const focused = deps.resolveFocus(action.target)
        if (focused === null)
          return failure(
            'target_required',
            `no focused ${action.target}`,
            field,
          )
        target[field] = focused
      } else {
        return failure(
          'target_required',
          `agents must name the ${action.target}`,
          field,
        )
      }
    }

    let handler: ActionHandler | null | undefined
    if (action.binding === 'tab') {
      handler = await deps.awaitTabHandler(
        action.appId ?? '',
        id,
        TAB_HANDLER_TIMEOUT_MS,
        originTabId,
      )
      if (!handler)
        return failure(
          'app_unavailable',
          `${action.appId ?? 'app'} did not become ready`,
        )
    } else {
      handler = deps.globalHandler(id)
      if (!handler) return failure('failed', `no handler registered for ${id}`)
    }

    try {
      const result = await handler({ input, caller, target })
      const normalized = result ?? null
      const validation = validate(action.result, normalized)
      if (!validation.ok)
        return failure('failed', validation.message, validation.field)
      return { ok: true, result: normalized }
    } catch (error) {
      if (error instanceof ActionInputError)
        return failure('invalid_input', error.message, error.field)
      if (error instanceof ActionChoiceError)
        return {
          ok: false,
          code: 'needs_choice',
          message: error.message,
          field: error.field,
          options: error.options,
        }
      return {
        ok: false,
        code: 'failed',
        message: errorMessage(error),
        next: nextStep('failed'),
        error,
      }
    }
  }

  return { runAction }
}
