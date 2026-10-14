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

1. Einstellungen → Gestaltung: die Gruppe „Icon-Set für Dateien“ zeigt Lucide (gewählt), Material und Adwaita mit Vorschau.
2. „Material Icon Theme“ anklicken, im Dateibrowser ein Verzeichnis mit `Dockerfile`, `main.rs`, `foto.png` und `src` öffnen: jede Datei hat ein eigenes Bild (Bilder als `img`, nicht als `svg` im DOM), „Gespeichert.“ ist zu sehen, kein Speichern-Knopf.
3. App neu starten: das Set ist noch gewählt.
4. Ein eigenes Set aus dem Fixture-Ordner importieren (über das Eingabefeld für Dateien, wie `BackgroundSetting`): es erscheint mit Vorschau; ein SVG im Set mit Skript ist beim Anzeigen wirkungslos, und im Speicher liegt die bereinigte Fassung.
5. Entfernen mit Bestätigung: das Set ist weg, die Wahl fällt auf Lucide zurück.
6. Ein Verzeichnis mit einer verschlüsselten, nicht lesbaren Datei: das Zustandsbild des Standard-Sets erscheint auch unter Adwaita (FR-005).

## 3. E2E `files-icon-sets-two-devices` (Spec 033)

```sh
pnpm test:e2e --grep files-icon-sets-two-devices
```

Vorlage: `appearance-sync-two-devices`. Laptop und Telefon derselben Vault: Auf dem Laptop ein eigenes Set importieren und wählen; auf dem Telefon erscheint es nach dem Sync und ist gewählt (kein halbes Set zwischendurch, dort zeigt die Liste bis dahin Lucide und „wird synchronisiert“). Beide Geräte wählen gleichzeitig verschiedene Sets: am Ende gilt auf beiden dasselbe. Entfernen auf dem Laptop lässt das Set auch auf dem Telefon verschwinden.

## 4. Handprüfung (nicht automatisierbar)

- **Android** (Spec 043): Import eines Archivs per Dateiauswahl (`content://`), „Ordner wählen“ ist ausgeblendet; die Gruppe ist bedienbar; die Bilder erscheinen.
- **Lizenzseite**: Einstellungen → Allgemein → Lizenzen zeigt für Lucide, Material und Adwaita Urheber, SPDX, Link und Text; die Angaben stimmen mit `src-tauri/icon-sets/SOURCES.md` überein.
- **Leistung** (SC-004): einen Ordner mit 1000 Einträgen mit Material öffnen und mit Lucide vergleichen; Unterschied unter 200 ms.
- **Marken-Logos im Material-Set** (offener Punkt für den Betreiber in [research.md](./research.md) R1): Entscheidung festhalten.
