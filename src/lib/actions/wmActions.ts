// window manager action catalog (spec 020-tab-navigation, contracts/wm-actions.md §1). Pure data; the
// handlers are registered by `stores/wmActionHandlers.ts`. Descriptions are English because they
// become tool descriptions for agents (spec 021).
import type { JsonSchema, ActionDefinition } from './types.ts'

const TAB_ID: JsonSchema = {
  type: 'string',
  description:
    'Tab to act on. Required for agents; users default to the focused tab.',
}

const MOVED: JsonSchema = {
  type: 'object',
  properties: {
    moved: {
      type: 'boolean',
      description: 'Whether the tab changed location.',
    },
  },
}

/** Back/forward, history jumps and in-tab navigation (FR-004–FR-007, FR-017). */
export const WM_NAVIGATION_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'wm.tab.back',
    titleKey: 'actions.wm.tab.back',
    description: "Go back one entry in a tab's own history.",
    input: { type: 'object', properties: { tabId: TAB_ID } },
    result: MOVED,
    target: 'tab',
    scope: 'wm.navigation',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
    defaultKeys: {
      default: ['Alt+ArrowLeft'],
      mac: ['Alt+ArrowLeft', 'Meta+BracketLeft'],
    },
    yieldToTextInput: { mac: ['Alt+ArrowLeft'] },
  },
  {
    id: 'wm.tab.forward',
    titleKey: 'actions.wm.tab.forward',
    description: "Go forward one entry in a tab's own history.",
    input: { type: 'object', properties: { tabId: TAB_ID } },
    result: MOVED,
    target: 'tab',
    scope: 'wm.navigation',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
    defaultKeys: {
      default: ['Alt+ArrowRight'],
      mac: ['Alt+ArrowRight', 'Meta+BracketRight'],
    },
    yieldToTextInput: { mac: ['Alt+ArrowRight'] },
  },
  {
    id: 'wm.tab.go',
    titleKey: 'actions.wm.tab.go',
    description:
      "Jump several entries in a tab's history: negative steps go back, positive steps go forward.",
    input: {
      type: 'object',
      properties: {
        tabId: TAB_ID,
        steps: { type: 'integer', description: 'Signed number of entries.' },
      },
      required: ['steps'],
    },
    result: MOVED,
    target: 'tab',
    scope: 'wm.navigation',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
  },
  {
    id: 'wm.tab.navigate',
    titleKey: 'actions.wm.tab.navigate',
    description:
      "Navigate a tab to a location inside its app, e.g. '/thread/<id>'. replace=true changes the current entry instead of adding one.",
    input: {
      type: 'object',
      properties: {
        tabId: TAB_ID,
        to: {
          type: 'string',
          description:
            "App-relative location starting with '/', optionally with a query.",
        },
        replace: { type: 'boolean' },
      },
      required: ['to'],
    },
    result: MOVED,
    target: 'tab',
    scope: 'wm.navigation',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
  },
  {
    id: 'wm.system.back',
    titleKey: 'actions.wm.system.back',
    description:
      'Platform back (Android back gesture): close a window manager overlay, else go back in the top visible tab, else open the window overview in compact mode.',
    input: { type: 'object', properties: {} },
    result: {
      type: 'object',
      properties: {
        outcome: {
          type: 'string',
          enum: ['closeOverlay', 'back', 'openWindowOverview', 'none'],
        },
      },
    },
    target: 'none',
    scope: 'wm.navigation',
    effect: 'write',
    agentCallable: false,
    binding: 'global',
  },
]

const APP_ID: JsonSchema = {
  type: 'string',
  description:
    "The app to open: its name as the user said it, e.g. 'haex-mail' or 'Einstellungen', or its id from wm.apps.list, e.g. 'system.chat'. Never invent an id; an extension's id is 'extension.' followed by a UUID.",
}
const AT: JsonSchema = {
  type: 'string',
  description: "Optional app-relative start location, e.g. '/thread/<id>'.",
}
const OPENED: JsonSchema = {
  type: 'object',
  properties: {
    tabId: { type: 'string', description: 'The tab that shows the app.' },
    created: {
      type: 'boolean',
      description:
        'false when an already open single-instance app was activated instead.',
    },
  },
}

/** Opening apps and tabs at a location (FR-012). */
export const WM_OPEN_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'wm.app.open',
    titleKey: 'actions.wm.app.open',
    description:
      'Open an app in a new window, optionally at a location. A single-instance app that is already open is activated and navigated there instead.' +
      ' Apps of holzi only: a website or web address (like example.com) is no app and goes to browser.open.',
    input: {
      type: 'object',
      properties: { appId: APP_ID, at: AT },
      required: ['appId'],
    },
    result: OPENED,
    target: 'none',
    scope: 'wm.navigation',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
  },
  {
    id: 'wm.tab.new',
    titleKey: 'actions.wm.tab.new',
    description:
      'Open an app as a new tab in a window, optionally at a location. A single-instance app that is already open is activated instead.' +
      ' Apps of holzi only: a website or web address (like example.com) is no app and goes to browser.open.',
    input: {
      type: 'object',
      properties: {
        windowId: {
          type: 'string',
          description:
            'Window to add the tab to. Required for agents; users default to the focused window.',
        },
        appId: APP_ID,
        at: AT,
      },
      required: ['appId'],
    },
    result: OPENED,
    target: 'window',
    scope: 'wm.layout',
    effect: 'write',
    agentCallable: true,
    binding: 'global',
  },
]
