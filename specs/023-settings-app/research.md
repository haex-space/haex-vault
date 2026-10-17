# Research: Einstellungs-App mit Kategorien

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Datum**:
2026-09-26

Referenz haex-vault 2.x (Legacy) @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`
(`src/components/haex/system/settings/`,
`src/components/haex/system/settings-layout/`).

## Graphify-Konsultation

Graph `graphify-out/graph.json` im Worktree, Stand 2026-09-21 (verknüpfter
Worktree: Snapshot wird ohne Auffrischung genutzt). Abfragen: „settings app
layout sections“, „tab location routing inside an app“, „color scheme dark mode
theme“, „federation app placeholder“, „app definition registry and deep link
open at location“.

Der Snapshot ist älter als die Window-Manager-Arbeit (015, 020, 022) und kennt
`useTabRouter`, `routeMatch.ts` und `lib/wm/apps.ts` nicht; die Kandidaten
wurden deshalb zusätzlich direkt im Code geprüft. Ergebnis:

- **Erweitert statt neu gebaut**: `lib/wm/apps.ts` (Alias der entfallenen App,
  R7), Download-Fortschritt neben `stores/models.ts` (R6), `components/wm/appRoutes.ts`
  (Routen der Einstellungen, R1), `usePreferences` (Farbschema, R8),
  `scripts/check-vue-templates.ts` (Farbprüfung, R9).
- **Kein Kandidat**: Kategorien-Register, Seitenleiste, Kopf mit Zurück,
  Farbschema-Anwendung. Es gibt kein Farbschema-Handling und keine
  Seitenleiste in holzi.
- **Geräteliste** (R12): `storage/known_devices.rs` und `device/commands.rs`
  werden erweitert; es gibt dort nur Lesen des eigenen Geräts und Umbenennen.
- Zur späteren manuellen Prüfung vermerkt: Abfragen gegen einen veralteten
  Graphen.
- **Nachtrag 2026-09-27 (T059)**: Graph auf `main` (`d81bf86`) neu gebaut,
  6305 Knoten, 12618 Kanten, 376 Gemeinschaften. Dieselben fünf Abfragen führen
  jetzt zu den Bausteinen dieser Spec: „settings app layout sections“ zu
  `lib/settings/registry.ts`, `apps/SettingsApp.vue`, `lib/settings/search.ts`;
  „tab location routing inside an app“ zu `registry.ts`, `SettingsApp.vue`,
  `useChatNavigation.ts`; „color scheme dark mode theme“ zu
  `lib/settings/colorScheme.ts`, `ColorSchemeSetting.vue`; „federation app
  placeholder“ zu `useDevice.ts`, `settings/FederationView.vue`; „app definition
  registry and deep link open at location“ zu `registry.ts`, `lib/wm/apps.ts`,
  `stores/windowManager.ts`, `wm/Link.vue`. Keine Abfrage zeigt einen zweiten,
  übersehenen Baustein für dieselbe Aufgabe; die Entscheidungen oben bleiben.
  Rauschen wie `autonomy_tests.rs` bei den Tab-Orten stammt aus gemeinsamen
  Begriffen, nicht aus einer Abhängigkeit.

## R1 Orte und Routen der Einstellungen

**Entscheidung**: Die App `system.settings` bekommt eine Routentabelle mit einem
Wurzel-Eintrag `/` (Komponente `SettingsApp.vue`, das Gerüst) und flachen
Kindern, alle auf Tiefe 1:

| Ort                                  | Ansicht                                            | Kategorie   |
| ------------------------------------ | -------------------------------------------------- | ----------- |
| `/`                                  | Allgemein: Gerätename, Sitzung wiederherstellen    | Allgemein   |
| `/appearance`                        | Darstellung: Farbschema                            | Darstellung |
| `/models`                            | Übersicht Modelle                                  | Modelle     |
| `/models/default`                    | Standardmodell                                     | Modelle     |
| `/models/installed`                  | Installierte Modelle (Laden, Updates, Löschen)     | Modelle     |
| `/models/download`                   | Empfohlene Modelle, Zeile „Auf HuggingFace suchen“ | Modelle     |
| `/models/download/search?q=…`        | HuggingFace-Suche (Suchfeld und Ergebnisse)        | Modelle     |
| `/models/download/repo/:owner/:name` | Dateiauswahl eines HuggingFace-Repos               | Modelle     |
| `/models/speech`                     | Spracherkennungsmodell                             | Modelle     |
| `/agents`                            | Übersicht Agenten                                  | Agenten     |
| `/agents/providers`                  | Anbieter verbinden                                 | Agenten     |
| `/agents/autonomy`                   | Autonomiemodus                                     | Agenten     |
| `/agents/deny-rules`                 | Deny-Regeln                                        | Agenten     |
| `/federation`                        | Föderation: Geräte der Vault                       | Föderation  |

- Der Start-Ort `/` eines neuen Tabs ist die erste Kategorie „Allgemein“
  (FR-010). Es gibt keinen zweiten Ort `/general` und keine Umleitung.
- Die Kinder sind flache Muster mit mehreren Segmenten (`routeMatch.ts`
  unterstützt das). Eine Übersicht wird so nie zusammen mit ihrer Unteransicht
  gerendert; `SettingsApp.vue` bleibt montiert und rendert das Kind über ein
  verschachteltes `WmRouterView`.
- Der Suchbegriff steht in der Query (`q`), gesetzt mit `setQuery` (ersetzt den
  Eintrag, Spec 020 FR-005). Der Wechsel von der Suche zu einem Repo ist ein
  `push`, Zurück kehrt zu den Ergebnissen zurück.
- HuggingFace-Repo-Kennungen haben die Form `owner/name` und belegen deshalb
  zwei Segmente.
- Unbekannte Orte fängt `wm/RouterView.vue` schon ab (Rückfall auf `/` mit
  Hinweis, Spec 020 FR-014), was FR-012 erfüllt.

**Begründung**: Spec 020 macht jede Ansicht zu einem Ort; die flache Tabelle
hält Übersicht und Unteransicht getrennt, ohne neue Router-Fähigkeiten.

**Alternativen**: Verschachtelte Kinder unter `/models` (würde die Übersicht
über der Unteransicht montieren oder eine leere Zwischenkomponente brauchen);
Kategorie `/general` mit Umleitung von `/` (zwei Orte für dieselbe Ansicht,
doppelte Historieneinträge).

## R2 Kategorien-Register

**Entscheidung**: Ein reines Modul `src/lib/settings/registry.ts` beschreibt
Kategorien und Orte: Kennung, Pfadmuster, Kategorie, Symbol, i18n-Schlüssel für
Titel und Beschreibung, übergeordneter Ort. Daraus entstehen:

- die Routenmuster (`settingsRoutePatterns()`), an die `appRoutes.ts` die
  Komponenten hängt (so bleibt das Register unter Node testbar),
- die Seitenleiste (Kategorien in fester Reihenfolge),
- die Zeilen der Übersichten,
- Kopf (Titel, Beschreibung, Zurück) und Tab-Titel (`titleKey` je Route, Spec
  020 R9).

**Begründung**: Eine Quelle für Reihenfolge, Titel und Hierarchie; FR-005,
FR-002, FR-003 und die Tab-Titel können nicht auseinanderlaufen.

**Tab-Titel** (Betreiberentscheidung bei der Analyse): Der Tab der Einstellungen
heißt immer „Einstellungen“; die Orte tragen ihren `titleKey` nur für die
Verlaufsliste. Dafür bekommt `AppDefinition` in `lib/wm/apps.ts` ein Feld
`tabTitle: 'location' | 'app'` (Standard `location`, wie der Chat mit dem Namen
der Unterhaltung), und eine reine Funktion `tabTitleFor(app, routed)` wählt
zwischen App-Titel und Orts-Titel; `tabDisplayInfo` im Store ruft sie auf.
`system.settings` setzt `tabTitle: 'app'`.

**Alternativen**: Kategorien in der Vue-Komponente fest verdrahten (nicht
testbar, dreifache Pflege).

## R3 Zurück im Kopf

**Entscheidung** (überarbeitet 2026-09-26 nach Betreiber-Rückmeldung): Reine
Funktion `headerBack(history)` für den aktuellen Eintrag:

- Liegt der vorige Historieneintrag des Tabs in derselben Kategorie, wirkt der
  Pfeil wie Zurück im Tab (`goTab(tab, -1)`); die Query des Eintrags bleibt,
  etwa der Suchbegriff. So führt Modelle → Installierte Modelle → Modelle
  herunterladen zurück zu „Installierte Modelle“.
- Sonst `push` auf den übergeordneten Ort: nach einem Deep-Link oder einem
  Sprung aus einer anderen Kategorie (Seitenleiste, Suche).
- Kategorien selbst haben keinen Pfeil (`undefined`).

Übergeordnet: Unteransicht → Übersicht der Kategorie; Suche → Download; Repo →
Suche. Ein Klick in der Seitenleiste ist immer ein `push` auf den Ort der
Kategorie, auch aus einer Unteransicht derselben Kategorie (FR-010). Die
Beschriftung des Pfeils nennt das Ziel.

**Begründung**: FR-009 verlangt „kein doppelter Eintrag“ und einen Weg zur
Übersicht nach einem Deep-Link. Die erste Fassung ging nur dann zurück, wenn
die vorige Station genau der übergeordnete Ort war; nach einem Sprung zwischen
Geschwistern (über eine Zeile, die an eine andere Stelle führt) landete der
Nutzer dann beim übergeordneten Ort statt dort, woher er kam.

## R4 Seitenleiste in schmalen Fenstern

**Entscheidung** (überarbeitet 2026-09-26 nach Betreiber-Rückmeldung, Vorbild
GNOME-Einstellungen): CSS-Container-Abfragen von Tailwind v4 (`@container` am
Einstellungs-Gerüst). Ab `@2xl` (42rem, 672 px) steht die Seitenleiste (16rem)
neben dem Inhalt und lässt sich ausblenden; darunter ist sie ausgeblendet und
öffnet sich über den Knopf in der Werkzeugleiste über den Inhalt. Die Lage kommt aus
CSS; der Rahmen liest beim Umschalten die berechnete `position` der Leiste, um
zu wissen, welcher der zwei Zustände (`wideHidden`, `menuOpen`) gemeint ist.

**Begründung**: FR-004 verlangt die Fensterbreite, nicht die Bildschirmbreite.
Die frühere Symbolleiste mit Tooltips kostete in schmalen Fenstern dauerhaft
Breite, und Tooltips öffnen auf Touch nicht beim langen Drücken. Das
Vollbild-Menü gibt dem Inhalt den ganzen Platz. Die Schwelle bleibt 672 px,
weil das Fenster mit 760 px startet (`lib/wm/apps.ts`).

**Alternativen**: Symbolleiste mit Tooltips (erste Umsetzung, verworfen);
Navigation als Seitenstapel wie im schmalen GNOME (die Seitenleiste als erste
Seite) – passt nicht zu den Orten aus Spec 020, weil die Leiste kein Ort ist;
Messen der Breite in JavaScript für die Darstellung (Aufblitzen beim ersten
Zeichnen).

## R5 Einstellungen ohne Speichern-Knopf (FR-021)

**Entscheidung**: Die vorhandenen Einstellungskomponenten werden umgestellt,
ihre Aktionen bleiben:

| Einstellung     | Bedienung neu                                                                                                                             | Aktion                                         |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| Gerätename      | Textfeld, speichert beim Verlassen und mit Enter; leer → Hinweis, kein Speichern                                                          | `settings.device.setAlias`                     |
| Sitzung         | unverändert (Spec 022, Vorbild)                                                                                                           | `settings.sessionRestore.*`                    |
| Farbschema      | zwei Auswahlen: „Dieses Gerät“ (Wie alle Geräte / Hell / Dunkel / System), „Alle Geräte“ (System / Hell / Dunkel)                         | neu, R8                                        |
| Standardmodell  | zwei Auswahlen: „Dieses Gerät“ (Wie alle Geräte / Modelle), „Alle Geräte“ (Keins / Modelle)                                               | `settings.models.setDefault` / `.clearDefault` |
| Spracherkennung | Auswahl nur mit installierten Modellen, speichert beim Wählen; nicht installierte stehen darunter mit Knopf „Herunterladen und verwenden“ | `settings.models.setStt`                       |
| Autonomiemodus  | Optionen speichern beim Wählen                                                                                                            | `settings.autonomy.setMode`                    |
| Verbotsregeln   | Kontrollkästchen speichern beim Anklicken; ein Fehler stellt die gespeicherte Auswahl wieder her                                          | `settings.delegate.setDenyRules`               |

Knöpfe bleiben für Handlungen: Anbieter verbinden, Modell laden, herunterladen,
Updates prüfen und installieren, löschen.

„Wie alle Geräte“ bzw. „Keins“ ist die Option „nicht festgelegt“ aus FR-021 und
ruft die `clear`-Aktion. Beim Farbschema ruft auch „Alle Geräte: System“ die
`clear`-Aktion auf, weil ohne Vault-Wert „System“ gilt; es wird nie `system`
als Vault-Wert gespeichert. Für das Gerät bleibt „System“ ein eigener Wert (er
überstimmt einen Vault-Wert „Dunkel“). Eine Rückmeldung „Gespeichert.“ bleibt als kurzer
Status, ein Fehler erscheint an der Einstellung und setzt die Anzeige auf den
gespeicherten Wert zurück.

**Begründung**: Betreiberentscheidung 2026-09-26 (Clarification, FR-021).

## R6 Download-Fortschritt überlebt die Navigation

**Entscheidung**: Ein eigener Store `stores/modelDownloads.ts`
(`useModelDownloadsStore`) hält `downloads` (Fortschritt je Modellkennung) und
`watchDownloads()`, das Fortschritt und Abschluss einmal je Vault-Session
abonniert (idempotent, ohne `stopListening` des Chats). (Umsetzung 2026-09-26:
ursprünglich in `stores/models.ts` geplant; die Datei wäre über 500 Zeilen
gewachsen.)
`pages/workspace/[instance].vue` ruft es beim Öffnen auf. Die Download-Ansichten
lesen nur noch den Store; ihre eigenen Abonnements
(`HuggingFaceModelManagement.vue`, `HuggingFaceFilePicker.vue`) entfallen.

**Begründung**: Edge Case der Spec: Ein Download läuft weiter, und die Ansicht
zeigt seinen Stand, wenn sie wieder offen ist. Heute hält die Komponente den
Fortschritt lokal und verliert ihn beim Verlassen; das Chat-Abonnement endet mit
dem Chat-Tab.

**Alternativen**: Referenzzählung pro Ansicht (verliert Ereignisse zwischen zwei
Ansichten); Fortschritt im Backend abfragen (neuer Befehl ohne Not).

## R7 Föderations-App entfällt

**Entscheidung**:

- `system.federation` verschwindet aus `WM_APPS` und `appRoutes.ts`;
  `components/apps/FederationApp.vue` und ihre Tests in
  `scripts/check-vault-lifecycle.ts` entfallen (FR-016, FR-018).
- `lib/wm/apps.ts` bekommt `LEGACY_APP_ALIASES`:
  `system.federation` → `{ appId: 'system.settings', at: '/federation' }` und
  `resolveAppAlias(appId)`. `wm.app.open` und `wm.tab.new` lösen den Alias vor
  der Prüfung auf; damit führen der Deep-Link `?open=system.federation`, Agenten
  und alte Aufrufe in die Kategorie „Föderation“ (FR-017).
- `pages/federation/[instance].vue` leitet direkt auf
  `?open=system.settings&at=/federation` um.
- Ein gespeicherter Föderations-Tab (Spec 022) wird wie jede unbekannte App beim
  Wiederherstellen verworfen (Spec 015 FR-025); das deckt der bestehende Test
  für unbekannte Apps ab.

**Begründung**: Kleinste Änderung, die jeden Weg abdeckt; die Kennung
`system.federation` bleibt für Agenten und alte Links gültig.

## R8 Farbschema

**Entscheidung**:

- Präferenz `appearance.color_scheme` mit `light`, `dark` oder `system` gilt
  für die Vault (FR-013, FR-014, FR-024). Kein Backend-Befehl: Lesen und
  Schreiben erfolgt über die vorhandenen `get_pref`/`set_pref`; die Migration
  `fold_scoped_preferences` übernimmt alte Gerätewerte beim Öffnen.
- Reines Modul `src/lib/settings/colorScheme.ts`: `parseColorScheme`,
  `isDark(scheme, systemDark)` und die Ergebnis-Projektion für `settings.get`.
- Composable `useColorScheme` (modulweiter Zustand): hält den Vault-Wert, hört
  auf `prefers-color-scheme` und setzt die Klasse `dark` an `<html>` (das Theme
  des haex-ui-Layers definiert `.dark`, `tailwind.css` hat die Variante).
- Das Plugin `plugins/colorScheme.client.ts` wendet beim Start „System“ an (vor
  dem Entsperren, FR-014); `pages/workspace/[instance].vue` lädt nach dem
  Öffnen den Vault-Wert. Ein Lesefehler lässt „System“ aktiv.
- Die Aktion `settings.appearance.setColorScheme` (`{ scheme }`) ist für
  Agenten aufrufbar (keine Leitplanke). Der Handler aktualisiert den Zustand,
  die App wechselt sofort (SC-005). `settings.get` meldet `colorScheme` mit.
- Alle Fenster des Window Managers liegen in einer Webview, eine Klasse genügt
  (US4 AS1).

**Begründung**: Die Betreiberentscheidung in R14 behandelt Einstellungen als
Eigenschaften der Vault; nur Geräte-/Hardwarewerte bleiben gerätebezogen.
Das vorhandene Theme genügt, `@nuxtjs/color-mode` wäre eine neue
Abhängigkeit für eine Klasse und eine Medienabfrage.

**Alternativen**: Farbschema im `localStorage` (liefe am Vault-Muster vorbei und
wäre vor dem Entsperren eine Spur der Vault).

## R9 Feste Farben auf Theme-Farben umstellen

**Entscheidung**: Die festen Tailwind-Farben der Vue-Dateien (beim Planen rund 170 in 28 Dateien, nach dem Umbau der Einstellungen im COSMIC-Stil noch rund 40 in 16) werden auf
die Theme-Farben des Layers umgestellt, in einem eigenen, mechanischen Commit:

| heute                                                          | neu                                                            |
| -------------------------------------------------------------- | -------------------------------------------------------------- |
| `text-neutral-500`                                             | `text-muted-foreground`                                        |
| `border-neutral-200`, `border-neutral-300`                     | `border-border` bzw. `border-input`                            |
| `text-red-500`                                                 | `text-destructive`                                             |
| `text-green-600`, `text-green-800`, `bg-green-100`             | `text-success`, `bg-success/10`                                |
| `bg-amber-500`, `text-amber-600`                               | `bg-warning`, `text-warning`                                   |
| `ring-blue-500`, `border-blue-500`, `text-blue-*`, `bg-blue-*` | `ring-ring`, `border-primary`, `text-primary`, `bg-primary/10` |
| `bg-emerald-500`                                               | `bg-success`                                                   |
| `bg-white/10`, `bg-black/10`                                   | `bg-foreground/10`                                             |
| `border-blue-500/20`, `bg-blue-100/70`                         | `border-primary/20`, `bg-primary/10`                           |

`scripts/check-vue-templates.ts` bekommt eine Sperrliste für Palettenfarben
(`(text|bg|border|ring|…)-(neutral|gray|red|green|amber|blue|emerald|white|black|…)`),
damit keine neuen dazukommen.

**Begründung**: Ohne Umstellung wäre das dunkle Schema an diesen Stellen
unlesbar (graue Schrift auf dunklem Grund, weiße Flächen); US4 AS1 verlangt die
ganze App.

**Alternativen**: `dark:`-Varianten neben jede Farbe setzen (doppelter Pflegeaufwand,
heute genau eine Stelle).

## R10 Aufteilung der Modellverwaltung

**Entscheidung**: `HuggingFaceModelManagement.vue` (580 Zeilen, Ausnahme mit
Aufteilungsplan) zerfällt entlang der Orte aus R1:

- `settings/InstalledModels.vue`: Liste, Laden, Löschen, Updates prüfen und
  installieren, Integritätsdialog.
- `settings/DownloadModels.vue`: empfohlene Modelle mit Download-Fortschritt aus
  dem Store (R6), Zeile zur Suche.
- `models/HuggingFaceSearch.vue`: liest und setzt `q` über den Tab-Router statt
  über ein Ereignis; ein Ergebnis navigiert zum Repo-Ort.
- `models/HuggingFaceFilePicker.vue`: liest `owner`/`name` aus den Routenparametern.

Die Ausnahme-Begründung am Kopf entfällt. Die gemeinsame Zustandshaltung, die den
Split bisher verhinderte (`downloadStates`, Abonnements), liegt nach R6 im Store.

**Begründung**: Complexity Tracking aus Spec 020; die Orte der Spec geben die
Schnitte vor.

## R12 Geräte der Vault (Kategorie „Föderation“)

**Entscheidung**:

- `storage/known_devices.rs` bekommt `list_devices(conn)`: alle Zeilen außer
  der internen Vault-Bereichszeile (`installation_uuid = VAULT_SCOPE_UUID`, von
  `identity::bootstrap` angelegt).
- Neuer Befehl `list_vault_devices` in `device/commands.rs`, registriert in
  `lib.rs`: `Vec<VaultDevicePayload>` mit `vaultDeviceUuid`, `alias` (`null`
  ohne Namen) und `isCurrent`; dieses Gerät zuerst, die übrigen nach Namen
  (ohne Namen zuletzt). Wire-Form camelCase wie `DeviceInfoPayload`,
  TypeScript-Typ von Hand in `useDevice.ts` (Muster des Moduls).
- Lese-Aktion `settings.devices.list` (Bereich `settings.read`, Wirkung
  `read`), damit Agenten die Liste abrufen können (FR-022).
- Ansicht `settings/FederationView.vue`: eine Zeile je Gerät mit Name oder
  „Unbenanntes Gerät“ und der Markierung „Dieses Gerät“. Kein Umbenennen hier:
  der eigene Name steht in „Allgemein“. Geladen beim Öffnen der Kategorie.
- **Nicht in 023** (Betreiberentscheidung beim Plan-Review): „zuletzt online“
  und die Live-Aktualisierung. holzi hat noch keinen Sync-Transport
  (`plans/README.md`: der Zwei-Geräte-Sync ist eine eigene Etappe nach dem MVP),
  also weder Verbindungen, deren Zeitpunkt sich messen ließe, noch ein
  Ereignis, wenn Änderungen anderer Geräte ankommen. Ein Lebenszeichen über
  synchronisierte Präferenzen wurde verworfen; beides kommt mit der Sync-Spec.
  Bis dahin sieht man andere Geräte nur in einer kopierten Vault, mit dem Stand
  der Kopie.

**Begründung**: Die Daten liegen schon in der Vault; ein Lesebefehl genügt.

**Alternativen**: Liste aus `settings.get` (vermischt Einstellungen mit einer
Geräteliste); Datum des ersten Öffnens aus `first_seen` (vom Betreiber
abgelehnt).

## R11 Tests

- **Neu `src-tauri/src/storage/known_devices_tests.rs`**: `list_devices` ohne
  Vault-Bereichszeile, Gerät ohne Namen; Sortierung im Befehl (dieses Gerät
  zuerst, dann nach Namen) als reine Funktion getestet.
- **Neu `scripts/check-settings.ts`** (`pnpm check:settings`, in CI): Register
  (eindeutige Kennungen und Pfade, jede Kategorie hat einen Ort, jeder Ort hat
  Titel und Beschreibung, Reihenfolge nach FR-005, `categoryOf` für jeden Ort),
  `settingsRoutePatterns` gegen `matchRoute`, `headerBack` (vorige Station ist
  übergeordnet → zurück, mit Query; sonst `push`; Deep-Link-Fall),
  `isDark`, `resolveAppAlias`, `tabTitleFor`, und dass
  jeder
  i18n-Schlüssel des Registers in `de.json` und `en.json` existiert (FR-020;
  holzi schaltet die Sprache zur Laufzeit nicht um, eine manuelle Prüfung auf
  Englisch ist deshalb nicht möglich).
- **Erweitert `scripts/check-vue-templates.ts`**: Sperrliste für Palettenfarben;
  die neuen Aktionen laufen durch die vorhandene Prüfung auf direkte Schreibzugriffe.
- **Angepasst `scripts/check-vault-lifecycle.ts`**: Föderations-Tests entfallen.
- **Regression**: `check:wm-state`, `check:wm-navigation`, `check:chat-state`,
  `typecheck`, `lint`, `format:check`, e2e-Suite.
- **End-to-End (Nachtrag 2026-09-27, SC-007)**: acht Szenarien
  `scripts/e2e/scenarios/settings-*.test.ts` gegen die gebaute App unter Xvfb
  (Spec 016), Zuordnung in [quickstart.md](./quickstart.md). Helfer in
  `scripts/e2e/lib/settings.ts`: Ort über `data-location` am Titel, Aktionen
  über den Aktionskatalog der Seite (`$pinia` am Vue-App-Objekt von
  `#__nuxt`), Kontrast nach WCAG über eine Leinwand gemischt, damit `oklch`
  und durchscheinende Ebenen so zählen, wie sie gemalt werden. Suchbegriffe
  sind deutsch, die Standardsprache. Nicht automatisiert: S16 (Download aus dem Netz), S18
  (Zeitmessung), die Übernahme alter Gerätewerte in S21 (Rust-Tests in
  `maintenance_tests.rs`). Entfallen (Betreiberentscheidung 2026-09-27): S13a
  mit einer kopierten Vault auf einer zweiten Installation, weil holzi noch
  keine Föderation hat und die Kopie nur die Tabellenzeilen zeigt, die die
  Rust-Tests schon abdecken; S14, weil es einen Build vor dieser Spec braucht
  und `check-wm-geometry.ts` das Verwerfen eines Tabs unbekannter App prüft.
