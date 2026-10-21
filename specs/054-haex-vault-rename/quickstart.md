# Quickstart: Umbenennung prüfen

Prüfschritte, die zeigen, dass die Umbenennung vollständig und folgenlos ist. Die Frontend- und
Script-Befehle laufen über die Nix-DevShell; Rust-Befehle über den Nix-Wrapper
(`scripts/with-nix-host-bridge.sh`). Rust-Builds sind teuer: nur gezielte Ziele, `-j 4`.

## Voraussetzungen

- Spec und Plan gemergt, Tag `holzi-final` im Remote.
- Sicherung der alten Vault-Datei außerhalb des Repos (Ordner mit `.db`, `.db.vault-id`).
- `nix develop --command pnpm install --frozen-lockfile` im Worktree.

## 1. Der Name ist weg (nach S8, Szenario US 1)

```sh
nix develop --command python3 scripts/ci/check-old-name.py          # Rückgabewert 0, „old name: clean …“
nix develop --command python3 -B -m unittest discover -s scripts/ci -p 'test_check_old_name.py'
git grep -n -i holzi -- . ':!specs/054-haex-vault-rename' ':!.spaex/**' ':!.specify/memory/constitution.md' # nur erlaubte Inhaltszeilen; Pfadtreffer prüft das Gate
```

Erwartung: keine Treffer außerhalb der Ausnahmeliste (SC-001). Ein Test-Branch, der „holzi“ in eine
Quelldatei schreibt, lässt „Documentation consistency“ fehlschlagen und nennt die Fundstelle.

## 2. Die alte App ist eindeutig (nach S1, US 4)

```sh
git grep -n -i -E "haex[-_ ]vault" -- src src-tauri docs specs ':!specs/054-haex-vault-rename' \
  | grep -v -i -E "legacy|2\.x"                # liefert nur noch Zeilen, die die neue App meinen (nach S6 alle)
```

Erwartung (nach S1): jede Fundstelle, die die alte App meint, trägt „2.x“ (beim ersten Auftreten je
Datei „2.x (Legacy)“) oder `legacy`; ausgenommen sind Dateinamen wie `haex-vault-mapping.md`, der
Verzeichnisname `037-haex-vault-import` und das Logo `haex-vault-icon.png` der neuen App;
das Modul heißt `src-tauri/src/passwords/import/legacy_haex_vault/`. Stichprobe in
`specs/034-password-manager/spec.md` und `specs/037-haex-vault-import/spec.md`.

## 3. Testvault öffnet sich (ab S1, mit jeder Stufe, US 2)

```sh
scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml \
  --test pre_rename_vault -j 4
```

Erwartung: der Test importiert `tests/fixtures/pre_rename_vault/`, findet alle Passwort-Einträge
(Anzahl und Felder stimmen mit der README der Datei), öffnet Kontakte ohne Abbruch, und zeigt nicht
prüfbare Sync-Daten als Zustand (FR-016). Wird der Test in einer Stufe rot, steht dort, welcher
persistierte Name ihn trifft (data-model.md, Tabelle „Persistierte Namen“).

## 4. Alles baut und läuft (nach jeder Stufe, US 3)

```sh
nix develop --command pnpm lint                   # Frontend
nix develop --command pnpm typecheck
nix develop --command pnpm check:passwords
nix develop --command pnpm check:contacts         # betroffene check-Gruppen
scripts/with-nix-host-bridge.sh cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
nix develop --command pnpm lint:rust              # clippy, beide Feature-Sätze, -D warnings
scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml -j 4   # nur gezielt lokal
nix develop --command pnpm test:e2e --run-timeout 2700 # Desktop-End-to-End (CI: Pflicht)
```

Die CI führt zusätzlich die drei Shards der Android-End-to-End-Tests, die Builds für Android und
iOS und die Plattform-Probe (Windows, macOS, Android) aus (SC-002). Nach S2 zusätzlich: die
ts-rs-Bindings sind neu erzeugt und die Drift-Prüfung der CI ist grün.

