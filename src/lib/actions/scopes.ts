// Permission scopes (spec 020-tab-navigation, T011, FR-031). Every action belongs to exactly one;
// spec 021 grants external agents access per scope. `guardrails` is locked for every agent,
// regardless of any grant (FR-032). Later specs (extensions, passwords, files, command-line shell) add scopes.

export const ACTION_SCOPE_IDS = [
  'wm.layout',
  'wm.navigation',
  'wm.read',
  'chat.read',
  'chat.write',
  'settings.read',
  'settings.device',
  'settings.models',
  'passwords.read',
  'files.read',
  'files.write',
  'files.shares',
  'contacts.read',
  'contacts.write',
  'guardrails',
] as const

export type ActionScopeId = (typeof ACTION_SCOPE_IDS)[number]

export type ActionScope = {
  id: ActionScopeId
  /** i18n key `actions.scopes.<id>`. */
  titleKey: string
  description: string
}

const DESCRIPTIONS: Record<ActionScopeId, string> = {
  'wm.layout': 'Open, arrange and close windows, tabs and workspaces.',
  'wm.navigation': 'Open apps at a location and move through tab history.',
  'wm.read':
    'Read the open workspaces, windows and tabs, tab histories, apps and the action catalog.',
  'chat.read': 'Read conversations and messages.',
  'chat.write':
    'Start, rename and delete conversations, send messages, cancel replies.',
  'settings.read': 'Read current settings, never provider credentials.',
  'settings.device': 'Change device settings such as the device name.',
  'settings.models': 'Choose, download and delete models.',
  'passwords.read':
    'Search the titles, tags and folders of the password manager. Never usernames, addresses or secrets.',
  'files.read':
    "List, search and read files of this device and of the storages granted, never holzi's own data.",
  'files.write':
    'Create folders, copy, rename, move and delete files of this device and of the storages granted for writing.',
  'files.shares':
    'List, create and change file shares to contacts and take access away. Agents need a permission of their own (spec 056 US6).',
  'contacts.read':
    'Search and read contacts of the address book, as far as the user allowed.',
  'contacts.write':
    'Change names and values of contacts, as far as the user allowed; never groups or where a contact goes.',
  guardrails:
    'Autonomy mode, delegate deny rules, provider connections, approvals and permission mode. Never callable by agents.',
}

export const ACTION_SCOPES: readonly ActionScope[] = ACTION_SCOPE_IDS.map(
  (id) => ({
    id,
    titleKey: `actions.scopes.${id}`,
    description: DESCRIPTIONS[id],
  }),
)
