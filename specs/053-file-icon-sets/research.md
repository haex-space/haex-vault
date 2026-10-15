# Research: Austauschbare Icon-Sets für den Dateibrowser

Stand des Codes: `main` bei `d44816eb` (2026-10-14). Lizenzprüfung am selben Tag, mit geklonten Quellen gelesen; was Einschätzung ist, steht als solche da (keine Rechtsberatung).

## R1 – Lizenzprüfung der mitgelieferten Sets (FR-017)

**Regel des Betreibers (2026-10-14)**: nur Icons und Icon-Sets mit offener Lizenz, die wir ohne Probleme nutzen können. Das ist in FR-017 der Spec als Prüfliste festgehalten: permissive Lizenz (MIT, ISC, Apache-2.0, CC0), Teile aus Drittquellen nur, wenn die Quelle benannt und ebenfalls permissiv ist, kein Copyleft. **Ausnahme, vom Betreiber bewusst angenommen**: Das Material Icon Theme enthält Logos von Firmen, Produkten, Sprachen und Werkzeugen. Nicht die MIT-Lizenz ist dabei das Problem (die Apache-2.0-Teile von Pictogrammers und Google sind zulässig), sondern dass eine Lizenz der Set-Autoren keine Marken- und Urheberrechte Dritter einräumen kann. VS Code liefert dieselben Bilder als Dateityp-Anzeige aus; das Restrisiko ist klein, aber nicht null, und der Betreiber hat es am 2026-10-14 angenommen („ich denke das Risiko können wir eingehen“). Die Lizenzseite sagt, dass Marken und Zeichen ihren Inhabern gehören und nur den Dateityp kennzeichnen. Importierte Sets sind von der Regel ausgenommen (der Nutzer bringt sie mit).

**Lizenz von holzi selbst**: `AGPL-3.0-or-later` (`src-tauri/Cargo.toml`, Zeile `license`). Im Repository liegt keine LICENSE-Datei, und `package.json` hat kein `license`-Feld; das ist eine Lücke außerhalb dieser Spec (Hinweis an den Betreiber).

Gelesen wurden die Lizenzdateien in geklonten Quellen (Stand 2026-10-14); was Einschätzung ist, steht als solche da (keine Rechtsberatung).

| Set                                    | Lizenz (belegt)                                                                                                                                                                 | Urteil nach FR-017                                                 | Grund                                                                                                                                                                                                                                                |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Lucide                                 | ISC, dazu MIT für Icons, die von Feather stammen (`LICENSE` im Repo `lucide-icons/lucide`)                                                                                      | **mitgeliefert** (unverändert, wie heute)                          | permissiv, keine Logos (`BRAND_LOGOS_STATEMENT.md`); die von uns genutzten Icons (`folder`, `file`, `file-text`, `image`, `film`, `music`, `folder-lock`, `file-lock`, `folder-key`) stehen nicht auf der Feather-Liste; beim Umsetzen erneut prüfen |
| Phosphor (`phosphor-icons/core` 2.1.1) | MIT, Copyright 2023 Phosphor Icons; keine Drittlizenzen im Repository gefunden                                                                                                  | **mitgeliefert** (kuratiert)                                       | ein Urheber, permissiv. Die Dateityp-Icons (`file-pdf`, `file-rs`, `file-vue` …) sind eigene Zeichnungen mit Buchstaben, keine Logos (geprüft an `file-rs.svg` und `file-vue.svg`); die `*-logo`-Icons (79 Dateien) werden **nicht** übernommen      |
| Material Icon Theme 5.39.0             | MIT; Icons teils von Pictogrammers (Apache-2.0, „Pictogrammers Free License“) und Google Material Symbols (nicht gelesen, Allgemeinwissen Apache-2.0); keine Pro-Icon-Nachweise | **mitgeliefert, vollständig** (Ausnahme für die Logos, siehe oben) | enthält hunderte Firmen- und Sprachlogos (`docker`, `react`, `adobe-*`); Restrisiko vom Betreiber angenommen; Lizenztexte (MIT, Apache-2.0) und Hinweis auf Marken im Paket und auf der Lizenzseite                                                  |
| Adwaita 51.0                           | Wahl „LGPL-3 oder CC-BY-SA 3.0 US“ (`COPYING`); `AUTHORS` nennt „3D Assets … CCBYSA“ für einige vollfarbige Icons                                                               | nicht mitgeliefert                                                 | Copyleft; welche Icons CC-BY-SA-Anteile haben, ist nicht belegt; CC-BY-SA 3.0 ist mit AGPL-3.0 nicht verträglich; Upstream rät von der Wiederverwendung ab                                                                                           |
| vscode-icons (Erweiterung)             | Code MIT, **Bilder CC BY-SA 4.0**, Logos unter eigener Lizenz (README)                                                                                                          | nicht mitgeliefert                                                 | Share-Alike (Copyleft); Material deckt denselben Zweck ab                                                                                                                                                                                            |
| Tabler                                 | MIT                                                                                                                                                                             | nicht mitgeliefert                                                 | zulässig, aber nur 23 Dateityp-Icons ohne Video und Audio, und über 5000 Icons (376 Marken-Icons) zum Kuratieren                                                                                                                                     |
| Catppuccin Icons                       | MIT                                                                                                                                                                             | nicht mitgeliefert                                                 | Logos (`docker`, `adobe-ps`, `vue`), Zuordnungs-JSON nicht im Repository                                                                                                                                                                             |
| Seti                                   | MIT                                                                                                                                                                             | nicht mitgeliefert                                                 | Logos, kleine Abdeckung, Icon-Font (nicht importierbar, R7)                                                                                                                                                                                          |
| Papirus                                | GPL-3                                                                                                                                                                           | ausgeschlossen                                                     | Copyleft (Betreiber-Entscheidung)                                                                                                                                                                                                                    |

