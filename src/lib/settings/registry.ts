// Settings registry (spec 023-settings-app, research R1–R3, data-model.md, contracts §1): the
// categories of the sidebar and every location of the settings app. One source for the route
// patterns, sidebar, overview rows, header and location titles. Pure — the Vue side attaches the
// components in `components/wm/appRoutes.ts`.
import { normalizePath, type TabHistory } from '../wm/navigation.ts'
import { matchRoute } from '../wm/routeMatch.ts'

export type SettingsCategoryId =
  'general' | 'ai' | 'storage' | 'contacts' | 'extensions' | 'federation'

/** Headings of an overview that groups its rows by task. */
export type SettingsGroupId = 'chat' | 'local' | 'speech' | 'agents'

export type SettingsCategory = {
  id: SettingsCategoryId
  /** The category's own location. */
  path: string
  icon: string
  titleKey: string
}

export type SettingsLocation = {
  id: string
  /** Route pattern relative to `/` (`''` is `/` itself). */
  pattern: string
  category: SettingsCategoryId
  /** Target of the header back arrow; only sub-views have one. */
  parent?: string
  /** Only locations shown as an overview row. */
  icon?: string
  /** The heading an overview row is listed under; rows without one stand alone. */
  group?: SettingsGroupId
  /** Title in the header and the history list; may use the route params. */
  titleKey: string
  /** One line under the title where the location appears as a row (overview, search entry). The
   * header shows no description (operator decision 2026-09-26). */
  descriptionKey?: string
  overviewRow: boolean
  /** Synonyms the settings search matches besides title and description (FR-023); only locations
   * without route params are searchable. */
  keywordsKey?: string
  /** Labels of the single settings on this location's view, each its own search hit (FR-023). */
  settingKeys?: readonly string[]
}

function category(
  id: SettingsCategoryId,
  path: string,
  icon: string,
): SettingsCategory {
  return {
    id,
    path,
    icon,
    titleKey: `settings.categories.${id}.title`,
  }
}

/** In sidebar order (FR-005). */
export const SETTINGS_CATEGORIES: readonly SettingsCategory[] = [
  category('general', '/', 'lucide:sliders-horizontal'),
  category('ai', '/ai', 'lucide:sparkles'),
  category('storage', '/storage', 'lucide:cloud'),
  category('contacts', '/contacts', 'lucide:contact'),
  category('extensions', '/extensions', 'lucide:puzzle'),
  category('federation', '/federation', 'lucide:share-2'),
]

function categoryLocation(
  id: SettingsCategoryId,
  settingKeys?: readonly string[],
): SettingsLocation {
  return {
    id,
    pattern: SETTINGS_CATEGORIES.find((c) => c.id === id)!.path.slice(1),
    category: id,
    titleKey: `settings.categories.${id}.title`,
    overviewRow: false,
    keywordsKey: `settings.categories.${id}.keywords`,
    settingKeys,
  }
}

function subView(
  id: string,
  pattern: string,
  parent: string,
  options: {
    icon?: string
    group?: SettingsGroupId
    overviewRow?: boolean
    row?: boolean
    settingKeys?: readonly string[]
  } = {},
): SettingsLocation {
  const shownAsRow = (options.overviewRow ?? false) || (options.row ?? false)
  return {
    id,
    pattern,
    category: parent.split('.')[0] as SettingsCategoryId,
    parent,
    icon: options.icon,
    group: options.group,
    titleKey: `settings.locations.${id}.title`,
    descriptionKey: shownAsRow
      ? `settings.locations.${id}.description`
      : undefined,
    overviewRow: options.overviewRow ?? false,
    keywordsKey: pattern.includes(':')
      ? undefined
      : `settings.locations.${id}.keywords`,
    settingKeys: options.settingKeys,
  }
}

