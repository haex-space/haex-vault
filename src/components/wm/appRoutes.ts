import { defineAsyncComponent, type Component } from 'vue'
import { contactsRoutePatterns } from '~/lib/contacts/registry'
import { FILES_ROUTE_CHILDREN } from '~/lib/files/registry'
import { passwordsRoutePatterns } from '~/lib/passwords/registry'
import { settingsRoutePatterns } from '~/lib/settings/registry'
import { getAppDefinition } from '~/lib/wm/apps'
import { locationTitle, type RoutePattern } from '~/lib/wm/routeMatch'

/**
 * Per-app route tables for tab navigation (spec 020-tab-navigation, T020,
 * research R4, contracts/tab-navigation-contract.md §1) — replaces 015's
 * `appComponents.ts`. Kept separate from `lib/wm/apps.ts` so that pure
 * module stays free of Vue. Each component is built once at module load, so
 * Vue's async-component cache is keyed per app rather than reset per render.
 *
 * A record's `component` is optional below the root: an app can keep one
 * mounted root component and just read `useTabRouter().route` (the chat does,
 * so a conversation switch never remounts it and loses a running reply).
 */
export type AppRouteRecord = RoutePattern & {
  component?: Component
  children?: readonly AppRouteRecord[]
}

const ChatApp = defineAsyncComponent(
  () => import('~/components/apps/ChatApp.vue'),
)
const SettingsApp = defineAsyncComponent(
  () => import('~/components/apps/SettingsApp.vue'),
)
const PasswordsApp = defineAsyncComponent(
  () => import('~/components/apps/PasswordsApp.vue'),
)
const FilesApp = defineAsyncComponent(
  () => import('~/components/apps/FilesApp.vue'),
)
const ContactsApp = defineAsyncComponent(
  () => import('~/components/apps/ContactsApp.vue'),
)

/** The view of each settings location (spec 023-settings-app, contracts §1). */
const SETTINGS_VIEWS: Record<string, Component> = {
  general: defineAsyncComponent(
    () => import('~/components/settings/OverviewView.vue'),
  ),
  'general.basic': defineAsyncComponent(
    () => import('~/components/settings/BasicView.vue'),
  ),
  'general.basic.password': defineAsyncComponent(
    () => import('~/components/settings/PasswordChangeView.vue'),
  ),
  'general.appearance': defineAsyncComponent(
    () => import('~/components/settings/AppearanceView.vue'),
  ),
  'general.dock': defineAsyncComponent(
    () => import('~/components/settings/DockView.vue'),
  ),
  'general.licenses': defineAsyncComponent(
    () => import('~/components/settings/LicensesView.vue'),
  ),
  ai: defineAsyncComponent(
    () => import('~/components/settings/OverviewView.vue'),
  ),
  'ai.default': defineAsyncComponent(
    () => import('~/components/settings/DefaultModelSetting.vue'),
  ),
  'ai.installed': defineAsyncComponent(
    () => import('~/components/settings/InstalledModels.vue'),
  ),
  'ai.download': defineAsyncComponent(
    () => import('~/components/settings/DownloadModels.vue'),
  ),
  'ai.download.search': defineAsyncComponent(
    () => import('~/components/models/HuggingFaceSearch.vue'),
  ),
  'ai.download.repo': defineAsyncComponent(
    () => import('~/components/models/HuggingFaceFilePicker.vue'),
  ),
  'ai.speech': defineAsyncComponent(
    () => import('~/components/settings/SttModelSetting.vue'),
  ),
  'ai.providers': defineAsyncComponent(
    () => import('~/components/settings/ConnectDelegateProvider.vue'),
  ),
  'ai.servers': defineAsyncComponent(
    () => import('~/components/settings/ModelServersView.vue'),
  ),
  'ai.servers.server': defineAsyncComponent(
    () => import('~/components/settings/ModelServerView.vue'),
  ),
  'ai.autonomy': defineAsyncComponent(
    () => import('~/components/settings/AutonomyModeSetting.vue'),
  ),
  'ai.denyRules': defineAsyncComponent(
    () => import('~/components/settings/DelegateDenyRulesSetting.vue'),
  ),
  'ai.files': defineAsyncComponent(
    () => import('~/components/settings/AgentFilesSetting.vue'),
  ),
  storage: defineAsyncComponent(
    () => import('~/components/settings/storage/StorageListView.vue'),
  ),
  'storage.connection': defineAsyncComponent(
    () => import('~/components/settings/storage/ConnectionForm.vue'),
  ),
  'storage.storage': defineAsyncComponent(
    () => import('~/components/settings/storage/StorageForm.vue'),
  ),
  'storage.remove': defineAsyncComponent(
    () => import('~/components/settings/storage/RemovalView.vue'),
  ),
  contacts: defineAsyncComponent(
    () => import('~/components/settings/ContactsView.vue'),
  ),
  extensions: defineAsyncComponent(
    () => import('~/components/settings/extensions/ExtensionsListView.vue'),
  ),
  'extensions.browserBridge': defineAsyncComponent(
    () => import('~/components/settings/BrowserBridgeSettings.vue'),
  ),
  'extensions.detail': defineAsyncComponent(
    () => import('~/components/settings/extensions/ExtensionDetailView.vue'),
  ),
  federation: defineAsyncComponent(
    () => import('~/components/settings/FederationView.vue'),
  ),
  'federation.link': defineAsyncComponent(
    () => import('~/components/settings/LinkDeviceView.vue'),
  ),
  'federation.remove': defineAsyncComponent(
    () => import('~/components/settings/RemoveDeviceView.vue'),
  ),
}