**Entscheidung** (Betreiber, 2026-10-14): Mitgeliefert werden **Lucide** (Standard und Rückfall, unverändert), **Phosphor** (kuratierter Auszug der Gewichtung `regular`: Dateityp-Icons, Ordner, Schloss) und das **Material Icon Theme** (vollständig, Release 5.39.0). Auf der Seite „Lizenzen“ stehen für alle Urheber, SPDX-Kennung, Link und voller Lizenztext (ISC und MIT für Lucide, MIT für Phosphor, MIT und Apache-2.0 für Material samt dem Hinweis auf Material Design Icons, Material Symbols und die Marken); die Texte kommen aus den Paketen selbst (R6). Adwaita, vscode-icons und jedes andere VS-Code-Theme bleiben für den Nutzer importierbar.

**Begründung**: Lucide (Strich, keine Dateitypen) und Phosphor (Strich, rund 35 Dateitypen, ein Urheber) erfüllen die Regel ohne Vorbehalt; Material liefert das VS-Code-Gefühl mit Ordner- und Dateinamen und ist unter einer permissiven Lizenz, der Vorbehalt betrifft nur die Logos. Die Bilder von Phosphor sind einzelne Pfade mit `fill="currentColor"`, die sich beim Bauen leicht färben lassen (R6).

**Verworfen**: Adwaita unter der LGPL-Option (Copyleft, unklare Anteile); vscode-icons (CC BY-SA); ein logofreier Auszug des Materials (erste Fassung dieses Plans: Handauswahl aus über 1200 Icons mit Einzelfall-Entscheidungen; der Betreiber nimmt das Restrisiko der Logos stattdessen an); ein weiteres Set nur zur Auswahl (Aufwand ohne neue Fähigkeit).

**Offener Punkt für den Betreiber (kein Blocker)**: die fehlende LICENSE-Datei von holzi.

## R2 – Wie viele Sets, und ist Lucide ein Paket?

**Entscheidung**: Drei mitgelieferte Sets (R1). **Lucide ist kein Paket**: Es bleibt das, was es heute ist, die Iconify-Symbole der App, und dient als Standard-Set und als Rückfall für jede Rolle, die ein anderes Set nicht belegt. Rust antwortet für Lucide mit dem Namen des Symbols (zum Beispiel `lucide:folder`), das Frontend zeichnet es mit dem vorhandenen `<Icon>` in der Textfarbe. **Phosphor und Material sind Pakete** (R6) und erscheinen als Bild.

**Begründung**: Lucide ist schon im Client-Bundle der App (200 Symbole, 61 KB); ein zweites Paket mit denselben Bildern wäre doppelte Pflege. Die Einfärbung zur Laufzeit (CSS-Maske) wird überflüssig: Das einzige Set, das der Textfarbe folgt, läuft über Iconify; Phosphor bekommt beim Bauen feste Farben für hell und dunkel, wie es das Format der Icon-Themes von VS Code vorsieht.

**Verworfen**: Lucide als SVG-Paket mit Maske (zweite Auslieferung desselben Sets, Maske nirgends im Repository getestet); Phosphor mit Maske zur Laufzeit (zweiter Zeichenweg neben dem Bild, und importierte Sets sind ohnehin bunt).

## R3 – Wo liegen eigene Sets, und wie kommen sie atomar auf andere Geräte?

**Entscheidung**: Ein eigenes Set ist **eine Zeile** in einer neuen CRDT-Tabelle `haex_file_icon_sets`; die Bilder und das Manifest liegen als ZIP-Paket (gepackt, höchstens 5 MiB entpackt) in einer BLOB-Spalte derselben Zeile ([data-model.md](./data-model.md)). Migration `0035_file_icon_sets` (nächste freie Nummer auf `main`: letzte ist `0034_contacts_phone_known`; beim Umsetzen gegen parallele Branches prüfen) und eine Erhöhung von `HOLZI_TRIGGER_VERSION` (heute 22).