/** Overview rows appear in this order. */
export const SETTINGS_LOCATIONS: readonly SettingsLocation[] = [
  // Spec 042: "Allgemein" is an overview of "Grundeinstellung" and "Erscheinungsbild".
  categoryLocation('general'),
  subView('general.basic', 'general/basic', 'general', {
    icon: 'lucide:settings-2',
    overviewRow: true,
    settingKeys: [
      'settings.language.label',
      'settings.alias.label',
      'settings.sessionRestore.title',
      'settings.screenCapture.title',
    ],
  }),
  // Reached from its row in "Grundeinstellung"; an agent opens it with `wm.app.open` (FR-014).
  subView('general.basic.password', 'general/basic/password', 'general.basic', {
    icon: 'lucide:key-round',
    row: true,
  }),
  subView('general.appearance', 'general/appearance', 'general', {
    icon: 'lucide:palette',
    overviewRow: true,
    settingKeys: [
      'settings.colorScheme.label',
      'settings.background.label',
      'settings.iconSet.label',
      'settings.appearance.accent',
      'settings.appearance.window',
      'settings.appearance.container',
      'settings.appearance.text',
      'settings.appearance.component',
      'settings.appearance.windowHint',
    ],
  }),
  // Spec 045: the dock's placement on this device and its entries for the vault.
  subView('general.dock', 'general/dock', 'general', {
    icon: 'lucide:panel-bottom',
    overviewRow: true,
    settingKeys: [
      'settings.dock.style',
      'settings.dock.edge',
      'settings.dock.align',
      'settings.dock.mode',
      'settings.dock.items',
    ],
  }),
  // Spec 061: how many runs the log of each sync rule keeps on this device.
  subView('general.syncLog', 'general/sync-log', 'general', {
    icon: 'lucide:history',
    overviewRow: true,
    settingKeys: ['settings.syncLog.depth'],
  }),
  // Spec 053: the licenses of the bundled icon sets.
  subView('general.licenses', 'general/licenses', 'general', {
    icon: 'lucide:scale',
    overviewRow: true,
  }),
  // "KI" gathers the models and the agents, grouped by task in the overview.
  categoryLocation('ai'),
  subView('ai.default', 'ai/default', 'ai', {
    icon: 'lucide:star',
    group: 'chat',
    overviewRow: true,
  }),
  // Reached from "Standard-Modell" and the chat; an agent opens it with `wm.app.open`.
  subView('ai.providers', 'ai/providers', 'ai', {
    icon: 'lucide:plug',
    group: 'chat',
    overviewRow: true,
  }),
  // Spec 055 US5: OpenAI-compatible model servers (a GPU machine, a school server, a cloud API);
  // a server's page is reached from its row, never searched for.
  subView('ai.servers', 'ai/servers', 'ai', {
    icon: 'lucide:server',
    group: 'chat',
    overviewRow: true,
  }),
  subView('ai.servers.server', 'ai/servers/:providerId', 'ai.servers'),
  subView('ai.installed', 'ai/installed', 'ai', {
    icon: 'lucide:hard-drive',
    group: 'local',
    overviewRow: true,
  }),
  subView('ai.download', 'ai/download', 'ai', {
    icon: 'lucide:download',
    group: 'local',
    overviewRow: true,
  }),
  subView('ai.download.search', 'ai/download/search', 'ai.download', {
    icon: 'lucide:search',
    row: true,
  }),
  subView(
    'ai.download.repo',
    'ai/download/repo/:owner/:name',
    'ai.download.search',
  ),
  subView('ai.speech', 'ai/speech', 'ai', {
    icon: 'lucide:mic',
    group: 'speech',
    overviewRow: true,
  }),
  // Spec 066 US8: the assistant's voice per language and its speaking rate.
  subView('ai.speech.voice', 'ai/speech/voice', 'ai', {
    icon: 'lucide:audio-lines',
    group: 'speech',
    overviewRow: true,
    settingKeys: [
      'settings.assistant.labels.voice',
      'settings.assistant.labels.speechRate',
    ],
  }),
  subView('ai.autonomy', 'ai/autonomy', 'ai', {
    icon: 'lucide:shield',
    group: 'agents',
    overviewRow: true,
  }),
  subView('ai.denyRules', 'ai/deny-rules', 'ai', {
    icon: 'lucide:ban',
    group: 'agents',
    overviewRow: true,
  }),
  // Spec 044 FR-031b: what each agent may do with files.
  subView('ai.files', 'ai/files', 'ai', {
    icon: 'lucide:folder-lock',
    group: 'agents',
    overviewRow: true,
  }),
  // Spec 021: external agents (Claude Code, Codex, other MCP clients) and how to connect them.
  subView('ai.agents', 'ai/agents', 'ai', {
    icon: 'lucide:bot',
    group: 'agents',
    overviewRow: true,
  }),
  subView('ai.agents.agent', 'ai/agents/:agentId', 'ai.agents'),
  subView('ai.agents.log', 'ai/agents/:agentId/log', 'ai.agents.agent'),
  // Spec 065: the agent browser, its browser and the browser rights of the built-in agent.
  subView('ai.browser', 'ai/browser', 'ai', {
    icon: 'lucide:globe',
    group: 'agents',
    overviewRow: true,
  }),
  // Spec 038: storage connections (S3) and the storages on them. The forms and the removal are
  // reached from the list, never searched for.
  categoryLocation('storage'),
  subView('storage.connection', 'storage/connections/:connectionId', 'storage'),
  subView(
    'storage.storage',
    'storage/connections/:connectionId/storages/:storageId',
    'storage',
  ),
  subView('storage.remove', 'storage/remove/:kind/:id', 'storage'),
  // Spec 051 FR-015a: the address book; phone sync, business card and blocked vaults come with
  // their stories.
  categoryLocation('contacts'),
  // Spec 017: installed extensions, installing from a file.
  categoryLocation('extensions'),
  // Reached from an extension's row in the list, never searched for.
  // Spec 049: the External Bridge for browser extensions, a row of its own in the list.
  subView(
    'extensions.browserBridge',
    'extensions/browser-bridge',
    'extensions',
    {
      icon: 'lucide:globe',
      row: true,
    },
  ),
  // Spec 069: the marketplaces of the vault, a row of its own in the list; `new` creates one.
  subView('extensions.marketplaces', 'extensions/marketplaces', 'extensions', {
    icon: 'lucide:store',
    row: true,
  }),
  subView(
    'extensions.marketplaces.edit',
    'extensions/marketplaces/:id',
    'extensions.marketplaces',
  ),
  subView('extensions.detail', 'extensions/:extensionId', 'extensions'),
  categoryLocation('federation', ['settings.federation.deviceAccess.title']),
  subView('federation.link', 'federation/devices/link', 'federation', {
    icon: 'lucide:qr-code',
    row: true,
  }),
  // Reached from a device's row in the list, never searched for (spec 024, FR-026).
  subView(
    'federation.remove',
    'federation/devices/:devicePubkey/remove',
    'federation',
  ),
]