## 5. Zwei Geräte unter dem neuen Namen (nach S3 und S5, SC-004)

Die Mehrgeräte-Szenarien aus Spec 033 (`sync-link`, `passwords-sync-two-devices`,
`contacts-two-devices`, `extension-two-devices` in `scripts/e2e/scenarios/`) laufen mit
`nix develop --command pnpm test:e2e` unverändert in ihren bisherigen Wartezeiten. Von Hand: zwei
frische Instanzen von `nix develop --command pnpm tauri:dev` verknüpfen (Einstellungen → Föderation → „Gerät verknüpfen“, auf dem neuen Gerät
„Mit einer Vault verknüpfen“) und einen Passwort-Eintrag anlegen; er erscheint auf dem zweiten
Gerät.

## 6. Erweiterung und Android (nach S5, US 3)

- Eine Erweiterung installieren und öffnen: sie lädt über `haex-ext://` und erreicht ihre Daten
  (Szenario `extension-lifecycle` des E2E-Rigs).
- Android: die Debug-APK trägt den Paketnamen `space.haex.vault`, das Eichhörnchen-Icon und den
  Namen „HaexVault“ (`adb shell pm list packages | grep space.haex.vault`,
  `aapt dump badging <apk>`); die Android-End-to-End-Tests laufen mit
  `nix develop --command pnpm test:e2e --platform android --apk <apk> --shard 1/3` (wie `scripts/ci/android-e2e.sh`); den
  Emulator liefert der devShell-Befehl, der heute `holzi-emulator` heißt und nach S4 den neuen Namen
  trägt (`.devshell/packages.nix`).

## 7. Die echte Datei (S9, SC-003)

Vorher: haex-vault 3 beenden, den alten Datenordner `space.haex.vault` entfernen, falls er Daten der
alten App enthält (Spec, Edge Cases). Dann:

1. Die gesicherte Ordnerkopie (nicht das Original) in ein Arbeitsverzeichnis kopieren.
2. In der App: Willkommensseite → „Vault-Datei öffnen“ → die Kopie von `test.db` wählen, Passphrase
   eingeben.
3. Passwort-Manager öffnen: Anzahl der Einträge mit der alten App vergleichen (in `holzi-final`
   gebaut: `git worktree add <pfad> holzi-final`), Stichprobe von drei Einträgen mit Notizen,
   Anhang und TOTP.
4. Meldet die App einen Zustand wegen nicht prüfbarer Sync-Daten, ist das erwartet (FR-016); die
   Geräte werden neu verknüpft.
5. Bricht das Öffnen ab oder fehlen Einträge: **Abbruch der Stufe**, Passwort-Export in
   `holzi-final` (CSV oder Bitwarden-JSON), Import in der neuen App, und die Spec bekommt einen
   Eintrag in den Clarifications.

Das Original und die Sicherung bleiben in jedem Fall unverändert (FR-015).

## 8. Der Umzug (S11, US 5)

Siehe die Prüfung am Ende von [contracts/cutover.md](contracts/cutover.md): Weiterleitung von
`haexmas/holzi`, archiviertes Legacy-Repo, Test-PR mit allen Pflichtprüfungen, abgelehnter Push auf
`main`.

## 9. Geschwister-Repos (S10, US 6)

- vault-sdk: `pnpm build` und `pnpm test` grün; `git grep -n -i holzi` leer, und die exportierten
  Typen tragen die neuen Namen ohne Alias (`ContactHaexVaultAddresses`, `ContactHaexVaultDevice`).
- haextension und haex-crdt: bauen und testen grün gegen das neue SDK (gepinnter Commit-SHA).
- atoms: das Molecule `com.github.haexmas.atoms.haex-vault`; im Repo nach der Neuinstallation
  `.spaex/manifest.json` mit der neuen Identität, `git diff` auf `.specify/` zeigt nur
  Erwartetes, danach zurückgesetzt.