**Begründung**: Der Sync überträgt eine Transaktionsgruppe erst vollständig angewendet (Spec 024, `inbound`), und ein einzelner Wert, der größer als eine Seite ist, reist in Teilen (`Change.continues`, `sync/change.rs`). Eine Zeile mit einem BLOB ist damit von selbst „erscheint erst vollständig“ (FR-015); die im Code fehlende allgemeine Markierung „Datensatz vollständig“ (Bericht zum Sync) wird nicht gebraucht. Sets sind unveränderlich (Ersetzen = neue Zeile, alte löschen), also gibt es kein feldweises Zusammenführen zweier Versionen. Ein einzelnes Löschen lässt einen Löschmarker (`haex_deleted_rows`), der auf allen Geräten gilt (Spec 024 FR-017); es gibt keine Kind-Zeilen, die vor dem Elternteil gelöscht werden müssten.

**Verworfen**: Zeile je Bild mit Kopfzeile zuletzt und Prüfung beim Lesen (Muster der Erweiterungs-Bundles `extension_blobs`): bis zu 2000 Zeilen je Set, eigener Vollständigkeits-Test, ein Fehlerbild, das hier unnötig ist. Ein Set als Preference (wie das Hintergrundbild, `preferences`, 4 MiB): das Hintergrund-Muster nennt selbst den Upgrade-Pfad „eigene Tabelle“. Ein Ordner im Dateisystem des Geräts: wird nicht synchronisiert.

`ponytail:` Jede Änderung würde das ganze Paket neu senden. Das ist hier kein Problem, weil Sets unveränderlich sind; ändert sich das (Umbenennen eines Sets im Paket), wäre die Obergrenze erreicht, und die Bilder bekämen eigene Zeilen.

## R4 – Auswahl und Anzeige, Gerät und Vault

**Entscheidung**: Die Auswahl ist die Vault-Preference `files.iconSet` (Scope Vault, nicht Gerät). Werte: `builtin:lucide`, `builtin:phosphor`, `builtin:material` oder `custom:<uuid>`. Die Form wird in `validate_value` geprüft (`storage/preferences_commands.rs`, neben `files.view`). Fehlt das gewählte Set (entfernt, noch nicht angekommen, Set einer neueren App-Version) oder ist es beschädigt, gilt Lucide und die Einstellungen nennen den Grund (FR-020). Das Frontend lädt bei Änderungen von `preferences` und `haex_file_icon_sets` neu (`onVaultTablesChanged`; der Tabellenname kommt in `vault_events.rs` dazu).

**Begründung**: `preferences` hat feldweises „jüngere Änderung gewinnt“ (HLC), also gilt bei zwei Geräten am Ende überall dieselbe Wahl (US4 Szenario 5, wie `appearance-sync-two-devices`). `files.view`, `files.hidden` und `files.sort` sind Geräte-Einstellungen; das Icon-Set ist ausdrücklich eine Vault-Einstellung (Klärung 2).

**Agenten und Erweiterungen (FR-016)**: Es wird keine `settings.*`-Aktion für das Icon-Set angelegt, also kann ein Agent es nicht ändern. `settings.get` zählt seine Felder ausdrücklich auf (Name, Sprache, Farbschema, Aussehen, Modelle und so weiter) und gibt keine beliebigen Preferences zurück; ein Test hält das fest. Ob die Brücke für Erweiterungen beliebige Preference-Schlüssel liest, ist im Bestand nicht belegt; Aufgabe: prüfen und per Test festhalten, dass `files.iconSet` und die Icon-Set-Befehle für Erweiterungen und Agenten nicht erreichbar sind.

## R5 – Auflösung nur in Rust, Rückgabe fertig an das Frontend

**Entscheidung**: Ein Befehl `files_icons_resolve(set, scheme, queries)` bekommt je Eintrag nur das, was er braucht (Name, Ordner oder Datei, Zugriff, Zustand der Verschlüsselung) und liefert je Eintrag eine fertige Antwort: entweder ein Bild (`id`, `mime`) oder ein Rückfall-Symbol (`lucide:…`). Die Rolle eines Eintrags (Ordner, Ordner ohne Zugriff, Bild, Video, Audio, PDF, Text, Datei, die Zustände aus Spec 048) und die Reihenfolge der Spec (Name, Endung, Rolle, Standard-Set; FR-003) stehen nur im neuen Rust-Modul. Die Bilder holt das Frontend mit `files_icon_image(set, id, scheme)` als rohe Bytes und zeichnet sie als Bild (`blob:`-URL, Muster `files_thumbnail` und `passwordsIcons`).