/** A `RoutePattern` that also names its location, so `appRoutes.ts` can attach the view. */
export type SettingsRoutePattern = {
  path: string
  titleKey?: string
  locationId?: string
  children?: readonly SettingsRoutePattern[]
}

/** A root `/` with one flat child per location, so an overview is never rendered together with
 * one of its sub-views (research R1). */
export function settingsRoutePatterns(): SettingsRoutePattern[] {
  return [
    {
      path: '/',
      children: SETTINGS_LOCATIONS.map((location) => ({
        path: location.pattern,
        titleKey: location.titleKey,
        locationId: location.id,
      })),
    },
  ]
}

export function locationFor(
  path: string,
): { location: SettingsLocation; params: Record<string, string> } | undefined {
  const match = matchRoute(settingsRoutePatterns(), path)
  const id = match?.chain[1]?.locationId
  const location = SETTINGS_LOCATIONS.find((candidate) => candidate.id === id)
  return location && match ? { location, params: match.params } : undefined
}

export function categoryOf(path: string): SettingsCategoryId | undefined {
  return locationFor(path)?.location.category
}

export function overviewRows(
  categoryId: SettingsCategoryId,
): SettingsLocation[] {
  return SETTINGS_LOCATIONS.filter(
    (location) => location.category === categoryId && location.overviewRow,
  )
}

/** The overview rows of a category under their headings, in registry order; `group` is
 * `undefined` for rows without one. Rows of one group stay together at the group's first row. */
export function overviewGroups(
  categoryId: SettingsCategoryId,
): { group: SettingsGroupId | undefined; rows: SettingsLocation[] }[] {
  const groups: {
    group: SettingsGroupId | undefined
    rows: SettingsLocation[]
  }[] = []
  for (const row of overviewRows(categoryId)) {
    const existing = row.group && groups.find((g) => g.group === row.group)
    if (existing) existing.rows.push(row)
    else groups.push({ group: row.group, rows: [row] })
  }
  return groups
}

/** The path of a location without route params (every parent is one). */
export function locationPath(location: SettingsLocation): string {
  return normalizePath(`/${location.pattern}`)
}

/** The parent's path of a sub-view, `undefined` for a category. */
export function parentPathOf(location: SettingsLocation): string | undefined {
  const parent = SETTINGS_LOCATIONS.find(
    (candidate) => candidate.id === location.parent,
  )
  return parent ? locationPath(parent) : undefined
}

export type HeaderBack =
  { kind: 'back'; path: string } | { kind: 'push'; path: string }

/** FR-009: where the header arrow of the current location leads. Back to the previous station
 * when it lies in the same category (so Modelle → Installierte Modelle → Modelle herunterladen
 * returns to Installierte Modelle, and the search keeps its query), as a tab back without a
 * second entry; otherwise — after a deep link, a sidebar or search jump from another category —
 * to the registry parent. `undefined` for a category start page, which has no arrow. */
export function headerBack(history: TabHistory): HeaderBack | undefined {
  const current = history.entries[history.index]
  const here = current ? locationFor(current.location.path) : undefined
  const parentPath = here ? parentPathOf(here.location) : undefined
  if (!here || !parentPath) return undefined
  const previous = history.entries[history.index - 1]
  if (
    previous &&
    categoryOf(previous.location.path) === here.location.category
  ) {
    return { kind: 'back', path: normalizePath(previous.location.path) }
  }
  return { kind: 'push', path: normalizePath(parentPath) }
}
