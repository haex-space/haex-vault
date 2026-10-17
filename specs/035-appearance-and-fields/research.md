# Research: Darstellung und Eingabefelder

Alle Punkte sind entschieden; „NEEDS CLARIFICATION“ bleibt nicht offen. Stand der Quellen:
holzi `main` @ `ef4998c` plus Branch `035-ui-foundation`, haex-ui @ `2dcb8bc`.

## R1 – Wohin gehört die Darstellung?

**Entscheidung**: Eine zusätzliche Vault-Einstellung `appearance.theme` (JSON-String, Scope
`vault`); das Schema bleibt unter dem bestehenden Schlüssel `appearance.color_scheme`.

**Begründung**: `usePreferences.setPrefAsync/getPrefAsync` und die Tabelle `preferences` tragen
beliebige String-Werte und sind schon CRDT-gesynct; `pages/workspace/[instance].vue` ruft
`onVaultTablesChanged(['preferences'], …)` und liest nach einem Sync neu (Spec 024 FR-032). Das
Schema hat bereits einen Schlüssel, den e2e `settings-color-scheme` prüft; ihn umzuziehen würde
Daten und Prüfungen ohne Nutzen anfassen. Eine zweite Einstellung statt vieler einzelner hält
den Export einfach und schreibt bei „Zurücksetzen“ nur einmal.

**Verworfen**: Je Regler ein Schlüssel (mehr Schreibvorgänge, Teilzustände beim Sync, ein
Import müsste mehrere Schreibvorgänge atomar machen); Schema in den neuen Wert ziehen
(Migration ohne Nutzen).

**Folge, bewusst**: Die Darstellung ist für den Sync **eine** Einstellung (eine Zelle, die
jüngere Änderung gewinnt). Ändern zwei Geräte gleichzeitig verschiedene Regler (A den Akzent,
B den Fensterhintergrund), gilt danach auf beiden die Darstellung des jüngeren Schreibers; die
andere Änderung geht verloren. Das ist selten und sofort sichtbar, und der Preis für einen
atomaren Import und ein einziges Zurücksetzen.

## R2 – Modell: Tönung statt Absolutfarbe

**Entscheidung** (Klärung vom 2026-10-02): Fenster-, Container-, Text- und Komponententönung
sind je ein Paar aus OKLCH-Farbton `h` (0–360) und Sättigung `c` (0 bis zu einer Obergrenze je
Regler). Die **Helligkeit** kommt weiter vom Standardwert des Schemas. Eine eigene Farbe
(`<input type="color">` liefert Hex) wird nach OKLCH gewandelt, Helligkeit verworfen,
Sättigung auf die Obergrenze gekürzt. Die Akzentfarbe ist ein Farbton und eine Sättigung; ihre
Helligkeit wird aus dem Kontrast je Schema gelöst (R4).

**Begründung**: Eine Wahl gilt in Hell und Dunkel, Hell bleibt hell und Dunkel dunkel. Weil nur
Farbton und gedeckelte Sättigung wechseln, bleiben Kontraste nahe an den heute gemessenen und
sind für jede Wahl vorab berechenbar.

**Verworfen**: Absolute Farbe je Regler (Kontrast bricht beim Schemawechsel); getrennte Wahl je
Schema (doppelte Einstellungen, Export doppelt, kein Gewinn für den Nutzer, der in COSMIC
ohnehin meist einen Ton wählt).

## R3 – Farbrechnung ohne Bibliothek

**Entscheidung**: Eigene Funktionen in `src/lib/appearance/oklch.ts` und `contrast.ts`: OKLCH →
lineares sRGB (Matrizen nach Ottosson), Begrenzung auf den sRGB-Raum durch Senken der Sättigung
bei gleicher Helligkeit, Relative Luminanz und Kontrast nach WCAG 2.

**Begründung**: Die Vorab-Prüfung (`check-appearance.ts`) und die App müssen dieselbe Rechnung
benutzen, sonst misst die Prüfung etwas anderes als der Nutzer sieht. ~120 Zeilen sind
kleiner als jede Farbbibliothek (Constitution `ponytail`: keine neue Abhängigkeit, wenn es ohne
geht). Die e2e-Messung am DOM (`textContrast`, `backgroundContrast`) bleibt die unabhängige
Gegenprobe.

## R4 – Kontrast erzwingen

Gemessener Ausgangsstand des heutigen Akzents (Rechnung mit den Funktionen aus R3, oklch(L 0.17
180)): hell `L 0.65`: weißer Text auf Akzent **2,67:1**, Akzent gegen Hintergrund **2,45:1**;
dunkel `L 0.70`: schwarzer Text **8,2:1**, gegen Hintergrund **7,4:1**. Der heutige helle Akzent
erfüllt also die Spec-Grenzen (FR-016: 4,5:1 und 3:1) **nicht**.