**Begründung**: „Manifest-Auflösung nur in Rust“ ist die Vorgabe (eine Umsetzung je Regel); heute stehen die Rollen doppelt, in `kind.rs` und in `viewerKind.ts`, und `encryptedLook` wählt Symbole im Frontend. Mit dem neuen Befehl fällt die Symbol-Wahl aus `EntryIcon.vue` und `encrypted.ts` weg; `encryptedLook` behält nur den Zustandstext. `viewerKind.ts` bleibt, weil die Vorschau und der Betrachter es brauchen. Bilder werden nach `id` dedupliziert, weil viele Dateien dasselbe Bild haben.

**Anfragen in Gruppen**: Ein Composable `useFileIcons` sammelt die Anfragen der sichtbaren Zeilen in einem Frame, fragt einmal und merkt sich Antwort (je Set, Farbschema, Name, Art, Zustand) und `blob:`-URL (je Set, Bild-`id`, Farbschema) in einem begrenzten Speicher (Muster `thumbnailCache.ts`, vorhanden). Bis die Antwort da ist, zeigt `EntryIcon.vue` einen leeren Platz derselben Größe (kein Springen); bei einem Treffer im Speicher gibt es keinen Umweg. Bei Wechsel von Set oder Farbschema wird der Speicher geleert (FR-002).

**Farbschema**: Rust bekommt `scheme` (`light` oder `dark`) mit. Im Frontend gibt es heute nur die Klasse `dark` und `onColorSchemeApplied`, kein reaktives Hell/Dunkel; ein kleines Composable `useAppliedScheme()` macht daraus ein Ref.

**Verworfen**: Das Manifest als JSON ins Frontend schicken und dort nachschlagen (zweite Umsetzung der Regel, 4659 Ordnernamen und 2148 Dateinamen im Speicher des Webviews); ein Befehl je Eintrag (IPC je Zeile bei 1000 Einträgen); Bilder als `data:`-URL in der Antwort (Antworten werden groß, keine Dedublizierung).

## R6 – Pakete: Format, mitgelieferte Sets Phosphor und Material, Auslieferung

**Entscheidung**: Ein Paket ist ein ZIP mit dem Manifest (`icon-theme.json`), den Bildern und einer Datei `LICENSE`. Das Manifest ist das Format der Icon-Themes von VS Code, ergänzt um einen Block `x-holzi` mit `roles` (Rolle → Bild-Kennung) und `origin` (Urheber, SPDX-Lizenz, Quelle). Ein Theme ohne `x-holzi` gilt ohne Rollen; seine `file`- und `folder`-Vorgaben übernehmen deren Aufgabe (R7).

**Herstellung der mitgelieferten Pakete** (ein Weg für beide): Ein Skript `scripts/icon-sets/prepare.ts` (läuft mit `node` in der Devshell, keine neue Abhängigkeit) liest die gepinnte Quelle und schreibt ein **Quellverzeichnis** `src-tauri/icon-sets/<set>/` (Manifest, ausgewählte Bilder, Lizenztexte), das eingecheckt wird und in jedem PR prüfbar ist. Gepackt wird es vom selben Rust-Code, den der Import für einen Ordner benutzt (`files/icons/import.rs`): ein Befehl (ignorierter Test oder Beispiel, Entscheidung beim Umsetzen) schreibt `src-tauri/icon-sets/<set>.zip` mit festen Zeitstempeln und sortierten Einträgen; ein normaler Test packt das Quellverzeichnis und vergleicht Byte für Byte mit dem eingecheckten ZIP (reproduzierbar). Das ZIP ist mit `include_bytes!` in die App eingebunden (kein Resource-Resolver und kein Unterschied zu Android; im Bestand ist `bundle.resources` leer und kein Android-Asset vorhanden). `src-tauri/icon-sets/SOURCES.md` nennt je Set Quelle, Version, Commit-SHA oder Prüfsumme.

**Phosphor** (Quelle `phosphor-icons/core` 2.1.1, Gewichtung `regular`):

