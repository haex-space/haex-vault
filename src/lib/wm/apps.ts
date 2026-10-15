// App registry for the Workspace-Shell (spec 015-workspace-shell, T015). Pure data — the
// id-to-route/component mapping lives separately in src/components/wm/appRoutes.ts, so
// this module stays free of Vue/Nuxt and loadable by scripts/check-wm-state.ts.
import type { Size } from './types.ts'

/** Describes what runs in a tab. `id` is namespaced: `system.*` here, `extension.*` for installed
 * extensions (spec 017, `src/lib/extensions/apps.ts`); an unresolvable tab is dropped on hydrate,
 * FR-025. `defaultSize`/`minSize` apply to a *new window* opened with this app as its first tab; a
 * window with more than one tab keeps its own geometry. */
export type AppDefinition = {
  id: string
  /** i18n key for the localized name shown in the Launcher and tab. */
  titleKey: string
  /** A name that is not translated (an extension's own name); shown instead of `titleKey`. */
  title?: string
  /** What the app is for, untranslated (an extension's own description); `wm.apps.list` passes it
   * to agents so they describe the app instead of naming its id (spec 046, R17). */
  description?: string
  /** Iconify name (e.g. `lucide:message-square`). */
  icon: string
  /** An image shown instead of `icon` (an extension's own icon as a `data:` URL, or the coloured
   * icon of a system app in `public/app-icons/`). */
  iconUrl?: string
  defaultSize: Size
  minSize: Size
  /** `false` = singleton: re-opening activates the existing tab instead of creating a second one
   * (FR-016). Settings is a singleton; Chat is multi-instance (030 FR-001, amending 015 FR-017). */
  multiInstance: boolean
  /** What the tab shows as its title: the current location's title (default, e.g. the chat's
   * conversation) or always the app's own title (the settings, spec 023 research R2). The history
   * list shows location titles either way. */
  tabTitle?: 'location' | 'app'
  /** i18n key of why the app cannot open on this device (an extension that is not ready here);
   * the Launcher shows the entry disabled with that reason (spec 017, T080). */
  unavailableKey?: string
  /** A development version of an extension (spec 017, US12): its tabs carry a permanent badge. */
  dev?: boolean
}

export const WM_APPS: readonly AppDefinition[] = [
  {
    id: 'system.chat',
    titleKey: 'wm.apps.chat',
    icon: 'lucide:message-square',
    iconUrl: '/app-icons/chat.svg',
    defaultSize: { width: 960, height: 640 },
    minSize: { width: 420, height: 360 },
    multiInstance: true,
  },
  {
    id: 'system.settings',
    titleKey: 'wm.apps.settings',
    icon: 'lucide:settings-2',
    iconUrl: '/app-icons/settings.svg',
    defaultSize: { width: 760, height: 560 },
    minSize: { width: 420, height: 360 },
    multiInstance: false,
    tabTitle: 'app',
  },
  // Spec 034-password-manager: several windows may be open (a second one while copying from the
  // first); the tab shows the app's name, never the title of an entry.
  {
    id: 'system.passwords',
    titleKey: 'wm.apps.passwords',
    icon: 'lucide:key-round',
    iconUrl: '/app-icons/passwords.svg',
    defaultSize: { width: 960, height: 640 },
    minSize: { width: 360, height: 360 },
    multiInstance: true,
    tabTitle: 'app',
  },
  // Spec 051: the address book; several windows may be open, the tab shows the app's name.
  {
    id: 'system.contacts',
    titleKey: 'wm.apps.contacts',
    icon: 'lucide:contact',
    iconUrl: '/app-icons/contacts.svg',
    defaultSize: { width: 860, height: 640 },
    minSize: { width: 360, height: 360 },
    multiInstance: true,
    tabTitle: 'app',
  },
  // Spec 044: the file browser; each tab has its own folder.
  {
    id: 'system.files',
    titleKey: 'wm.apps.files',
    icon: 'lucide:folder',
    iconUrl: '/app-icons/files.svg',
    defaultSize: { width: 960, height: 640 },
    minSize: { width: 360, height: 360 },
    multiInstance: true,
    tabTitle: 'app',
  },
]

/** Apps that no longer exist and where their callers land now (spec 023 research R11): the
 * federation app became the settings category "Föderation", so the deep link
 * `?open=system.federation`, agents and old calls open that category. */
export const LEGACY_APP_ALIASES: Readonly<
  Record<string, { appId: string; at: string }>
> = {
  'system.federation': { appId: 'system.settings', at: '/federation' },
}

/** The app an `appId` opens, with the alias's location; `at: null` for an app without an alias. */
export function resolveAppAlias(appId: string): {
  appId: string
  at: string | null
} {
  return LEGACY_APP_ALIASES[appId] ?? { appId, at: null }
}

/** `undefined` for an unknown `appId` — every caller (Launcher, `+` menu, hydrate) must handle
 * that case rather than assume the registry is exhaustive (research R9). `apps` defaults to the
 * shipped registry; the reducers (layoutState.ts, tabs.ts) take it as a parameter instead of
 * importing `WM_APPS` directly so a test can substitute a `multiInstance: true` app (T052)
 * without touching the shipped registry. */
export function getAppDefinition(
  appId: string,
  apps: readonly AppDefinition[] = WM_APPS,
): AppDefinition | undefined {
  return apps.find((app) => app.id === appId)
}

export type TitleRef = {
  key: string | undefined
  params: Record<string, string>
}

/** The tab title for a tab of `app` whose location is titled `routed` (spec 023 research R2). */
export function tabTitleFor(
  app: AppDefinition | undefined,
  routed: TitleRef,
): TitleRef {
  return app?.tabTitle === 'app' ? { key: app.titleKey, params: {} } : routed
}