**Entscheidung**: Die Ableitung löst für jeden Akzent und jedes Schema die Helligkeit `L` so,
dass (a) die bessere der beiden Textfarben (`0.985` weiß oder `0.145` schwarz) auf dem Akzent
mindestens 4,5:1 erreicht und (b) der Akzent gegen Fenster- und Container-Fläche mindestens 3:1
erreicht (Ring, Schalterspur, Rand), und zwar gegen jede Fläche, auf der Bedienelemente liegen:
`--background`, `--card`, `--popover`, `--sidebar` und `--muted` (die Boxen der
Einstellungsgruppen sind `bg-muted`, `components/settings/Group.vue`). Gewählt wird die kleinste
Änderung der Helligkeit gegenüber dem Ausgangswert (Farbfeld: die heutige Helligkeit des
Schemas, `0.65` hell und `0.70` dunkel; eigene Farbe: ihre eigene Helligkeit); bei Gleichstand
gewinnt die weiße Schrift. Für den heutigen Farbton liegt der Akzent im hellen Schema bei
`L ≈ 0.54–0.59` mit dunkler Schrift (bei `0.60` fällt er gegen `--background` schon auf 2,94:1)
oder `L ≤ 0.50` mit weißer; die kleinste Änderung ist die dunkle Schrift bei `L ≈ 0.59`. Im
dunklen Schema bleibt der Akzent.

**Folge, sichtbar**: Der Standardakzent bleibt Blaugrün (Farbton 180), wird aber im **hellen**
Schema dunkler (oder bekommt dunkle Schrift) als heute. Das ist die ehrliche Folge der
Kontrastgrenzen aus der Spec; die genauen Werte stehen nach der Umsetzung in `data-model.md`
(Tabelle „Standard“) und im PR.

**Weitere Standardwerte, die sich ändern** (gemessen mit derselben Rechnung):

- `--muted-foreground` hell (`0.556`) erreicht heute nur **4,15:1** gegen `--background` und
  **4,34:1** gegen `--muted`; FR-017 verlangt 4,5:1 für gedämpften Text. Neuer Standard
  `L ≈ 0.53` (4,63:1 und 4,84:1). Dunkel (`0.708`, 6,91:1 und 5,83:1) bleibt.
- `--ring` ist heute grau (hell `0.708`, dunkel `0.439`, je ~2,3:1 gegen den Hintergrund) und
  `--sidebar-primary` hell fast schwarz, dunkel blau (Farbton 264). Beide folgen künftig dem
  Akzent (`contracts/token-map.md`, FR-003, FR-015), auch im dunklen Schema.
  `--sidebar-primary*` wird in `src/` heute nirgends benutzt; sichtbar ändert sich der Ring.

**Die Standardwerte stehen in `tailwind.css`**: Die neuen Werte werden dort eingetragen, nicht nur
von `useAppearance` gesetzt, denn ohne Vault, auf dem Sperrbildschirm und nach „Zurücksetzen“
gilt `tailwind.css` (R5). `check-appearance.ts` prüft, dass `derive(Standard, Schema)` für jedes
Schema genau die Werte aus `tailwind.css` ergibt.

**Tönungen**: Obergrenze der Sättigung je Regler (Fenster `0.03`, Container `0.03`, Komponenten
`0.04`, Text `0.04`). Nach der Ableitung wird jedes Paar (Text gegen Fenster, Container und
Komponenten; gedämpfter Text; Bedienelemente gegen ihre Flächen; nicht Zierränder, FR-016)
gemessen; liegt eines unter der Grenze, wird
erst die Sättigung der verursachenden Tönung gesenkt, dann die Helligkeit des Textes
nachgeführt. Die Anpassung wird dem Nutzer gemeldet (FR-017): die Zeile zeigt „angepasst“ mit
Grund.

## R5 – Anwendung auf die Oberfläche

**Entscheidung**: `useAppearance` setzt die abgeleiteten Variablen mit
`documentElement.style.setProperty` (Inline-Stil schlägt `:root` und `.dark` in
`tailwind.css`); „Zurücksetzen“ entfernt sie. Die Standardwerte stehen weiter in `tailwind.css`
und gelten, solange kein Wert gesetzt ist (Start, Sperrbildschirm, Fehler). Wechselt das
Schema (Klasse `dark`, Systembeobachtung), rechnet `useAppearance` neu, indem es auf denselben
Beobachter hört, den `useColorScheme` schon betreibt.

**Begründung**: Alle Fenster teilen ein Webview (023); ein Satz Variablen genügt. Kein
Neuladen, kein eigenes Stylesheet, und der Standard bleibt in einer Datei sichtbar.

