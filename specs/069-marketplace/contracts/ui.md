# Vertrag: Oberfläche

Stil nach Spec 035/036 und 023: Werkzeugleiste, große Überschrift, Gruppen und Zeilen (`SettingsGroup`,
`Row`, `OptionRow`), keine Speichern-Schaltflächen in den Einstellungen, „HaexVault“ im Text, kein Wort
„Layout“. Alle Texte in `src/i18n/locales/{de,en}`. Jedes bedienbare Element hat ein `data-testid`.

## wm-App `system.marketplace`

- Eintrag in `src/lib/wm/apps.ts`: `titleKey: wm.apps.marketplace` („Erweiterungen entdecken“ /
  „Discover extensions“), Symbol `lucide:store`, `multiInstance: false`, `tabTitle: 'app'`,
  Größe 960 × 640, Minimum 360 × 360. Komponente `MarketplaceApp.vue`, Routen in
  `src/lib/marketplace/registry.ts` und `appRoutes.ts`.
- Routen (Tab-Navigation, Spec 020): `/` Katalog, `/:slug` Details (mit `?source=<id>`).
- **Katalog**: Werkzeugleiste mit Suchfeld, Kategorie-Auswahl und „Aktualisieren“ (FR-017). Darunter ein
  Raster (einspaltig unter `@lg`) aus Kacheln. Zustände: lädt, Leerzustand „Keine Erweiterungen“, „Kein Marketplace eingerichtet“ mit „Öffentlichen Marketplace hinzufügen“ und „Eigenen Marketplace hinzufügen“ (US4-9), „Keine
  Verbindung“ (alle Quellen ausgefallen), „Alle Marketplaces sind ausgeschaltet“ mit Sprung zu den
  Einstellungen. Fehlerzeile je Quelle mit „Erneut versuchen“, über dem Raster (FR-031). „Quelle“-Auswahl (Alle / ein Marketplace)
  neben der Kategorie (FR-012); jede Kachel nennt ihre Quelle. Blättern unten.
- **Kachel**: Symbol, Name, Kurzbeschreibung, Herausgeber (mit „geprüft“ als bloßer Anzeige), Bewertung,
  Zahl der Downloads, Zustandsmarke „Installiert (x)“ / „Update verfügbar (x → y)“, Schaltfläche
  „Installieren“ / „Aktualisieren“, gesperrt mit Hinweis „Benötigt HaexVault ab x“ (FR-018).
- **Details** (`/:slug`): Beschreibung, Herausgeber, Kategorie, Tags, Quelle, Liste der Fassungen (mit
  Änderungsnotiz, kompatibel oder nicht), Schaltflächen „Installieren/Aktualisieren/Entfernen“. Eine
  gewählte ältere Fassung führt in der Vorschau zur Downgrade-Bestätigung (Spec 017).
- **Installieren**: Fortschrittsanzeige mit „Abbrechen“ (FR-015), danach `InstallDialog` mit
  `source = { kind: 'marketplace', id }`; er nennt die Quelle zusätzlich (FR-011). Schließen ohne
  Installieren ruft `marketplace_download_discard`.
- Test-IDs: `marketplace-app`, `marketplace-search`, `marketplace-category`, `marketplace-source-filter`, `marketplace-empty-add-preset`, `marketplace-empty-add-own`, `marketplace-refresh`,
  `marketplace-tile-<slug>`, `marketplace-install-<slug>`, `marketplace-update-<slug>`,
  `marketplace-source-error-<sourceId>`, `marketplace-source-retry-<sourceId>`, `marketplace-empty`,
  `marketplace-offline`, `marketplace-page-next`, `marketplace-progress`, `marketplace-progress-cancel`.

- **Aktualisierung nach Änderungen**: Nach Installieren, Aktualisieren und Entfernen (auch aus den
  Einstellungen oder von einem anderen Gerät) laden die Kachelzustände neu (`vault-data-changed`), ohne dass
  der Nutzer den Katalog neu öffnet. Das Legacy musste das nachträglich beheben.

## Einstellungen → Erweiterungen → Marketplaces

- Neue Zeile in der Liste der Erweiterungen (wie die Zeile „Browser-Verbindung“, Spec 049) und Unteransicht
  `extensions/marketplaces` in `src/lib/settings/registry.ts`, Formular `extensions/marketplaces/:id`
  (`new` für Anlegen).
- **Liste**: je Marketplace Name, Adresse, Schalter Ein/Aus (gilt sofort), Auf/Ab zum Umsortieren
  (Tastatur und Zeiger), Zustand der Zugangsdaten („Zugangsdaten fehlen“ / „wird synchronisiert“) und
  „Bearbeiten“. Alle Zeilen sind gleich (auch die Voreinstellung): Bearbeiten, Ein/Aus, Umsortieren, Löschen.
- **Formular** (bewusst bestätigt, FR-021): Name, Adresse (mit Hinweis bei `http://`), Auth-Art
  (Keine/Bearer/Basic), Zugangsdaten-Eintrag (`CredentialsEntryPicker` mit `source = marketplace_*`,
  neue Werte werden als sichtbarer Eintrag abgelegt, Spec 052), „Testen und speichern“ zeigt das Ergebnis
  (erreichbar, N Erweiterungen / Fehlerursache) und erlaubt trotzdem zu speichern. „Löschen“ mit
  Bestätigung (nur eigene).
- Test-IDs: `settings-marketplaces`, `settings-marketplace-row-<id>`, `settings-marketplace-toggle-<id>`,
  `settings-marketplace-up-<id>`, `settings-marketplace-down-<id>`, `settings-marketplace-form`,
  `settings-marketplace-name`, `settings-marketplace-url`, `settings-marketplace-auth`,
  `settings-marketplace-test-result`, `settings-marketplace-save`, `settings-marketplace-delete`.

## Liste der installierten Erweiterungen

- Eine Zeile mit bekanntem Update (aus `marketplace_known_updates`, ohne eigene Abfrage, FR-017) zeigt
  „Update verfügbar (y)“ und springt in den Katalog zu dieser Erweiterung (`/:slug`).
- Test-ID: `extension-update-hint-<id>`.

## Barrierefreiheit und Eingabe

Alle Aktionen per Tastatur (Tab, Enter, Leertaste), sichtbare Fokusmarke, Beschriftungen für Schalter und
Sortierpfeile, Zustandsmarken nicht nur als Farbe. Mobil: Touch-Ziele ≥ 44 px, Raster einspaltig.
