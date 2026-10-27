import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  ACTION_LOCALES,
  listPushedActions,
  toOutcomeWire,
  type ActionLocale,
} from '~/lib/actions/agentTools'
import { ALL_ACTIONS } from '~/lib/actions/catalog'
import type { ActionCaller, ActionOutcome } from '~/lib/actions/types'

/**
 * `action-call-request` (contracts/tauri-commands.md): a model in the chat or an external agent
 * (spec 021) asks for an action. Rust names the caller; it never comes from the call's input.
 */
interface ActionCallRequest {
  requestId: string
  actionId: string
  input: Record<string, unknown>
  caller?: ActionCaller
}

/** The caller Rust named; an agent caller only, never the user. */
function agentCaller(caller: ActionCaller | undefined): ActionCaller {
  return caller?.kind === 'externalAgent'
    ? { kind: 'externalAgent', agentId: caller.agentId }
    : { kind: 'builtinAgent' }
}

let listener: Promise<void> | null = null
let startup: Promise<void> | null = null
let localeWatchStarted = false

/**
 * The agents' access to the actions (spec 032, spec 021, ADR-0006). Pushes the definitions any agent
 * may call to Rust (`set_agent_actions`); Rust gives the built-in agent its share and offers the rest
 * to registered external agents by their rights. Runs the calls Rust sends back
 * (`action-call-request`) through the same runner a click uses, with the caller Rust named.
 *
 * Call `startAsync()` once per vault session, from the workspace page: both Tauri commands need
 * an open vault, and the listener stays for the session like the sync listener. Definitions are
 * pushed again when the language changes, because they carry localized titles for the search.
 */
export function useAgentActions() {
  const wm = useWindowManagerStore()
  const { t, locale, loadLocaleMessages } = useI18n()

  /** Translates an action title in the requested catalog language. */
  const titleOf = (key: string, language: ActionLocale) =>
    t(key, {}, { locale: language })

  /** Replaces Rust's action tools with the eligible catalog and its localized titles. */
  async function pushDefinitionsAsync(): Promise<void> {
    // Only the active language is loaded lazily; the titles need every catalog language.
    await Promise.all(ACTION_LOCALES.map((code) => loadLocaleMessages(code)))
    await invoke('set_agent_actions', {
      args: { actions: listPushedActions(ALL_ACTIONS, titleOf) },
    })
  }

  /** Runs a request as its agent and sends its outcome without raw errors to Rust. */
  async function runCallAsync(request: ActionCallRequest): Promise<void> {
    let outcome: ActionOutcome
    try {
      outcome = await wm.runAction(
        request.actionId,
        request.input,
        agentCaller(request.caller),
      )
    } catch (error) {
      console.error('[agent] running an action failed', error)
      outcome = {
        ok: false,
        code: 'failed',
        message: 'The action failed.',
      }
    }
    await invoke('respond_action_call', {
      args: { requestId: request.requestId, outcome: toOutcomeWire(outcome) },
    })
  }

  /** Installs the listener once, publishes the catalog, and refreshes it on language changes. */
  function startAsync(): Promise<void> {
    if (startup) return startup

    const pending = (async () => {
      listener ??= listen<ActionCallRequest>('action-call-request', (event) => {
        void runCallAsync(event.payload).catch((error: unknown) => {
          console.error('[agent] answering an action call failed', error)
        })
      }).then(
        () => {},
        (error: unknown) => {
          listener = null
          throw error
        },
      )
      await listener
      await pushDefinitionsAsync()
      if (!localeWatchStarted) {
        localeWatchStarted = true
        watch(locale, () => {
          void pushDefinitionsAsync().catch((error: unknown) => {
            console.error(
              '[agent] updating the action definitions failed',
              error,
            )
          })
        })
      }
    })()

    startup = pending
    void pending.catch(() => {
      if (startup === pending) startup = null
    })
    return pending
  }

  return { startAsync }
}
