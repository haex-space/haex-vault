# Contract: user interface

Style follows specs 035/036 and 023: toolbar, large heading, groups and rows (`SettingsGroup`,
`Row`, `OptionRow`), no save buttons in the settings, "HaexVault" in the text, never the word
"Layout". All texts in `src/i18n/locales/{de,en}`. Every operable element has a `data-testid`.

## wm app `system.marketplace`

- Entry in `src/lib/wm/apps.ts`: `titleKey: wm.apps.marketplace` ("Erweiterungen entdecken" /
  "Discover extensions"), icon `lucide:store`, `multiInstance: false`, `tabTitle: 'app'`,
  size 960 × 640, minimum 360 × 360. Component `MarketplaceApp.vue`, routes in
  `src/lib/marketplace/registry.ts` and `appRoutes.ts`.
- Routes (tab navigation, spec 020): `/` catalog, `/:slug` details (with `?source=<id>`). The catalog keeps
  its state in the query of the tab (`?q`, `?category`, `?source`, `?page`), so "Zurück" from the details
  returns to the catalog as it was.
- **Agents**: an agent may open the window (`wm.app.open` finds the app by its name like every other
  app; opening is harmless). Querying the catalog and installing stay out of reach of agents and
  extensions (FR-030); the app registers no agent action.
- **Catalog**: toolbar with a search field, a category selector and "Aktualisieren" (FR-017). Below it a
  grid of tiles (one column below `@lg`). States: loading, empty state "Keine Erweiterungen", "Kein Marketplace eingerichtet" with "Öffentlichen Marketplace hinzufügen" and "Eigenen Marketplace hinzufügen" (US4-9), "Keine
  Verbindung" (all sources failed), "Alle Marketplaces sind ausgeschaltet" with a jump to the
  settings. One error line per source with "Erneut versuchen", above the grid (FR-031). A "Quelle" selector (all / one marketplace)
  next to the category (FR-012), shown when more than one marketplace is switched on; every tile names its
  source. Paging at the bottom.
- **Tile**: icon, name, short description, publisher (with "geprüft" as a mere display), rating,
  number of downloads, state mark "Installiert (x)" / "Update verfügbar (x → y)", button
  "Installieren" / "Aktualisieren", disabled with the hint "Benötigt HaexVault ab x" (FR-018).
- **Details** (`/:slug`): description, publisher, category, tags, source, list of versions (with
  change note, compatible or not), buttons "Installieren/Aktualisieren/Entfernen". A
  chosen older version leads to the downgrade confirmation in the preview (spec 017).
- **Installing**: progress display with "Abbrechen" (FR-015), then `InstallDialog` with
  `source = { kind: 'marketplace', id }`; it additionally names the source (FR-011). Closing without
  installing calls `marketplace_download_discard`.
- Test IDs: `marketplace-app`, `marketplace-search`, `marketplace-category`, `marketplace-source-filter`, `marketplace-empty-add-preset`, `marketplace-empty-add-own`, `marketplace-refresh`,
  `marketplace-tile-<sourceId>-<slug>`, `marketplace-install-<sourceId>-<slug>`,
  `marketplace-update-<sourceId>-<slug>`,
  `marketplace-source-error-<sourceId>`, `marketplace-source-retry-<sourceId>`, `marketplace-empty`,
  `marketplace-offline`, `marketplace-none`, `marketplace-all-off`, `marketplace-loading`,
  `marketplace-page-next`, `marketplace-page-previous`, `marketplace-page`, `marketplace-progress`,
  `marketplace-progress-cancel`, `marketplace-installed-<sourceId>-<slug>`,
  `marketplace-blocked-<sourceId>-<slug>`, `marketplace-tile-source-<sourceId>-<slug>`,
  `marketplace-details`, `marketplace-details-source`, `marketplace-details-error`, `marketplace-back`,
  `marketplace-version-<version>`, `marketplace-install-version-<version>`,
  `marketplace-remove-<sourceId>-<slug>`, `marketplace-install-error`,
  `marketplace-install-error-dismiss`; in the install dialog `extensions-install-origin`,
  `extensions-install-publisher-differs`, `extensions-install-cancel`.

- **Refresh after changes**: After installing, updating and removing (also from the
  settings or from another device) the tile states reload (`vault-data-changed`) without the
  user having to reopen the catalog. The legacy app had to fix this afterwards.

## Settings → Extensions → Marketplaces

- A new row in the list of extensions (like the row "Browser-Verbindung", spec 049) and a sub-view
  `extensions/marketplaces` in `src/lib/settings/registry.ts`, form `extensions/marketplaces/:id`
  (`new` for creating).
- **List**: per marketplace name, address, an on/off switch (takes effect at once), up/down to reorder
  (keyboard and pointer), state of the credentials ("Zugangsdaten fehlen" / "wird synchronisiert") and
  "Bearbeiten". All rows are equal (the preset too): edit, on/off, reorder, delete.
- **Form** (deliberately confirmed, FR-021): name, address (with a hint for `http://`), auth type
  (Keine/Bearer/Basic), credentials entry (`CredentialsEntryPicker` with `source = marketplace_*`,
  newly entered values are stored as a visible entry, spec 052), "Testen und speichern" shows the result
  (reachable, N extensions / cause of the error) and still allows saving. "Löschen" with
  confirmation (own marketplaces only).
- Test IDs: `settings-marketplaces`, `settings-marketplace-row-<id>`, `settings-marketplace-toggle-<id>`,
  `settings-marketplace-up-<id>`, `settings-marketplace-down-<id>`, `settings-marketplace-form`,
  `settings-marketplace-name`, `settings-marketplace-url`, `settings-marketplace-edit-<id>`,
  `settings-marketplace-save-anyway`, `settings-marketplace-delete-confirm`, `settings-marketplaces-empty`,
  `settings-marketplace-http-hint`, `settings-marketplace-credentials-<id>`, `extensions-marketplaces`,
  `marketplace-all-off-settings`, `settings-marketplace-auth`,
  `settings-marketplace-test-result`, `settings-marketplace-save`, `settings-marketplace-delete`.

## List of installed extensions

- A row with a known update (from `marketplace_known_updates`, without a query of its own, FR-017) shows
  "Update verfügbar (y)" for the known compatible candidate with the highest version and jumps
  into the catalog to exactly that entry (`/:slug?source=<id>`). On equal versions the
  marketplace rank decides which entry.
- Test ID: `extension-update-hint-<id>`.

## Accessibility and input

All actions by keyboard (Tab, Enter, Space), a visible focus marker, labels for switches and
sort arrows, state marks not only as color. Mobile: touch targets ≥ 44 px, one-column grid.
