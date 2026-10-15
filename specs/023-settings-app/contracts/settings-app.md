# Vertrag: Einstellungs-App

> **Hinweis (2026-10-15)**: Maßgeblich für Kategorien und Orte ist heute
> `src/lib/settings/registry.ts`. „Modelle“ und „Agenten“ sind seit PR #454 die
> Kategorie „KI“ (`/ai`, Orte `ai.*`); die Pfade `/models/…` und `/agents/…`
> in diesem Dokument sind der Stand bei der Umsetzung. „Darstellung“ gehört
> seit Spec 042 zu „Allgemein“.

**Spec**: [spec.md](../spec.md) | **Research**: [research.md](../research.md) |
**Datenmodell**: [data-model.md](../data-model.md)

## 1. Orte

App `system.settings`, Register in `src/lib/settings/registry.ts`, Komponenten
in `src/components/wm/appRoutes.ts`.

| Ort-`id`                 | Pfad                                 | Kategorie  | Übergeordnet             | Übersichtszeile | Komponente                              |
| ------------------------ | ------------------------------------ | ---------- | ------------------------ | --------------- | --------------------------------------- |
| `general`                | `/`                                  | general    | —                        | —               | `settings/GeneralView.vue`              |
| `appearance`             | `/appearance`                        | appearance | —                        | —               | `settings/ColorSchemeSetting.vue`       |
| `models`                 | `/models`                            | models     | —                        | —               | `settings/OverviewView.vue`             |
| `models.default`         | `/models/default`                    | models     | `models`                 | ja              | `settings/DefaultModelSetting.vue`      |
| `models.installed`       | `/models/installed`                  | models     | `models`                 | ja              | `settings/InstalledModels.vue`          |
| `models.download`        | `/models/download`                   | models     | `models`                 | ja              | `settings/DownloadModels.vue`           |
| `models.download.search` | `/models/download/search`            | models     | `models.download`        | —               | `models/HuggingFaceSearch.vue`          |
| `models.download.repo`   | `/models/download/repo/:owner/:name` | models     | `models.download.search` | —               | `models/HuggingFaceFilePicker.vue`      |
| `models.speech`          | `/models/speech`                     | models     | `models`                 | ja              | `settings/SttModelSetting.vue`          |
| `agents`                 | `/agents`                            | agents     | —                        | —               | `settings/OverviewView.vue`             |
| `agents.providers`       | `/agents/providers`                  | agents     | `agents`                 | ja              | `settings/ConnectDelegateProvider.vue`  |
| `agents.autonomy`        | `/agents/autonomy`                   | agents     | `agents`                 | ja              | `settings/AutonomyModeSetting.vue`      |
| `agents.denyRules`       | `/agents/deny-rules`                 | agents     | `agents`                 | ja              | `settings/DelegateDenyRulesSetting.vue` |
| `federation`             | `/federation`                        | federation | —                        | —               | `settings/FederationView.vue`           |

Query: nur `models.download.search` nutzt `q` (Suchbegriff). Deep-Link:
`/workspace/<vault>?open=system.settings&at=/models/download/search?q=qwen`
(URL-kodiert).

Tab-Titel: immer „Einstellungen“ (`tabTitle: 'app'` an der App-Definition,
`tabTitleFor` in `lib/wm/apps.ts`); die `titleKey` der Orte erscheinen in der
Verlaufsliste an Vor/Zurück und im Kopf.

Route-Komponenten bekommen keine Props. Was eine Einstellung vom Gerät braucht
(`vaultDeviceUuid`, Alias), stellt `SettingsApp.vue` per `provide` bereit
(`SETTINGS_DEVICE_KEY`, geladen einmal beim Montieren).

## 2. Gerüst (`components/apps/SettingsApp.vue`)

```text
┌──────────────────────────────────────────────────────────┐
│ [▯] [⌕ Suchen…      ×]            Werkzeugleiste         │
├──────────────┬───────────────────────────────────────────┤
│ ⚙ Allgemein  │  [←] Titel                                │
│ ◐ Darstell.  │  ╭───────────────────────────────────────╮│
│ ▣ Modelle    │  │ Einstellung             [Bedienung]   ││
│ ✦ Agenten    │  ├───────────────────────────────────────┤│
│ ⇄ Föderation │  │ Einstellung             [Bedienung]   ││
│              │  ╰───────────────────────────────────────╯│
└──────────────┴───────────────────────────────────────────┘
```

- Wurzel mit `@container`, zwei Zeilen: die Werkzeugleiste
  (`settings/Toolbar.vue`) und darunter Seitenleiste und Inhalt.
