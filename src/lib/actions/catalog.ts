// The complete action catalog (spec 020-tab-navigation): every shipped action definition, in one
// list, for the runner, `wm.actions.list` and the catalog invariants in check-wm-actions.ts.
// The per-area catalogs (wm, chat, settings) are added here as their stories land.
import { WM_NAVIGATION_ACTIONS, WM_OPEN_ACTIONS } from './wmActions.ts'
import { BROWSER_ACTIONS } from './browserActions.ts'
import { AGENT_BROWSER_ACTIONS } from './settingsAgentBrowserActions.ts'
import { WM_LAYOUT_ACTIONS, WM_READ_ACTIONS } from './wmLayoutActions.ts'
import { CHAT_ACTIONS, CHAT_MODEL_ACTIONS } from './chatActions.ts'
import { CONTACTS_ACTIONS } from './contactsActions.ts'
import { FILE_SHARE_ACTIONS } from './fileShareActions.ts'
import { FILES_ACTIONS } from './filesActions.ts'
import { PASSWORDS_ACTIONS } from './passwordsActions.ts'
import { SETTINGS_ACTIONS } from './settingsActions.ts'
import { SYNC_RULE_ACTIONS } from './syncRuleActions.ts'
import { MCP_AGENT_ACTIONS } from './settingsMcpAgentActions.ts'
import type { ActionDefinition } from './types.ts'

export const ALL_ACTIONS: readonly ActionDefinition[] = [
  ...WM_NAVIGATION_ACTIONS,
  ...WM_OPEN_ACTIONS,
  ...WM_LAYOUT_ACTIONS,
  ...WM_READ_ACTIONS,
  ...CHAT_ACTIONS,
  ...CHAT_MODEL_ACTIONS,
  ...SETTINGS_ACTIONS,
  ...MCP_AGENT_ACTIONS,
  ...AGENT_BROWSER_ACTIONS,
  ...PASSWORDS_ACTIONS,
  ...FILES_ACTIONS,
  ...FILE_SHARE_ACTIONS,
  ...SYNC_RULE_ACTIONS,
  ...CONTACTS_ACTIONS,
  ...BROWSER_ACTIONS,
]