**Kein Zwischenspeicher im Browser**: Die Darstellung gehört zur Vault; ein lokaler
Zwischenspeicher könnte beim Vault-Wechsel die der vorherigen zeigen (Edge Case der Spec). Der
kurze Wechsel Standard → eigene Farbe beim Öffnen wird durch Warten auf `loadAsync` an der
Stelle ausgeblendet, an der das Farbschema schon gelesen wird, bevor der Arbeitsbereich
gezeichnet wird; das wird in der Umsetzung geprüft und, falls es blitzt, dort gelöst, nicht
durch Zwischenspeicherung.

## R6 – Welche Felder, was daraus wird

Bestandsaufnahme (`git grep` nach `ShadcnInput|ShadcnTextarea|ShadcnSelect|<input|<textarea|<select`):

| Bereich         | Dateien                                                                                                                                                                                                                                                 | Ziel                                                                                                                                              |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Passwortmanager | `EntryEditor` (7 Felder, 1 mehrzeilig, 1 Passwort), `GeneratorPanel` (3 Felder, `<select>`, Regler), `FolderDialog` (2), `KeyValues` (2), `Passkeys`, `TagManager`, `Attachments` (je 1), `SelectionToolbar` (`<select>` ×2, 1 Feld), `Toolbar` (Suche) | `UiInput`/`UiInputPassword`/`UiTextarea`/`UiSelect`; Suche ohne Label mit `clearable`; Regler bleibt                                              |
| Einstellungen   | `AliasSetting`, `ConnectDelegateProvider`, `ServerList`, `Toolbar` (Suche), `Select` (Zeilen-Auswahl), `OptionRow` (Optionsfeld)                                                                                                                        | Felder → `UiInput`, Suche ohne Label; **`Select.vue` bleibt** (siehe unten); Optionsfeld/Auswahlknopf bleibt nativ                                |
| Chat            | `Composer` (`<textarea>`), `ComposerControl` und `ComposerSettingsPopover` (`ShadcnSelect`, Optionsfelder), `ThreadSidebar` (Umbenennen)                                                                                                                | Auswahllisten → `UiSelect` ohne Label; Umbenennen → `UiInput` ohne Label; `Composer` bleibt (wächst mit dem Inhalt, Enter/Umschalt-Enter, Senden) |
| Modelle         | `HuggingFaceSearch`, `HuggingFaceFilePicker`                                                                                                                                                                                                            | `UiInput` (Suche, `clearable`)                                                                                                                    |
| Einrichtung     | `AliasStep`, `CreateSheet`, `LinkSheet` (3)                                                                                                                                                                                                             | `UiInput`/`UiInputPassword` mit Label                                                                                                             |

**Ausnahmen der Positivliste** (`scripts/check-fields.ts`, je mit Grund, FR-001):

1. `Composer.vue`: wachsendes Eingabefeld des Chats mit eigenem Tastaturverhalten.
2. `settings/Select.vue`: nimmt gruppierte Optionen, einen Leer-Wert („Keins“) und `data-value`
   an den Einträgen für die e2e-Prüfungen; das Label ist der Titel der Zeile. Wird aufgehoben,
   sobald haex-ui `UiSelect` das kann (Upstream-Folgeänderung A1 unten).
3. Native Schalter und Regler (`type=radio|checkbox|range|file|color`) sind keine Textfelder und
   unterliegen der Prüfung nicht.

**Upstream-Folgeänderung A1** (haex-space/haextension, klein, vor Stufe 1 oder parallel):
`UiSelect` bekommt `groups`, `data-value` an den Einträgen, Weiterreichen von `data-testid` und
Attributen an den Auslöser (damit `choose(selectHook, value)` aus `scripts/e2e/lib/settings.ts`
unverändert geht) und einen Leer-Wert. Ohne A1 bleiben `UiSelect`-Umstellungen auf Auswahllisten
ohne e2e-Hook und ohne Gruppen; Stufe 1 hängt nicht daran.

**`labelBg`**: Felder in `SettingsGroup`-Boxen liegen auf `bg-muted`
(`components/settings/Group.vue`): dort `label-bg="var(--muted)"`, auf Karten `var(--card)`, in Dialogen `var(--popover)`, auf der Seite der Standard
`var(--background)`. Eine kleine Hilfe in `src/lib/` ist nicht nötig, die Werte stehen am Feld.

## R7 – Farbwähler und Dateien, auch mobil