- **Werkzeugleiste**: nur der Knopf für die Seitenleiste (`aria-expanded`,
  `aria-controls="settings-sidebar"`, Beschriftung „Seitenleiste ein-/
  ausblenden“) und die Suche. Das Such-Symbol klappt dort ein Suchfeld auf
  (FR-023); dessen Knopf „×“ und Escape leeren es erst und schließen es dann.
  Der Knopf bleibt in beiden Breiten an derselben Stelle.
- Ab `@2xl` (672 px) steht die Seitenleiste (16rem, abgerundet, Farbe
  `muted`) neben dem Inhalt; `wideHidden` blendet sie auf Breite 0 aus.
  Darunter liegt sie absolut über dem Inhalt, außerhalb des Bildes und
  `invisible`; `menuOpen` schiebt sie herein, der Inhalt ist dann `inert`
  (FR-004). Beide Zustände sind lokal im Rahmen und werden nicht gemerkt;
  welcher gilt, liest der Rahmen an der berechneten `position` der Leiste ab
  (keine zweite Kopie der Schwelle). Wird das Fenster breit, schließt
  `menuOpen`. Breite und Verschiebung gehen mit 200 ms über (`motion-reduce`
  schaltet den Übergang ab). Beim Einblenden geht der Fokus auf die aktive
  Kategorie; Escape im schmalen Menü schließt es und gibt den Fokus an den
  Knopf zurück.
- **Listen** (`settings/Group.vue`, `settings/Row.vue`,
  `settings/OptionRow.vue`): abgerundete Gruppen (`muted`) mit fein getrennten
  Zeilen; eine Zeile hat optional Symbol, Titel, eine Zeile Beschreibung und
  rechts das Bedienelement, das in schmalen Fenstern unter den Text umbricht.
  Mit `to` ist die ganze Zeile ein Knopf mit Pfeil. Auswahl-Zeilen sind ganz
  das Label ihres Optionsfelds oder Häkchens. Ein Gruppenname steht nur, wo
  eine Ansicht mehrere Gruppen oder eine Auswahl hat. Keine eigenen
  Überschriften oder Beschreibungsabsätze (FR-002).
- **Seitenleiste** (`settings/Sidebar.vue`): eine Schaltfläche je Kategorie in
  Registerreihenfolge; hervorgehoben ist die Kategorie des aktuellen Orts
  (`aria-current="page"`). Ein Klick ist `router.push(category.path)`, auch wenn
  die Kategorie schon aktiv ist und eine Unteransicht offen ist (FR-010).
- **Titel**: unter der Werkzeugleiste, groß; nur der Titel des Orts
  (`locationFor`, Parameter eingesetzt), keine Beschreibungszeile. Titel und
  Inhalt teilen eine zentrierte Spalte
  (`max-w-3xl`). Hat der Ort ein `parent`, steht links ein Zurück-Pfeil mit
  Beschriftung „Zurück zu <Titel des Ziels>“; er führt `headerBack(history)`
  aus: `back` → `router.back()` (vorige Station in derselben Kategorie),
  `push` → `router.push(path)` (übergeordneter Ort) (FR-009). Ohne `parent` (Startseite einer Kategorie) steht an derselben
  Stelle und in derselben Größe das Symbol der Kategorie (`categoryOf`,
  dekorativ, `aria-hidden`), damit der Titel nicht springt.
- **Inhalt**: `<WmRouterView />` auf Tiefe 1; nur dieser Bereich scrollt,
  Werkzeugleiste, Titel und Seitenleiste stehen (US1 AS3).
- **Übersicht** (`settings/OverviewView.vue`): eine abgerundete Karte je
  `overviewRows(category)`: Symbol, Titel, eine Zeile Beschreibung, Pfeil;
  Klick `router.push(row.path)` (FR-003).
- Eine Unteransicht, deren Daten fehlen (gelöschtes Modell, getrennter
  Anbieter), zeigt einen Hinweis; der Zurück-Pfeil führt zur Übersicht (Edge
  Case).

## 3. Aktionen

In `src/lib/actions/settingsActions.ts`, Handler in
`src/stores/settingsActionHandlers.ts`. Einstellungen gelten für die Vault,
Standard- und Spracherkennungsmodell für dieses Gerät (FR-024); keine Aktion hat
einen Parameter `scope`.

