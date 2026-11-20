// window manager layout and read actions (spec 020-tab-navigation, T047, contracts/wm-actions.md §1).
// Pure data; handlers in `stores/wmLayoutHandlers.ts`. Split from wmActions.ts to keep each
// file below the 500-line limit.
import type { JsonSchema, ActionDefinition } from './types.ts'

const TAB_ID: JsonSchema = {
  type: 'string',
  description:
    'Tab to act on. Required for agents; users default to the focused tab.',
}
const WINDOW_ID: JsonSchema = {
  type: 'string',
  description:
    'Window to act on. Required for agents; users default to the focused window.',
}
const WORKSPACE_ID: JsonSchema = {
  type: 'string',
  description:
    'Workspace to act on. Required for agents; users default to the active workspace.',
}
const DONE: JsonSchema = {
  type: 'object',
  properties: { done: { type: 'boolean' } },
}
const ANY_OBJECT: JsonSchema = { type: 'object' }

type Layout = Pick<
  ActionDefinition,
  'id' | 'description' | 'input' | 'target' | 'effect'
> &
  Partial<Pick<ActionDefinition, 'result'>>

function layout(action: Layout): ActionDefinition {
  return {
    titleKey: `actions.${action.id}`,
    result: DONE,
    scope: 'wm.layout',
    agentCallable: true,
    binding: 'global',
    ...action,
  }
}

function read(
  action: Pick<ActionDefinition, 'id' | 'description' | 'input' | 'target'>,
): ActionDefinition {
  return {
    titleKey: `actions.${action.id}`,
    result: ANY_OBJECT,
    scope: 'wm.read',
    effect: 'read',
    agentCallable: true,
    binding: 'global',
    ...action,
  }
}

/** Arranging windows, tabs and workspaces (FR-024). Closing and deleting run the same user
 * confirmations as the UI (015 FR-014/FR-021); a declined confirmation fails the action. */
export const WM_LAYOUT_ACTIONS: readonly ActionDefinition[] = [
  layout({
    id: 'wm.tab.activate',
    description:
      'Make a tab the active tab of its window and focus the window.',
    input: { type: 'object', properties: { tabId: TAB_ID } },
    target: 'tab',
    effect: 'write',
  }),
  layout({
    id: 'wm.tab.close',
    description:
      'Close a tab (and its window if it was the last tab). The user confirms if a reply is running or an approval is pending.',
    input: { type: 'object', properties: { tabId: TAB_ID } },
    target: 'tab',
    effect: 'destructive',
  }),
  layout({
    id: 'wm.window.focus',
    description: 'Restore a window if minimized and bring it to the front.',
    input: { type: 'object', properties: { windowId: WINDOW_ID } },
    target: 'window',
    effect: 'write',
  }),
  layout({
    id: 'wm.window.minimize',
    description: 'Minimize a window.',
    input: { type: 'object', properties: { windowId: WINDOW_ID } },
    target: 'window',
    effect: 'write',
  }),
  layout({
    id: 'wm.window.toggleMaximize',
    description: 'Maximize a window to the workspace area, or restore it.',
    input: { type: 'object', properties: { windowId: WINDOW_ID } },
    target: 'window',
    effect: 'write',
  }),
  layout({
    id: 'wm.window.setGeometry',
    description:
      'Move and resize a window (CSS pixels within the workspace area); the window manager keeps it reachable and above its minimum size.',
    input: {
      type: 'object',
      properties: {
        windowId: WINDOW_ID,
        x: { type: 'number' },
        y: { type: 'number' },
        width: { type: 'number' },
        height: { type: 'number' },
      },
      required: ['x', 'y', 'width', 'height'],
    },
    target: 'window',
    effect: 'write',
  }),
  layout({
    id: 'wm.window.close',
    description:
      'Close a window with all its tabs. The user confirms if a reply is running or an approval is pending.',
    input: { type: 'object', properties: { windowId: WINDOW_ID } },
    target: 'window',
    effect: 'destructive',
  }),
  layout({
    id: 'wm.window.moveToWorkspace',
    description: 'Move a window with all its tabs to another workspace.',
    input: {
      type: 'object',
      properties: {
        windowId: WINDOW_ID,
        toWorkspaceId: {
          type: 'string',
          description: 'Destination workspace.',
        },
      },
      required: ['toWorkspaceId'],
    },
    target: 'window',
    effect: 'write',
  }),
  layout({
    id: 'wm.workspace.create',
    description: 'Create a new empty workspace at the end and switch to it.',
    input: { type: 'object', properties: {} },
    result: {
      type: 'object',
      properties: { workspaceId: { type: 'string' } },
    },
    target: 'none',
    effect: 'write',
  }),
  layout({
    id: 'wm.workspace.switch',
    description: 'Show another workspace.',
    input: { type: 'object', properties: { workspaceId: WORKSPACE_ID } },
    target: 'workspace',
    effect: 'write',
  }),
  layout({
    id: 'wm.workspace.delete',
    description:
      'Delete a workspace and close its windows. The last workspace cannot be deleted; the user confirms open windows.',
    input: { type: 'object', properties: { workspaceId: WORKSPACE_ID } },
    target: 'workspace',
    effect: 'destructive',
  }),
  layout({
    id: 'wm.windows.overview',
    description: 'Open the window overview.',
    input: { type: 'object', properties: {} },
    target: 'none',
    effect: 'write',
  }),
  layout({
    id: 'wm.workspaces.overview',
    description: 'Open the workspace overview.',
    input: { type: 'object', properties: {} },
    target: 'none',
    effect: 'write',
  }),
  layout({
    id: 'wm.launcher.open',
    description: 'Open the app launcher.',
    input: { type: 'object', properties: {} },
    target: 'none',
    effect: 'write',
  }),
]

/** Observation for agents (FR-028): layout, tab history, apps and the action catalog. */
export const WM_READ_ACTIONS: readonly ActionDefinition[] = [
  read({
    id: 'wm.state.get',
    description:
      'Read the open workspaces, windows (geometry, minimized, maximized) and their tabs with app, current location, title and attention flag. Workspaces carry a workspaceId, windows a windowId and tabs a tabId; actions on a tab, window or workspace need that id as their target.',
    input: { type: 'object', properties: {} },
    target: 'none',
  }),
  read({
    id: 'wm.tab.history',
    description: "Read a tab's back/forward history and its current position.",
    input: { type: 'object', properties: { tabId: TAB_ID } },
    target: 'tab',
  }),
  read({
    id: 'wm.apps.list',
    description:
      "List every installed app that can be opened: HaexVault's system apps and all installed extensions, each with its id, title, an extension's description, whether several instances are allowed, and its location patterns.",
    input: { type: 'object', properties: {} },
    target: 'none',
  }),
  read({
    id: 'wm.actions.list',
    description:
      'List every action with its description, input schema, target, permission scope, effect and whether agents may call it.',
    input: { type: 'object', properties: {} },
    target: 'none',
  }),
]