**Entscheidung**: Eigene Farbe über `<input type="color">` (nativ, auf Android und iOS vom
Webview als Farbwähler gezeigt) in einem Popover der „+“-Kachel, daneben ein Hex-Feld
(`UiInput`, geprüft). Export/Import über `@tauri-apps/plugin-dialog` (`save`/`open`) für den
Pfad und `@tauri-apps/plugin-fs` (`readTextFile`/`writeTextFile`, vorher `stat` für die
16-KiB-Grenze) im Frontend, wie in haex-vault 2.x (Legacy) (`useIdentityExport.ts`, `useIdentitiesActions.ts`).
Der Passwortimport in `ImportWizard.vue` liest dagegen nicht im Frontend, sondern gibt den Pfad an
einen Befehl im Backend; er ist hier kein Vorbild. Die Datei ist klein (< 2 KiB) und wird im
Speicher geprüft.

**Rechte**: `tauri-plugin-fs` steht schon in `Cargo.lock` (über `tauri-plugin-dialog`), das
JS-Paket in `package.json`; neu sind nur der direkte Eintrag in `src-tauri/Cargo.toml`, die
Registrierung in `lib.rs` und genau drei Rechte: `fs:allow-read-text-file`,
`fs:allow-write-text-file`, `fs:allow-stat`, ohne `fs:default` und ohne `fs:scope`. `open()` und
`save()` des Dialog-Plugins tragen die gewählte Datei zur Laufzeit in den Scope von `plugin-fs` ein
(`try_fs_scope().allow_file`), darum darf die App nur die Datei anfassen, die der Nutzer gewählt
hat. haex-vault 2.x erlaubt `fs:scope` mit `**` und breite Schreibrechte; das wird nicht übernommen.
Auf Android liefert die Auswahl eine Content-URI; Lesen und Schreiben dort prüft die Handprüfung.

**Verworfen**: Eigener Farbwähler (Aufwand ohne Nutzen für Stufe 2); Anzeigen und Einfügen von
Text statt Datei (der Nutzer wünscht „Export/Import“ wie COSMIC, Datei).

## R8 – Aktionen (Aktionskatalog)

Jede Bedienung steht im Aktionskatalog (Spec 020/032): `settings.appearance.set` (Teilwerte),
`settings.appearance.reset`, `settings.appearance.export` (liefert die Datei als Text),
`settings.appearance.import` (nimmt Text, prüft, wendet an). Rechte und Wirkung wie
`settings.appearance.setColorScheme` (`scope: settings.device`, `effect: write`); `reset` und
`import` sind wie der Rest der Gruppe schreibend. Der Dialog für Datei (Pfad wählen) bleibt in
der Oberfläche; die Aktion kennt nur den Text. Details in `contracts/appearance-actions.md`.

`settings.get` liefert zusätzlich `appearance` (die Darstellung), damit ein Modell im Chat sie
lesen kann.

## R9 – Fensterhinweis

`components/wm/Window.vue` setzt heute `border-foreground/40` für aktive und `border-border` für
inaktive Fenster. Mit eingeschaltetem Hinweis wird für das aktive `border-primary` gesetzt
(Akzent erfüllt gegen die Fläche 3:1 durch R4). Der Wert kommt aus `useAppearance`. Es gibt
immer nur ein aktives Fenster (`wm.focusWindow`), damit trifft FR-024 zu.

## R10 – Prüfstrategie

- `check-appearance.ts`: Farbrechnung (bekannte Werte, Rückwandlung), Ableitung (Standard =
  Werte aus `tailwind.css` in beiden Schemata, nachdem dort die geänderten Standardwerte aus R4
  eingetragen sind), **Kontrastmatrix**: jedes
  vordefinierte Farbfeld und die Extremwerte einer eigenen Farbe (Sättigung = Obergrenze, Farbton
  in 15°-Schritten) × beide Schemata × jedes Paar der Token-Tabelle (`contracts/token-map.md`);
  Datei: gültige Beispiele, jedes fehlerhafte Feld einzeln, „alles oder nichts“.
- `check-fields.ts`: sucht in `src/**/*.vue` die alten Feldarten und meldet jede Fundstelle, die
  nicht in der Positivliste steht; die Liste steht in der Datei, mit Grund je Eintrag.
- e2e: `appearance-basic` (Akzent wählen, Neustart, Zurücksetzen, Export, Import, kaputte Datei),
  `appearance-sync-two-devices` (Wechsel auf Gerät A erscheint auf Gerät B, Rig aus Spec 033),
  `fields-basic` (Label schwebt, Ring in Primärfarbe, Fehlertext, Löschen/Kopieren), erweiterte
  `settings-color-scheme` (Kontrast der Standardwerte nach R4).
- Bestehende e2e (`passwords-*`, `settings-*`, Chat, Einrichtung) laufen unverändert; ein
  Fehlschlag dort ist ein Verhaltensverlust (FR-007).