| Kennung                              | Eingabe                                     | Ergebnis                     | Bereich           | Wirkung | Agent |
| ------------------------------------ | ------------------------------------------- | ---------------------------- | ----------------- | ------- | ----- |
| `settings.appearance.setColorScheme` | `{ scheme: 'light' \| 'dark' \| 'system' }` | `{ scheme }`                 | `settings.device` | write   | ja    |
| `settings.sessionRestore.set`        | `{ enabled: boolean }`                      | `{ enabled }`                | `settings.device` | write   | ja    |
| `settings.models.setDefault`         | `{ modelId }`                               | `{ done }`                   | `settings.models` | write   | ja    |
| `settings.models.clearDefault`       | `{}`                                        | `{ done }`                   | `settings.models` | write   | ja    |
| `settings.devices.list`              | `{}`                                        | `{ devices: VaultDevice[] }` | `settings.read`   | read    | ja    |

Entfallen: `settings.appearance.clearColorScheme`,
`settings.sessionRestore.clear` und der `scope` der Standard-Modell-Aktionen.
Autonomie (`settings.autonomy.setMode`) und Verbotsregeln
(`settings.delegate.setDenyRules`) behalten ihre Eingabe und schreiben den
Vault-Wert.

`VaultDevice = { vaultDeviceUuid, alias?, isCurrent }`; ein Gerät
ohne Namen hat kein `alias` (Schema-Subset ohne `null`).

`settings.get` meldet `colorScheme` (Wert), `sessionRestore` (boolean),
`defaultModel` (Kennung, fehlt ohne Wert), `sttModel`, `autonomyMode` und
`delegateDenyRules`.

Beim Standardmodell ruft „Keins“ `settings.models.clearDefault`.

Alle anderen Einstellungen behalten ihre Aktionen (FR-019); nur die Bedienung
ändert sich (research R5).

## 4. Farbschema (`composables/useColorScheme.ts`)

```text
useColorScheme() → {
  scheme: Readonly<Ref<ColorScheme>>          // Vault-Wert, ohne Wert 'system'
  startSystem(): void                         // vor dem Entsperren: System-Schema anwenden
  loadAsync(): Promise<void>                  // liest den Vault-Wert, wendet an
  setAsync(scheme): Promise<ColorScheme>      // schreibt, wendet an
}
```

- Modulweiter Zustand: ein Zustand je Prozess (eine Vault je Prozess, Spec 013).
- Wendet an: `document.documentElement.classList.toggle('dark', isDark(...))`
  und `style.colorScheme` (`dark`/`light`), damit native Elemente wie
  Optionsfelder und Rollbalken mitziehen; hört auf
  `matchMedia('(prefers-color-scheme: dark)')` und wendet bei `system` neu an
  (US4 AS2).
- `plugins/colorScheme.client.ts` wendet beim Start `system` an
  (`startSystem`). `pages/workspace/[instance].vue` ruft nach dem Öffnen
  `loadAsync` auf; ein Fehler beim Lesen lässt `system` stehen (Edge Case).
- Die Aktions-Handler rufen `setAsync`; die Einstellungsansicht
  (`settings/ColorSchemeSetting.vue`, Kategorie „Darstellung“) liest `scheme`
  und ruft nur Aktionen (Spec 020 FR-024): eine Zeile „Farbschema“ mit Hell /
  Dunkel / System.
- Die Suche findet die Einstellung über ihre Bezeichnung „Farbschema“
  (`settingKeys` der Kategorie `appearance`).

## 5. Entfallene App (`lib/wm/apps.ts`, `stores/wmActionHandlers.ts`)

- `resolveAppAlias('system.federation')` →
  `{ appId: 'system.settings', at: '/federation' }`; jede andere Kennung →
  `{ appId, at: null }`.
- `wm.app.open` und `wm.tab.new`: zuerst Alias auflösen, dann `knownApp`; ein
  `at` aus der Eingabe geht vor dem `at` des Alias.
- `pages/federation/[instance].vue` leitet auf
  `?open=system.settings&at=/federation` um.

## 5a. Befehl `list_vault_devices` (`src-tauri/src/device/commands.rs`)

- Keine Argumente; braucht eine offene Vault (sonst `NoActiveInstance`, wie
  `current_device_info`).
- Liefert `Vec<VaultDevicePayload>` (camelCase): `vaultDeviceUuid`, `alias`,
  `isCurrent`; dieses Gerät zuerst, dann nach Namen, Geräte ohne Namen zuletzt.
- Liest über `known_devices::list_devices`; die Vault-Bereichszeile fehlt.
- Frontend: `useDevice().listVaultDevicesAsync()`; der Handler von
  `settings.devices.list` liefert `{ devices }` und lässt `alias: null` weg.
- Rust: `known_devices::list_devices(conn) → Vec<KnownDevice>`, Reihenfolge in
  `order_vault_devices(devices, installation)` (rein, getestet).

## 6. Download-Store (`stores/modelDownloads.ts`)

