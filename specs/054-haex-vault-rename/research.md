# Research: Umbenennung von holzi in haex-vault

Stand: 2026-10-17, Repo auf `main` bei `3392de8f`. Gemessen mit `git grep` (ohne `specs/`, falls
nicht anders genannt).

## R1: Wie groß ist die Änderung, und wo steckt der Name?

**Befund.** Auf `main` bei `3392de8f` enthalten 1147 verfolgte Dateien „holzi“ mit 7101 Treffern;
außerhalb von `specs/` sind es 879 Dateien mit 4232 Treffern, in `specs/` 268 Dateien mit 2869
Treffern. `src-tauri` hat 3225 Treffer (2431 in `src-tauri/src`, 72 Integrationstests importieren
`holzi_lib`), `scripts` 109 Dateien (davon 267 Treffer im E2E-Rig), `src` 107 Dateien, `docs` 25,
`.github` 2, `.spaex` 3. Der Name steckt auch in **Pfaden**: das Plugin-Verzeichnis
`src-tauri/plugins/holzi-android`, die Kotlin-Verzeichnisse `com/haex/holzi` (App) und
`space/haex/holzi` (Plugin), zehn Bilddateien `holzi-*.svg` in den Icon-Sets, ein Skriptordner
`scripts/icon-sets/holzi-states`, das Spec-Verzeichnis `032-model-operates-holzi` und ein
Constitution-Verzeichnis, das vom spaex-Werkzeug erzeugt wird.

**Entscheidung.** Die Umbenennung wird als Regelwerk beschrieben (R2) und per Skript angewendet,
nicht von Hand. Das Skript ist ein Hilfsmittel der Arbeit, kein Teil des Repos.

**Alternativen.** Von Hand ersetzen (zu fehleranfällig bei 4500 Treffern); ein einziger PR (nicht
reviewbar, und jeder parallele PR kollidiert).

## R2: Welche Schreibweisen gibt es, und was wird daraus?