- **Symbole ohne Netz (Nachtrag 2026-09-27)**: Die E2E-Läufe zeigten die
  Kategorie-Symbole der Seitenleiste nicht. `@nuxt/icon` bündelt nur Namen, die
  sein Scan findet, und der liest standardmäßig kein `.ts`; Register und
  App-Liste nennen ihre Symbole aber dort, die App holte sie also aus der
  iconify-API. `icon.clientBundle.scan.globInclude` umfasst jetzt `.ts`.
- **Manuell**: [quickstart.md](./quickstart.md).

## R14 Einstellungen pro Vault (FR-024)

**Entscheidung** (Betreiber-Rückmeldung 2026-09-26): Einstellungen gelten für
die Vault; die Ausnahmen Gerätename, Standard-Modell und Spracherkennungsmodell
gelten für dieses Gerät. Umsetzung:

- Ein neuer Schritt in `storage/maintenance.rs` (`fold_scoped_preferences`)
  läuft beim Öffnen: Für jeden Vault-Schlüssel wird der Wert dieses Geräts zum
  Vault-Wert, wenn die Vault keinen hat, dann wird der Gerätewert gelöscht;
  beim Standard-Modell umgekehrt (Vault-Wert zu diesem Gerät). Idempotent,
  Fehler werden geloggt und halten das Öffnen nicht auf (wie die übrigen
  Schritte).