- `downloads: Record<string, DownloadProgressEvent>`; Fortschritt setzt den
  Eintrag, Abschluss setzt ihn auf 100 %; die Ansicht, die den Download
  gestartet hat, entfernt ihn mit `clearDownload(id)`, wenn sie das Ergebnis
  verarbeitet hat (fertig, fehlgeschlagen, repariert). `downloadPercent(id)`.
- `watchDownloads()`: abonniert `onDownloadProgress`/`onDownloadComplete`
  einmal je Prozess; weitere Aufrufe kehren sofort zurück. Aufruf in
  `pages/workspace/[instance].vue` nach dem Öffnen.

## 7. i18n (de, en)

- `settings.categories.<id>.{title,description}` für die fünf Kategorien.
- `settings.federation.*` (Dieses Gerät, Unbenanntes Gerät).
- `settings.locations.<id>.{title,description}` für jeden Ort mit übergeordnetem
  Ort (jede Unteransicht).
- `settings.back` („Zurück zu {title}“).
- `settings.colorScheme.*` (Bezeichnung, Optionen, Meldungen).
- `actions.settings.appearance.setColorScheme`, `actions.settings.devices.list`.
- Entfällt: `wm.apps.federation`, `settings.header.forDevice`, die
  Speichern-Texte der umgestellten Einstellungen.

## Suche (`lib/settings/search.ts`, FR-023)

- `searchSettings(query, translate) → SettingsSearchHit[]` mit
  `{ location, path, label, trail }`; leere Eingabe → `[]`.
- Einträge: je Ort mit `keywordsKey` (alle Orte ohne Parameter) einer mit
  Titel, Beschreibung und Suchbegriffen, dazu einer je `settingKeys`-Bezeichnung
  (Pfad um den Ortstitel verlängert).
- Vergleich nach NFD ohne diakritische Zeichen und in Kleinbuchstaben; jedes
  Wort muss vorkommen. Rang: Bezeichnung beginnt mit der Eingabe, ein Wort der
  Bezeichnung beginnt damit, Bezeichnung enthält sie, nur Beschreibung oder
  Suchbegriffe; bei Gleichstand Reihenfolge der Registry.
- Suchfeld in der Werkzeugleiste (`type="search"`); sobald es Text enthält,
  zeigt der Rahmen die Seitenleiste mit den Treffern statt der Kategorien.
  Enter wählt den ersten Treffer; nach der Wahl ist die Suche geschlossen und
  das schmale Menü zu.

## Deep-Links (`pages/workspace/[instance].vue`, FR-011)

- `?open=<appId>&at=<Pfad>` wirkt beim Laden der Seite und, sobald die Sitzung
  wiederhergestellt ist, auch wenn der Parameter später in die Adresse kommt
  (`router.replace` auf dieselbe Seite); danach entfernt die Seite ihn wieder.

## Test-Hooks (Spec 016, `scripts/e2e/`)

Für die End-to-End-Szenarien `settings-*` (SC-007). Ein Hook ist ein
`data-testid` oder ein Datenattribut mit Daten, nie mit angezeigtem Text.

| Hook                                                                | Element                                                |
| ------------------------------------------------------------------- | ------------------------------------------------------ |
| `settings-title` mit `data-location="<Ort-ID>"`                     | Titel des Rahmens; zeigt, wo die Einstellungen sind    |
| `settings-category-<id>`, `settings-category-icon`                  | Seitenleisten-Eintrag, Kategorie-Symbol im Titel       |
| `settings-row-<Ort-ID>`, `settings-back`                            | Übersichtszeile, Pfeil im Titel                        |
| `settings-sidebar-toggle`, `#settings-sidebar`                      | Seitenleisten-Knopf, Seitenleiste                      |
| `settings-search-open`, `settings-search`, `settings-search-clear`  | Suche in der Werkzeugleiste                            |
| `settings-search-hit` mit `data-location`                           | Suchtreffer                                            |
| `[role="option"][data-value="<Wert>"]`                              | Eintrag einer `SettingsSelect`-Liste                   |
| `settings-color-scheme`, `settings-alias`, `session-restore-switch` | Farbschema, Gerätename, Sitzung                        |
| `settings-autonomy-<Modus>`, `settings-deny-<Kategorie>`            | Autonomiemodus, Verbotsregeln                          |
| `settings-device` mit `data-current`, `settings-device-current`     | Gerätezeile in „Föderation“, Markierung „Dieses Gerät“ |
| `[data-app-id="<appId>"]`                                           | Kachel im Launcher                                     |

Fensteraktionen (Größe, Zurück im Tab) laufen in den Szenarien über den
Aktionskatalog der Seite (`scripts/e2e/lib/settings.ts`), wie später ein Agent.
