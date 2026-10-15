# Quickstart: Austauschbare Icon-Sets für den Dateibrowser

Prüfpfade für die Umsetzung. Befehle laufen in der Devshell (`nix develop --command …`); Cargo zusätzlich über `scripts/with-nix-host-bridge.sh` (siehe die Regel im holzi-Atom). Nur die berührten Ziele bauen und testen, `-j 4`.

## 1. Automatische Prüfungen

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib files::icons
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib identity::migrations_icon_sets
nix develop --command node --test scripts/check-file-icons.ts
nix develop --command pnpm exec prettier --check specs/053-file-icon-sets
```

Erwartung: grün. Die Tests nennen das Verhalten, zum Beispiel `a_script_in_a_svg_is_not_stored`, `a_theme_without_a_role_uses_its_file_default`, `a_state_icon_is_never_covered_by_an_extension`, `an_archive_path_outside_the_target_is_refused`, `a_pack_with_a_wrong_hash_is_damaged`, `every_bundled_pack_passes_the_import_checks`.

Die Fixtures liegen in `src-tauri/tests/fixtures/icon_sets/` (gültiges Theme, SVG mit Skript, Archiv mit `../`, Bombe, beschädigtes PNG, Bild über 512 px).

## 2. E2E `files-icon-sets` (Desktop)

```sh
pnpm test:e2e --grep files-icon-sets
```

Das Szenario deckt US1, US2, US3 und US5 ab:

1. Einstellungen → Gestaltung: die Gruppe „Icon-Set für Dateien“ zeigt Material (gewählt), Phosphor und Lucide mit Vorschau.
2. „Phosphor“ anklicken, im Dateibrowser ein Verzeichnis mit `bericht.pdf`, `main.rs`, `foto.png`, `daten.csv` und einem unbekannten Dateityp öffnen: die bekannten Endungen haben je ihr Phosphor-Bild (als `img`, nicht als `svg` im DOM), der unbekannte Typ das Datei-Bild des Sets, „Gespeichert.“ ist zu sehen, kein Speichern-Knopf. Im hellen und im dunklen Farbschema sind die Bilder lesbar (zwei Fassungen).
3. App neu starten: das Set ist noch gewählt.
4. Ein eigenes Set aus dem Fixture-Ordner importieren (über das Eingabefeld für Dateien, wie `BackgroundSetting`): die Vorschau zeigt Name, Zahl der Bilder und den Hinweis zur Lizenz; nach dem Import erscheint das Set mit Vorschau; ein SVG im Set mit Skript ist beim Anzeigen wirkungslos, und im Speicher liegt die bereinigte Fassung.
5. Entfernen mit Bestätigung: das Set ist weg, die Wahl fällt auf Lucide zurück.
6. Ein Verzeichnis mit einer verschlüsselten, nicht lesbaren Datei und einem Ordner ohne Zugriff: Schloss und Ordner-ohne-Zugriff erscheinen auch unter Phosphor und unter einem importierten Set ohne diese Rollen (dort aus dem Standard-Set, FR-005).
7. „Material“ wählen, ein Verzeichnis mit `src`, `docs`, `images`, `README.md`, `LICENSE`, `data.json`, `main.rs` und `Dockerfile` öffnen: Ordner, Dateien nach Name und Endung haben je ihr Material-Bild.
8. Test „Lizenzregel“: jedes mitgelieferte Paket hat `LICENSE` und `origin`, die SPDX-Kennung steht in der Liste erlaubter (MIT, ISC, Apache-2.0, CC0); das Material-Paket hat zusätzlich Apache-2.0 und `NOTICE` mit dem Hinweis auf Marken (FR-017); das eingecheckte ZIP enthält dieselben Dateien (Namen und Inhalt) wie das Quellverzeichnis.

## 3. E2E `files-icon-sets-two-devices` (Spec 033)

```sh
pnpm test:e2e --grep files-icon-sets-two-devices
```

Vorlage: `appearance-sync-two-devices`. Laptop und Telefon derselben Vault: Auf dem Laptop ein eigenes Set importieren und wählen; auf dem Telefon erscheint es nach dem Sync und ist gewählt (kein halbes Set zwischendurch, dort zeigt die Liste bis dahin Lucide und „wird synchronisiert“). Beide Geräte wählen gleichzeitig verschiedene Sets: am Ende gilt auf beiden dasselbe. Entfernen auf dem Laptop lässt das Set auch auf dem Telefon verschwinden.

## 4. Handprüfung (nicht automatisierbar)

- **Android** (Spec 043): Import eines Archivs per Dateiauswahl (`content://`), „Ordner wählen“ ist ausgeblendet; die Gruppe ist bedienbar; die Bilder erscheinen.
- **Lizenzseite**: Einstellungen → Allgemein → Lizenzen zeigt für Lucide, Phosphor und Material Urheber, SPDX, Link und Text (bei Material zusätzlich Apache-2.0 und den Hinweis auf Material Design Icons und Material Symbols); die Angaben stimmen mit `src-tauri/icon-sets/SOURCES.md` überein.
- **Leistung** (SC-004): einen Ordner mit 1000 Einträgen mit Material öffnen und mit Lucide vergleichen; Unterschied unter 200 ms.
- **Importierte Themes**: Der Import lehnt Themes mit Logos nicht ab; die Vorschau nennt Urheber und Lizenz und weist auf die Verantwortung des Nutzers hin (FR-017).