1. Aus `assets/regular` wird nur eine feste Liste übernommen (Dateityp-Icons `file-*`, `folder`, `folder-lock`, `folder-simple`, `folder-simple-lock`, `file`, `file-lock`, `file-x`, `file-dashed`); alle `*-logo`-Icons bleiben draußen. Die Liste steht im Repository (`src-tauri/icon-sets/phosphor.icons.txt`), damit jede Aufnahme eine Entscheidung im Diff ist.
2. Jedes Bild wird in zwei Fassungen geschrieben: `currentColor` wird durch eine feste helle Farbe (dunkles Schema) und eine feste dunkle Farbe (helles Schema) ersetzt. Das Manifest führt im Hauptteil die Fassung mit hellen Strichen (für das dunkle Schema, wie bei VS Code, dessen Hauptteil das dunkle Theme ist) und im Block `light` die Fassung mit dunklen Strichen (für das helle Schema). Die Farben stehen im Skript (Vorgabe: Grautöne, die zu `text-muted-foreground` passen).
3. Das Manifest ordnet Endungen den vorhandenen Bildern zu (`src-tauri/icon-sets/phosphor.mapping.json`, von Hand geschrieben): zum Beispiel `pdf` → `file-pdf`, `doc`, `docx`, `odt` → `file-doc`, `xls`, `xlsx`, `ods` → `file-xls`, `ppt`, `pptx` → `file-ppt`, `zip`, `7z`, `tar`, `gz` → `file-zip`, `png` → `file-png`, `jpg`, `jpeg` → `file-jpg`, `rs` → `file-rs`, `ts` → `file-ts`, `py` → `file-py`; Sprachen ohne eigenes Bild (`go`, `kt`, `java`, `rb`, `php`, `sh`) → `file-code`. Phosphor hat keine Namen für Dateien und Ordner, also gibt es dort keine; Rollen (`x-holzi.roles`) belegen Bild, Video, Audio, PDF, Text, Ordner, Ordner ohne Zugriff, die Schloss-Zustände (`folder-lock`, `file-lock`) und beschädigt (`file-x`).
4. `LICENSE` im Paket ist der MIT-Text von Phosphor, `origin` nennt Urheber, SPDX `MIT` und die Quelle.

Größe: alle `file*` und `folder*` der Gewichtung `regular` sind zusammen rund 35 KB roh; mit zwei Fassungen und Packen weit unter 0,2 MB (beim Packen messen).

**Material** (Quelle: das Release-VSIX 5.39.0 des Material Icon Themes mit `extension/dist/material-icons.json` und `extension/icons/`; das Git-Repository reicht nicht, weil `file.svg`, `folder.svg` und `folder-open.svg` erst beim Build entstehen; URL und SHA-256 des VSIX stehen in `SOURCES.md`):

1. `prepare.ts` übernimmt **alle** Bilder (1251 SVG, rund 1,0 MB roh, davon 55 `*_light.svg`) und das Manifest, und streicht nur die Teile, die wir ohnehin ignorieren (`folderNamesExpanded`, `rootFolder*`, `rootFolderNamesExpanded`, `languageIds`, `highContrast`, `hidesExplorerArrows`), damit das Paket klein bleibt (das Manifest hat 451 KB, davon der größte Teil `folderNamesExpanded`). Die Pfade `./../icons/x.svg` werden auf `icons/x.svg` normiert. Es gibt keine Auswahlliste und keine Prüfung auf Logos.
2. **Lizenzen im Paket**: `LICENSE` ist der MIT-Text des Material Icon Themes; dazu der Text der Apache-2.0 und eine Datei `NOTICE` mit „enthält Icons aus Material Design Icons (Pictogrammers) und Material Symbols (Google), Apache-2.0; die Logos und Zeichen von Firmen, Produkten, Sprachen und Werkzeugen gehören ihren Inhabern und kennzeichnen nur den Dateityp“. `origin` nennt das alles; die Lizenzseite zeigt es.
3. **Hell/Dunkel**: Das Theme bringt eigene `*_light.svg` für einige Bilder und den Block `light`; beides bleibt, wie es ist.
4. **Prüfung im Test**: Das Paket besteht dieselbe Prüfung wie ein Import (Pfade, Größen, SVG-Bereinigung); jedes SVG des Pakets übersteht die Bereinigung ohne Fehler, und die Zahl der Bilder, Namen und Endungen entspricht der Quelle (Schutz vor einem Fehler beim Filtern).

Größe: das ZIP mit 1251 SVG und dem gekürzten Manifest ist nach Schätzung unter 0,5 MB (SVG komprimieren gut); beim Packen messen. Die Grenzen für eigene Sets (2000 Bilder, 256 KiB je Bild, 5 MiB entpackt) erfüllt das Paket ebenfalls.

**Begründung**: Ein Format für mitgelieferte und eigene Sets heißt ein Lader, ein Gültigkeitstest und eine Lizenzseite, die die `LICENSE`-Dateien und `origin` der Pakete liest. Der Test „jedes mitgelieferte Paket besteht die Prüfung des Imports“ (`cargo test`) hält beide gleich. Das Quellverzeichnis im Repository macht jede Änderung an einem mitgelieferten Bild im Diff sichtbar; das ZIP ist ein reproduzierbares Ergebnis davon, keine zweite Quelle der Wahrheit. Feste Farben beim Bauen halten die Laufzeit bei einem einzigen Zeichenweg (ein Bild).