/** The settings routes from the registry: `SettingsApp` as the frame, one flat child per location. */
function settingsRoutes(): AppRouteRecord[] {
  return settingsRoutePatterns().map((root) => ({
    path: root.path,
    component: SettingsApp,
    children: (root.children ?? []).map((child) => ({
      path: child.path,
      titleKey: child.titleKey,
      component: SETTINGS_VIEWS[child.locationId ?? ''],
    })),
  }))
}

/** One component object for an entry and its Verlauf, so a change between the two tabs keeps the
 * page mounted (spec 036, research R2) instead of loading the entry again. */
const PASSWORDS_ENTRY_PAGE = defineAsyncComponent(
  () => import('~/components/passwords/EntryPage.vue'),
)

/** One component object for the top level and a folder, so moving between them keeps the list
 * mounted instead of building every row again. */
const PASSWORDS_LIST_PAGE = defineAsyncComponent(
  () => import('~/components/passwords/List.vue'),
)

/** The view of each place of the password manager (spec 034); the stories add theirs. */
const PASSWORDS_VIEWS: Partial<Record<string, Component>> = {
  list: PASSWORDS_LIST_PAGE,
  folder: PASSWORDS_LIST_PAGE,
  entry: PASSWORDS_ENTRY_PAGE,
  trash: defineAsyncComponent(
    () => import('~/components/passwords/TrashView.vue'),
  ),
  history: PASSWORDS_ENTRY_PAGE,
  generator: defineAsyncComponent(
    () => import('~/components/passwords/GeneratorPage.vue'),
  ),
  import: defineAsyncComponent(
    () => import('~/components/passwords/ImportWizard.vue'),
  ),
}

/** The password manager routes from the registry: `PasswordsApp` as the frame, one flat child per
 * place. */
function passwordsRoutes(): AppRouteRecord[] {
  return passwordsRoutePatterns().map((root) => ({
    path: root.path,
    component: PasswordsApp,
    children: (root.children ?? []).map((child) => ({
      path: child.path,
      titleKey: child.titleKey,
      component: PASSWORDS_VIEWS[child.locationId ?? ''],
    })),
  }))
}

/** One component object for all contacts and a group, so moving between them keeps the list
 * mounted. */
const CONTACTS_LIST_PAGE = defineAsyncComponent(
  () => import('~/components/contacts/List.vue'),
)

/** The view of each place of the address book (spec 051). */
const CONTACTS_VIEWS: Partial<Record<string, Component>> = {
  list: CONTACTS_LIST_PAGE,
  group: CONTACTS_LIST_PAGE,
  entry: defineAsyncComponent(
    () => import('~/components/contacts/EntryPage.vue'),
  ),
  edit: defineAsyncComponent(
    () => import('~/components/contacts/EntryEditor.vue'),
  ),
  card: defineAsyncComponent(() => import('~/components/contacts/OwnCard.vue')),
  cardImport: defineAsyncComponent(
    () => import('~/components/contacts/CardImport.vue'),
  ),
}

/** The address book routes from the registry: `ContactsApp` as the frame, one flat child per
 * place. */
function contactsRoutes(): AppRouteRecord[] {
  return contactsRoutePatterns().map((root) => ({
    path: root.path,
    component: ContactsApp,
    children: (root.children ?? []).map((child) => ({
      path: child.path,
      titleKey: child.titleKey,
      component: CONTACTS_VIEWS[child.locationId ?? ''],
    })),
  }))
}

const APP_ROUTES: Record<string, readonly AppRouteRecord[]> = {
  // Chat (research R10): one mounted root; children only carry the location.
  'system.chat': [
    {
      path: '/',
      component: ChatApp,
      children: [
        { path: '' },
        { path: 'thread/:id', titleKey: 'wm.chat.thread' },
      ],
    },
  ],
  'system.settings': settingsRoutes(),
  'system.passwords': passwordsRoutes(),
  'system.contacts': contactsRoutes(),
  // File browser (spec 044, research R13): one mounted root; children only carry the location.
  'system.files': [
    { path: '/', component: FilesApp, children: [...FILES_ROUTE_CHILDREN] },
  ],
}

/** `undefined` for an `appId` no routes are registered for — `wm/TabPanel.vue` then renders
 * nothing, the same as an app the registry does not know (015 research R9). */
export function getAppRoutes(
  appId: string,
): readonly AppRouteRecord[] | undefined {
  return APP_ROUTES[appId]
}

/** The i18n key (and params) titling a location of an app (spec 020 research R9). */
export function titleForLocation(
  appId: string,
  path: string,
): { key: string | undefined; params: Record<string, string> } {
  return locationTitle(
    getAppRoutes(appId) ?? [],
    path,
    getAppDefinition(appId)?.titleKey,
  )
}