**Befund.** Aus den Treffern ergeben sich diese Familien: Crate und Pfade (`holzi`, `holzi_lib`),
Typen mit Präfix (`HolziError` 217 Treffer, `HolziBootstrap`, `HolziValue`, `HolziSide`,
`HolziAndroid`, `OriginKind::Holzi`), Umgebungsvariablen (`HOLZI_*`, 25 verschiedene), SQL-Namen
(`holzi_maintenance_no_sync`), Kontaktarten (`holzi_vault`, `holzi_nostr_relay`, `holzi_iroh`),
feste Texte (`holzi/…/v1`, `holzi-…/v1`, ALPNs), das Schema (`holzi-ext`), CSS-Variablen und
-Klassen (`--holzi-inset-*`, `.holzi-safe-area`), Manifest-Schlüssel (`x-holzi`), MCP-Namen
(`holzi-approve`), Plattform-Eigenschaften (`debug.holzi.probe`), Android-Pakete
(`com.haex.holzi`, `space.haex.holzi.android`) und Anzeigetexte („Holzi“, „holzi's …“).

**Entscheidung.** Die Tabelle steht in [contracts/naming-map.md](contracts/naming-map.md). Die
Grundregel: dieselbe Zeichenfolge in der Konvention der Umgebung (`holzi` → `haex-vault` oder
`haex_vault`, `HOLZI_` → `HAEX_VAULT_`, `Holzi…` → `Vault…`), mit wenigen benannten Sonderfällen.

Sonderfälle, die die Regel nicht von selbst löst:

- **Typpräfix**: `HolziError` wird `VaultError`, nicht `HaexVaultError`. `VaultError` ist im Code
  frei; `VaultGate` und ähnliche Namen existieren, kollidieren aber nicht. Jeder Typ wird
  beim Anwenden gegen vorhandene Namen geprüft; bei Kollision gilt `HaexVault…`.
- **Kontaktart `holzi_vault`** und ihre Konstante `HOLZI_VAULT` (keine Umgebungsvariable, nur in
  `contacts/kinds.rs` und deren Tests): Die Regel ergäbe `haex_vault_vault` und `HAEX_VAULT_VAULT`.
  Sie werden `haex_vault` und `HAEX_VAULT` (Entscheidung des Betreibers, 2026-10-17); `HAEX_VAULT`
  ist im Code frei. Die zwei anderen Arten folgen der Regel: `haex_vault_nostr_relay` und
  `haex_vault_iroh`. Der Name `haex_vault` meint hier eine
  Vault-Identität dieser App; die alte App spielt in dieser Liste keine Rolle (R4).
- **Anzeigename**: In der Oberfläche, im Fenstertitel und als Produktname heißt die App „HaexVault“
  (Entscheidung des Betreibers, 2026-10-21); „haex-vault“ bleibt die technische Schreibweise. Die
  Schreibweise „Haex Vault“ des Produktnamens der alten App wird nicht übernommen.

**Alternativen.** Ein „schönerer“ neuer Name pro Sonderfall (mehr Entscheidungen, mehr Review);
`Holzi…`-Typen zu `HaexVault…` (lang, ohne Gewinn).

## R3: Was bricht in einer alten Vault-Datei, wenn die Namen sich ändern?

**Befund.** Die Vault-Datei trägt Namen, die der Code erwartet. Aus den Quellen ergibt sich:

| Was in der alten Datei steht                                                                                   | Wer es liest                                                                                      | Wirkung nach der Umbenennung                                                                                                                                                                                                                                          |
| -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tabelle `holzi_maintenance_no_sync`                                                                            | `storage/maintenance.rs`, `run_pending_tasks` (`?` auf `SELECT … FROM holzi_maintenance_no_sync`) | Die Tabelle heißt in neuen Vaults anders (die Migration wird in S2 geändert, es gibt keine Nutzer). In der alten Datei fehlt der neue Name, die Abfrage schlägt fehl. Ob das nur ein Warneintrag ist oder das Öffnen verhindert, entscheidet der Aufrufer; zu prüfen. |
| Herkunft eines Kontakts `origin.kind = 'holzi'`, Kontaktarten `holzi_vault`, `holzi_nostr_relay`, `holzi_iroh` | `contacts/kinds.rs`, `ContactOriginKind`, Frontend                                                | Alte Zeilen tragen Werte, die das neue Programm nicht kennt. Betrifft Kontakte, nicht Passwörter. Zu prüfen: lesen sie sich als „unbekannt“, oder lehnt die Prüfung sie ab?                                                                                           |
| UUID der lokalen Provider (UUIDv5 aus `holzi:provider/local/…`)                                                | `providers/local.rs`                                                                              | Das neue Programm legt eigene Zeilen mit neuer UUID an; die alten bleiben als zusätzliche Provider stehen. Kosmetisch.                                                                                                                                                |
| Gespeicherte Signaturen und Schlüssel-IDs (Geräteliste, Zulassungen, Schlüssel-Umschläge, `key_id`)            | `sync/signing.rs`, `content_keys.rs`                                                              | Prüfung mit den neuen Labels scheitert. Betrifft nur den Sync.                                                                                                                                                                                                        |
| Identität                                                                                                      | `sync/keys.rs`                                                                                    | Der private Schlüssel steht fertig in `vault_identity_secret_no_sync`; nur eine **neue** Ableitung aus dem Seed ergäbe eine andere Identität. Für den Import ohne Folge.                                                                                              |
| Passwörter und ihre Tabellen                                                                                   | `passwords/*`                                                                                     | Unverändert, kein Name wird berührt.                                                                                                                                                                                                                                  |

**Entscheidung.** Wir raten nicht, wir messen. In Stufe S1 entsteht ein Testvault aus dem Stand
`holzi-final` (R6); jede Stufe lässt den Test laufen. Der Test hält fest, was nach dem Import
gelten muss: Passwörter lesbar und vollständig (FR-014, SC-003), das Öffnen bricht nicht ab, und
alles, was sich nicht prüfen lässt, ist als Zustand sichtbar statt als Abbruch (FR-016). Eine
Abweichung wird dort gelöst, wo sie auftritt (die Prüfung tolerant machen), nicht durch einen
Alias für den alten Namen. Fällt kein Weg ohne Migration im Code (FR-013), greift der Rückfall der
Spec: ein einfacher Passwort-Export vor dem Umzug (FR-014).

**Alternativen.** Die Tabelle in einer neuen Migration umbenennen (wäre eine Datenmigration im Code
und widerspräche FR-013); den alten Namen als Alias behalten (widerspräche FR-002).

## R4: Wo meint „haex-vault“ heute die alte App?

**Befund.** 40 Dateien in `src-tauri`, 87 in `specs`, je 9 in `src` und 10 in `docs` enthalten
„haex-vault“ oder `haex_vault`. Im Code sind es vor allem das Importmodul
`passwords/import/haex_vault/` mit `HaexVaultModel` und `ImportSource::HaexVault`, die Warnungen
`WARNING_CLOSE_FIRST` und `haex_vault_locked`/`haex_vault_close_first`, die Testhilfe
`tests/common/haex_vault_fixture.rs`, Testdaten `tests/fixtures/passwords/haex_vault_*` und die
Beschriftung im Importdialog. Dazu kommen 94 Textstellen („haex-vault“) und 4 Links auf
`github.com/haex-space/haex-vault`.

**Entscheidung.** S1 benennt alles um, was die alte App meint: Modul `legacy_haex_vault`, Variante
`ImportSource::LegacyHaexVault`, Testhilfen und Fixtures `legacy_haex_vault_*`, Warnkennungen
`legacy_haex_vault_*`. In Texten (Specs, Doku, Kommentare, UI) steht „haex-vault 2.x (Legacy)“.
Die vier Links auf `github.com/haex-space/haex-vault` zeigen bis S11 noch auf die alte App und
bleiben deshalb in S1 unverändert; erst nach dem Umbenennen des Legacy-Repos gibt es die Adresse
`haex-space/haex-vault-legacy`, und ab dann würde der alte Link auf die **neue** App führen. S11
stellt sie darum im selben Zug um wie die eigenen URLs (FR-021, [cutover.md](contracts/cutover.md)).
Das nach außen sichtbare Format des Imports bleibt, nur seine Kennung ändert sich. Der Wert
`ImportSource` geht über ts-rs an das Frontend; die Bindings werden neu erzeugt.

**Alternativen.** Die alten Verweise nach der Umbenennung von Hand zu trennen (ununterscheidbar,
siehe die Sätze „haex-vault wird durch haex-vault abgelöst“).

## R5: Wie wird verhindert, dass der Name zurückkehrt?

**Befund.** Die CI hat bereits einen Auftrag „Documentation consistency“ mit `check-docs.py` und
einem Test dazu (`scripts/ci/`); er ist Pflichtprüfung für `main`.

**Entscheidung.** Die Prüfung des alten Namens ist ein weiteres Skript neben `check-docs.py`
(`scripts/ci/check-old-name.py`, eigener Test), aufgerufen im selben Auftrag, damit sie ohne
Änderung der Pflichtprüfungen von `main` wirkt. Sie durchsucht verfolgte Dateien **und Pfade** und
liest ihre Ausnahmen aus einer Liste im Repo (Vertrag: [contracts/old-name-gate.md](contracts/old-name-gate.md)).
Bis S10 trägt die Liste zwei Einträge für Dinge, die erst in den Geschwister-Repos entstehen
(`.spaex/`, `.specify/memory/constitution.md`); S10 entfernt sie.

**Alternativen.** Ein Git-Hook (umgehbar); `rg` in einem Workflow-Schritt ohne Test (die Ausnahmen
wären im YAML versteckt und ohne Prüfung).

## R6: Wie entsteht der Testvault aus `holzi-final`?

**Befund.** Ein Vault erzeugen, mit Passwörtern füllen und exportieren können die vorhandenen
Befehle und Testhilfen (`instances/create.rs`, `passwords/service/*`, `tests/common`); der Import
einer Datei ist `import_instance_core`. Die Testhilfen nutzen `holzi_lib`.

**Entscheidung.** In einem Worktree auf dem Tag `holzi-final` schreibt ein einmaliger Test (nicht
eingecheckt) einen kleinen Vault: ein paar Passwort-Einträge mit Ordnern, Tags und Anhängen, einen
Kontakt mit Herkunft und Kontaktarten, eine Provider-Zeile, eine Geräteliste mit Signaturen aus dem
Sync-Setup. Passphrase und Inhalt sind erfunden und stehen in der README neben der Datei. Die Datei
heißt `pre_rename_vault.db` (kein „holzi“ im Namen, sonst fiele sie in die Prüfung aus R5), liegt
unter `src-tauri/tests/fixtures/pre_rename_vault/` und enthält den alten Namen nur verschlüsselt.
Der Test (`src-tauri/tests/pre_rename_vault.rs`) importiert die Datei und prüft: Passwörter
vollständig, Kontakt lesbar, kein Abbruch. Er läuft schon in S1 grün und ab S2 mit jeder Stufe;
der Moment, in dem er rot wird, zeigt, welche Umbenennung welchen persistierten Namen trifft.

**Alternativen.** Nur mit der echten `test.db` prüfen (nicht in CI, nicht wiederholbar, enthält
echte Passwörter); eine Datei von Hand bauen (bildet die echten Signaturen nicht nach).

## R7: Identifier, Datenordner und Android

**Befund.** `AppLocalData` (`<…>/<identifier>/`) liegt auf dem Desktop unter dem Identifier, die
Vaults unter `instances/`. Die Android-App leitet ihre Paket-ID aus dem Identifier ab. Der Code
nutzt zwei Android-Paketnamensräume: `com.haex.holzi` (App, `gen/android`) und
`space.haex.holzi.android` (Plugin); beide ändern sich. Die Tauri-Konfiguration hat keine
Updater-Einstellung.

**Entscheidung.** Identifier `space.haex.vault` überall (FR-006). `gen/android` wird mit dem
Tauri-CLI neu erzeugt oder mechanisch verschoben (`com/haex/holzi/` → `space/haex/vault/`), das
Plugin bekommt `space.haex.vault.android`; beides in S5, weil Paket, Manifest und Gradle sich
gegenseitig brauchen. Der Datenordner `space.haex.vault` ist der der alten App; der Betreiber löscht
ihn vor dem ersten Start von Hand (Spec, Edge Cases), die App prüft nichts. Es gibt keinen
Updater, also keine Umstellung.

**Alternativen.** Der Identifier der alten App `com.haex.holzi` (widerspräche FR-006); eine
Weiterleitung aus dem alten Ordner (widerspräche FR-013).

## R8: Das Schema `haex-ext://` und die Erweiterungen

**Befund.** `extensions/protocol/mod.rs` setzt `SCHEME` auf `holzi-ext`, dazu die `.localhost`-Form
für Windows und Android. Das Schema steht in der Sicherheitsrichtlinie (`tauri.conf.json`: `img-src`,
`frame-src`) und in 12 Testverweisen. Erweiterungen laufen in Iframes mit diesem Schema als Origin;
ihr eigener Browser-Speicher geht mit dem Origin verloren, Vault-Daten über das SDK nicht.

**Entscheidung.** Das Schema wird in S5 geändert, in einem PR mit der CSP. Das vault-sdk muss das
Schema selbst nicht kennen; ist das doch so, wird es in S10 geprüft. Verlorener Browser-Speicher
einer Erweiterung ist hinzunehmen (keine Nutzer).

**Alternativen.** Beide Schemas eine Weile parallel (widerspräche FR-002).

## R9: Die Geschwister-Repos

**Befund.** vault-sdk: 121 Dateien erwähnen holzi, meist Kommentare (`holzi spec 018`), dazu
exportierte Typen `ContactHolziAddresses` und `ContactHolziDevice` (`src/index.ts`) und das
Dokument `bundle-format.md` mit Verweis auf die Spec 017. atoms: 16 Dateien, darunter das Molecule
`com.github.haexmas.atoms.holzi`. spaex: 15, haextension: 10, haex-crdt: 6. Die Constitution im
Repo (`.spaex/`) wird vom spaex-Werkzeug aus einem gepinnten Stand der atoms gebaut; frühere
Erfahrung: eine Neuinstallation bringt rund 270 Zeilen Rauschen und ändert `.specify/`.

**Entscheidung.** Reihenfolge in S10: erst atoms (neues Molecule, PR dort, neuer Commit-SHA), dann
`spaex install` mit dem neuen Pin (Cross-Repo-Verweis mit vollem SHA, Constitution IV), das
Ergebnis prüfen und `.specify/` zurücksetzen, dann vault-sdk (Major, Typen umbenannt ohne Alias),
haextension gegen das neue SDK, haex-crdt (Kommentare). Jedes Repo mit eigener Branch und PR; für
das vault-sdk gilt der Account `haex-space` (Push-Rechte).

**Alternativen.** Die Constitution von Hand umschreiben (widerspräche Principle VI, Änderungen an
Anweisungen nur über Review des Werkzeugs).

## Offene Punkte

Keine NEEDS CLARIFICATION. Ein Befund ist an eine Messung gebunden (R3): wie sich die alte Datei
verhält, steht erst nach S1 und S2 fest; die Spec trägt dafür den Rückfall (Passwort-Export).