**Verworfen**: nur das ZIP einchecken (die Auswahl wäre im Diff unsichtbar); ein zweiter Packer in Skript-Sprache (zweite Umsetzung des Paketformats, kein ZIP-Schreiber im Bestand); Bilder in `public/` (im Frontend-Bundle, am Rust-Lader vorbei, auf Android nicht geprüft); Download beim ersten Start (Offline, Spec 043); `currentColor` im Paket lassen und per CSS-Maske zeichnen (zweiter Weg, R2).

## R7 – Zuordnung von Dateien zu Bildern (Manifest lesen)

**Entscheidung** (Format nach dem `icon-theme.json` von VS Code, geprüft an der Datei `material-icons.json` des Release 5.39.0 des Material Icon Themes; sie ist zugleich das Beispiel eines großen importierten Themes):

- `iconDefinitions`: Bild-Kennung → `iconPath` (relativ zum Manifest; `./../icons/x.svg` wird beim Lesen auf einen Pfad im Paket normiert). Definitionen mit `fontCharacter` (Icon-Fonts wie bei Seti) werden ignoriert.
- Dateiname (kleingeschrieben): `fileNames`. Endung: `fileExtensions`, probiert von der längsten Endung nach dem ersten Punkt zur kürzesten (`a.test.ts` → `test.ts`, dann `ts`), wie VS Code. Ordnername (kleingeschrieben): `folderNames`.
- Vorgaben: `file` und `folder`. `folderExpanded`, `folderNamesExpanded`, `rootFolder*`, `languageIds`, `highContrast` und `hidesExplorerArrows` werden ignoriert (Klärung 2: kein geöffneter Ordner).
- Hell: Der Block `light` überschreibt `file`, `folder`, `fileNames`, `fileExtensions` und `folderNames` für das helle Schema; je Schritt zählt zuerst `light`, dann der Hauptteil (wie VS Code). Beim Material-Theme sind es 31 Endungen, 179 Dateinamen und 25 Ordnernamen (Dateien `*_light.svg`); die Vorgaben `file` und `folder` haben keinen Hell-Override.

**Reihenfolge der Auflösung** (FR-003, FR-005; eine Funktion in `icons/resolve.rs`):

0. Zustände zuerst: Ist der Eintrag ein verschlüsselter Ordner (lesbar), nicht lesbar, beschädigt oder ein Ordner ohne Zugriff, gilt `x-holzi.roles[zustand]`, sonst gleich das Standard-Set (Schritt 5). Ein Dateityp-Bild verdeckt einen Zustand nie.
1. Ordner: `folderNames[name]`; Datei: `fileNames[name]`.
2. Datei: `fileExtensions` (längste Endung zuerst).
3. Rolle: `x-holzi.roles[rolle]` für Bild, Video, Audio, PDF und Text (Phosphor trägt sie; Material und große importierte Themes brauchen sie nicht, weil ihre Endungen und Namen fast alles abdecken).
4. Vorgabe des Themes: `file` oder `folder`.
5. Standard-Set (Lucide: Tabelle Rolle → Symbol in Rust, übernommen aus dem heutigen `EntryIcon.vue` und `encrypted.ts`).

**Begründung**: Das ist die Reihenfolge der Spec, mit dem `file`/`folder`-Vorgabewert des VS-Code-Formats an der Stelle „Rolle Datei“. Ein importiertes Theme zeigt so für unbekannte Dateien sein eigenes Datei-Bild, nicht ein Lucide-Symbol.

## R8 – Rollen aus dem Eintrag

**Entscheidung**: `icons/role.rs` bildet den Eintrag auf eine Rolle ab: `viewer_kind(name)` aus `files/kind.rs` (Bild, Video, Audio, PDF, Text, sonst Datei), `Entry.no_access` (nur Ordner), `EncryptedInfo` (lesbarer verschlüsselter Ordner, nicht lesbarer Ordner oder Datei, beschädigt). `kind.rs` wird nicht verändert; „Archiv“ und „Programmcode“ sind keine Rollen (Dateityp-Bilder kommen aus `fileExtensions`).

**Begründung**: Die Rollen sind heute genau die Fälle, die `EntryIcon.vue` unterscheidet; neue Rollen ohne Anzeigeort wären Aufwand ohne Wert (YAGNI).

## R9 – SVG bereinigen (FR-010, FR-011)

**Entscheidung**: Neues Modul `icons/svg.rs` auf `quick-xml` (im Bestand, 0.41; im Lock gibt es kein `usvg`, `resvg`, `roxmltree` oder `ammonia`, und keine SVG-Bereinigung im Code). Es liest den Strom der Ereignisse und schreibt nur, was auf der **Erlaubnisliste** steht:

- Elemente: `svg`, `g`, `defs`, `title`, `desc`, `symbol`, `use`, `path`, `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, `linearGradient`, `radialGradient`, `stop`, `clipPath`, `mask`, `pattern`, `filter` samt den `fe*`-Primitiven, `image` nur mit `data:`-Bild (PNG/WebP). Alles andere (`script`, `foreignObject`, `animate*`, `set`, `a`, `style`, `iframe` …) wird samt Inhalt verworfen.
- Attribute: Erlaubnisliste von Darstellungsattributen; alle `on*`-Attribute, `href` und `xlink:href` (nur ein Verweis auf `#…` im selben Bild bleibt), `style` (nur Deklarationen ohne `url(`, `@import` und `expression`) und jeder Wert mit `url(` auf etwas anderes als `#…` entfallen.
- DOCTYPE, Entitäten, Verarbeitungsanweisungen und CDATA mit Markup werden abgelehnt (keine Entitäts-Expansion). Tiefe und Größe des Baums sind begrenzt.

Die Ausgabe wird neu geschrieben (nicht Text ersetzt). Die Funktion ist idempotent und läuft an zwei Stellen: beim Import (gespeichert wird nur die bereinigte Fassung) und beim Ausliefern eines Bildes an das Frontend, einmal je Bild und Set (Ergebnis im Speicher); das ist dieselbe Funktion, keine zweite Umsetzung.

**Begründung**: Eine Erlaubnisliste ist gegenüber einer Sperrliste vollständig im Sinne von „unbekanntes fällt weg“; neue SVG-Features sind harmlos ausgeschlossen. `<style>` entfällt ganz, weil `@import url()` und externe `url()` sonst Daten nach außen schicken könnten; die Bilder von Phosphor sind einzelne Pfade ohne `<style>`, und große importierte Themes wie Material nutzen überwiegend Attribute (beim Umsetzen an den Fixtures zählen, wie viele Bilder eines großen Themes ein `<style>` brauchen; Aufgabe). Die Anzeige verlässt sich zusätzlich auf FR-010 (nur als Bild, nie inline).

**Empfangende Geräte (Frage aus der Klärung)**: Ein per Sync angekommenes Set wird nicht blind übernommen. Beim ersten Öffnen prüft derselbe Lader Aufbau, Limits und SHA-256 (Spalte `sha256` gegen das Paket) und liefert jedes SVG erst nach der Bereinigung aus. Fällt die Prüfung durch, gilt das Set als beschädigt: Lucide bleibt aktiv, die Einstellungen zeigen „Set beschädigt“ und bieten „Entfernen“. Das schützt vor einer älteren App-Version auf einem anderen Gerät, vor einer beschädigten Übertragung und vor einem Paket aus anderer Quelle in der Vault.

**Verworfen**: Das SVG als Text mit regulären Ausdrücken bereinigen (bekannte Umgehungen, kein Strukturwissen); `<style>` zulassen und nur `url()` filtern (CSS-Parser nötig, neue Abhängigkeit); ein SVG-Renderer wie `resvg` zum Umwandeln in PNG (große neue Abhängigkeit, verliert die Vektor-Qualität).

## R10 – PNG und WebP prüfen

**Entscheidung**: Mit `image` (0.25, Features `png` und `webp` sind an): lesen mit `Limits` (höchstens 512 × 512 px, begrenzter Speicher), das Format muss zur Endung passen, beschädigte Bilder werden abgelehnt. Gespeichert wird das Original (keine Neu-Kodierung). Andere Formate (GIF, JPEG, BMP …) gelten als nicht lesbar und stehen im Bericht des Imports (US3 Szenario 4).

## R11 – Import: Quellen, Ablauf, Android

**Entscheidung**: Der Import hat zwei Schritte wie die Installation einer Erweiterung (`extension_install_preview` und `extension_install`): `file_icon_set_import_preview` liest Quelle und Prüfung, schreibt nichts und liefert Name, Zahl der Bilder, Größe, ausgelassene Dateien mit Grund und, ob ein Set gleichen Namens existiert. `file_icon_set_import` liest noch einmal, schreibt die Zeile und ersetzt auf Wunsch das gleichnamige Set (neue Zeile, dann alte löschen).