- Sitzung wiederherstellen: `wm_session_restore_get/set` lesen und schreiben
  nur den Vault-Wert (`{ enabled }` statt Scope und Zustand je Scope);
  ausgeschaltet löscht es die gespeicherte Sitzung dieses Geräts wie bisher,
  andere Geräte löschen ihre beim nächsten Laden (Spec 022 FR-008).
- Verbotsregeln: `autonomy::get_deny_rules` liest den Vault-Wert (ohne Geräte-Parameter).
- Berechtigungsmodus des Chats (`chat.permission_mode`, Manuell/Auto/Plan): Backend (`approval_bridge.rs`, `tool_round.rs`) und Frontend lesen und schreiben den Vault-Wert.
- Zuletzt genutztes Modell (`chat.last_active_model_id`) bleibt pro Gerät wie das Standard-Modell.
- Autonomie und Aufwandsstufe: das Frontend liest und schreibt den Vault-Wert.
- Farbschema: nur der Vault-Wert; `clearColorScheme` entfällt.
- Standard-Modell: die Aktionen schreiben nur für dieses Gerät;
  `resolve_default_model` behält den Vault-Wert als letzten Rückfall, der nach
  der Übernahme leer ist.
- Aktionen verlieren ihren Parameter `scope`: `settings.sessionRestore.set`
  (`{ enabled }`), `settings.appearance.setColorScheme` (`{ scheme }`),
  `settings.models.setDefault` (`{ modelId }`), `settings.models.clearDefault`
  (`{}`); `settings.sessionRestore.clear` und
  `settings.appearance.clearColorScheme` entfallen.