- **Quellen**: ein Ordner oder ein Archiv (zip). Das Manifest wird gefunden als `icon-theme.json` im Wurzelverzeichnis, oder über `package.json` einer VS-Code-Erweiterung (`contributes.iconThemes[0].path`, auch im VSIX unter `extension/`); daher lässt sich ein VSIX direkt importieren. Name, Urheber und Lizenz kommen aus `package.json`, sonst aus dem Manifest-Namen oder dem Dateinamen.
- **Datei wählen**: `usePickedFile` und der Typ `PickedFile` sind vorhanden (auf Android `content://` ohne Dateifilter, `files/picked.rs`).
- **Ordner auf Android**: gibt es nicht (`folder_pick` ist dort `false`, das Plugin hat keinen Ordner-Befehl, Spec 043 FR-016). Auf dem Telefon geht nur das Archiv; „Ordner wählen“ wird dort wie bei den Erweiterungen ausgeblendet (`useDeviceCapabilities`). Das ändert FR-009 und US3 der Spec (siehe Spec-Änderung unten).
- **Pfadregeln und Archive**: `haex_bundle::format::path_rule_violation` und `PathChecker` sind öffentlich und generisch (leer, nicht NFC, Steuerzeichen, `\`, `:`, absolut, `..`, zu lange Segmente, Duplikate auch nach Normalisierung) und werden für Pfade im Archiv und im Ordner wiederverwendet; das `.xt`-spezifische `read_archive` wird nicht benutzt (sein Verhalten ohne Manifest und Signatur ist nicht geprüft; Aufgabe: kurz prüfen, ob es ein einfaches ZIP liest, sonst `zip` 8 mit eigener Buchführung). Gelesen wird im Speicher, nie auf die Platte entpackt; Verzeichniseinträge, Symlinks und Nicht-Regulärdateien werden abgelehnt; Gesamtgröße, Anzahl, Größe je Datei und das Verhältnis gepackt/entpackt sind begrenzt (FR-013).
- **Grenzen** (Vorgabe der Annahmen der Spec): 5 MiB je Set entpackt, 2000 Bilder, 256 KiB je Bild, 512 × 512 px für Pixelbilder, 10 eigene Sets, Archiv höchstens 8 MiB gepackt.

**Verworfen**: Zustand zwischen Vorschau und Schreiben in der App halten (Ablauf und Aufräumen, wenn der Nutzer abbricht); die Datei lesen und dem Frontend zur Prüfung geben (zweite Umsetzung der Regeln).

## R12 – Einstellungen und Seite „Lizenzen“

**Entscheidung**: In `AppearanceView.vue` kommt eine Gruppe „Icon-Set für Dateien“ (`SettingsGroup` im Modus `radio`, je Set eine `SettingsOptionRow` mit Vorschau, Name und Urheber/Lizenz; Wahl wird beim Anklicken gespeichert wie bei `ColorSchemeSetting.vue`, Bestätigung „Gespeichert.“, `settings-save-on-selection`). Dazu „Eigenes Icon-Set hinzufügen“ und je eigenem Set „Entfernen“ (mit `ShadcnAlertDialog`) und „Zurücksetzen“ auf Lucide. Die Vorschau zeichnet einen Beispielordner über denselben Befehl `files_icons_resolve` (feste Beispiel-Anfragen: Ordner, Bild, Video, Musik, PDF, Text, Programmcode, Archiv). Die Seite „Lizenzen“ wird eine eigene Unterseite in den Einstellungen (Allgemein, `registry.ts`, `SETTINGS_VIEWS` in `appRoutes.ts`) und liest über `file_icon_set_licenses` die `LICENSE`-Dateien und `origin` der mitgelieferten Pakete (zusätzlich Lucide, dessen Texte als Text in der App stehen).

Texte: `de.json` und `en.json` (nur diese Sprachen), Begriff „Icon-Set“.

## R13 – Leistung und Größe (FR-018, SC-004)

**Entscheidung**: Ein Set wird beim ersten Gebrauch geöffnet (Manifest lesen, Tabellen Name → Bild bauen; Material hat 4659 Ordnernamen, 2148 Dateinamen und 1381 Endungen, Phosphor nur einige Dutzend Endungen) und als geteiltes Objekt im Speicher gehalten (Schlüssel: Set und SHA-256). Bilder werden nicht beim Öffnen entpackt, sondern beim ersten Abruf (aus dem ZIP, bereinigt, im Speicher begrenzt gehalten). Das JS-Bundle wächst nicht (die mitgelieferten Pakete liegen in der Rust-Binärdatei, zusammen deutlich unter 0,5 MB; beim Packen messen; eigene Sets liegen in der Vault, nicht in der Binärdatei).

**Zielwerte**: Eine Gruppe von 200 sichtbaren Einträgen in unter 20 ms antworten, ein Set in unter 100 ms öffnen (beim Packen und im Test messen; kein Wert ist im Bestand belegt).

## R14 – Test und Fixtures

**Entscheidung**: Testdateien getrennt von der Produktion (`*_tests.rs`). Fixtures unter `src-tauri/tests/fixtures/icon_sets/`: ein gültiges VS-Code-Theme (klein, mit `light`-Block), ein SVG mit Skript, `onload`, `foreignObject`, `href` auf eine fremde Adresse und `url()`; ein Archiv mit `../`-Pfaden, ein Archiv mit sehr hohem Verhältnis (Bombe), ein Manifest mit Verweisen aus dem Ordner heraus, ein beschädigtes PNG und eines über 512 px. E2E: `files-icon-sets` (wählen, Bilder erscheinen, Neustart behält die Wahl, Import, Entfernen) und `files-icon-sets-two-devices` nach dem Muster `appearance-sync-two-devices`.