- Bedienelemente aus haex-ui (`ShadcnSelect`, `ShadcnInput`, `ShadcnSwitch`,
  `ShadcnCheckbox`); haex-ui hat keine Optionsfeld-Gruppe, native Optionsfelder
  bekommen über `color-scheme` am `<html>` das passende Schema.

**Begründung**: Eine Vault ist der Arbeitsraum des Nutzers; auf jedem Gerät
dasselbe Verhalten ist einfacher als eine Wahl je Einstellung. Name und Modelle
hängen an Gerät und Hardware. Die Übernahme erhält bisherige Werte, besonders
die Verbotsregeln, die sonst auf den Standard zurückfielen.

**Alternativen**: Modelle ebenfalls für die Vault mit Hinweis zum Herunterladen
(Betreiber wählte die Ausnahme); alte Werte verwerfen (Verbotsregeln gingen
verloren); Übernahme im Frontend (liefe erst nach dem ersten Lesen des
Backends).

## R13 Suche in den Einstellungen (FR-023)

**Entscheidung**: Ein Suchfeld, das ein Such-Symbol in der Werkzeugleiste aufklappt; die Treffer stehen in der Seitenleiste. Gesucht wird in einem
reinen Modul `lib/settings/search.ts` über die Registry: Titel, Beschreibung und
Suchbegriffe jedes Orts ohne Parameter (`settings.categories.<id>.keywords`,
`settings.locations.<id>.keywords`) und die Bezeichnungen einzelner
Einstellungen (`settingKeys`, heute Gerätename und Sitzung wiederherstellen in
„Allgemein“). Treffer führen an den Ort per `router.push`; die Suche selbst ist
kein Ort.

**Begründung**: Die Registry kennt schon alle Orte und Titel; Suchbegriffe
fangen Wörter ab, die in keinem Titel stehen („Whisper“, „dunkel“). Als reines
Modul läuft die Suche in `check:settings` mit dem deutschen Katalog. Ein
Suchbegriff als Ort (`?q=`) brächte Verlaufseinträge je Tastendruck.

**Alternativen**: Die Ansichten zur Laufzeit durchsuchen (braucht gemountete
Ansichten); eine Such-Bibliothek mit unscharfer Suche (für rund 20 Einträge
unnötig).
