# Quickstart: Passwortmanager-Redesign

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Prüfung in drei Stufen: automatisch (Rust und Skripte), End-to-End (Rahmen aus 016) und von
Hand (alles, was Berührung, native Dialoge und fremde Programme braucht).

## Voraussetzungen

```sh
cd .worktrees/036-password-redesign
nix develop --command pnpm install --frozen-lockfile
```

Rust und End-to-End laufen nur in `nix develop` mit `scripts/with-nix-host-bridge.sh`
(Cargo-Aufrufe immer mit `-j 4`, gezielt je Prüfziel; kein vollständiges `cargo test`).

## 1. Automatisch

```sh
nix develop --command sh -c '
  pnpm check:passwords && pnpm check:agent-actions && pnpm check:wm-navigation &&
  pnpm check:wm-state && pnpm check:settings && pnpm check:vault-data &&
  pnpm check:templates && pnpm typecheck && pnpm typecheck:scripts && pnpm lint &&
  pnpm format:check'
```

Rust (nacheinander, jedes für sich):

```sh
nix develop --command sh -c '
  for t in passwords::references passwords::webauthn passwords::copy passwords::passkeys_ops \
           passwords::passkey_links passwords::import::references \
           passwords::trash identity::migrations; do
    CARGO_BUILD_JOBS=4 scripts/with-nix-host-bridge.sh cargo test -j 4 \
      --manifest-path src-tauri/Cargo.toml --lib "$t"
  done
  for t in passwords_references passwords_copy passwords_passkeys passwords_sync passwords_import \
           vault_upgrade; do
    CARGO_BUILD_JOBS=4 scripts/with-nix-host-bridge.sh cargo test -j 4 \
      --manifest-path src-tauri/Cargo.toml --test "$t"
  done
  scripts/with-nix-host-bridge.sh cargo fmt --manifest-path src-tauri/Cargo.toml --check
  pnpm lint:rust
  pnpm generate:ts-types   # danach git diff src/types/bindings muss leer sein
'
```

Erwartung: alle grün; `passwords_passkeys` prüft Signaturen mit dem öffentlichen Schlüssel
und die Herkunftsfälle (SC-005, SC-007); `passwords_references` die Vektoren aus
[contracts/references.md](./contracts/references.md) (SC-006, SC-009).

## 2. End-to-End

```sh
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=$PWD/src-tauri/target/e2e-build nix develop --command \
  scripts/with-nix-host-bridge.sh pnpm tauri build --debug --no-bundle
for s in passwords-basic passwords-narrow-window passwords-session-restore \
         passwords-sync-two-devices passwords-tabs passwords-organize passwords-references \
         passwords-passkeys passwords-attachments; do
  nix develop --command pnpm test:e2e --app $PWD/src-tauri/target/e2e-build/debug/haex --grep $s
done
```

(Je Szenario ein `--grep`, ohne `|`.) Erwartung: die vier vorhandenen grün (SC-010;
`passwords-narrow-window` und `passwords-session-restore` sind um die Tabs erweitert), die fünf
neuen grün.

## 3. Von Hand (`pnpm tauri:dev` in `nix develop`)

Alle Punkte auch in einem Fenster mit 360 px Breite und, wo möglich, auf einem Telefon.

**M1 Tabs und Wischgeste (US1, FR-001 bis FR-006)**

1. Eintrag mit TOTP, eigenen Feldern, Anhang, Passkey öffnen. Tabs durch Tippen, mit den
   Pfeiltasten der Leiste und (Telefon/Touchscreen) durch Wischen wechseln; am ersten und
   letzten Tab geschieht beim Weiterwischen nichts.
2. Ein Wisch, der auf dem Passwortfeld oder einem TOTP-Code beginnt, wechselt den Tab nicht.
3. „Bewegung reduzieren“ im Betriebssystem einschalten: der Wechsel geschieht ohne Gleiten.
4. Bearbeiten, Titel leeren, speichern: Sprung zu Details, Feld markiert; die Eingaben in
   Details und Extra bleiben beim Wechsel; Verlauf ist beim Bearbeiten nicht wählbar.
5. Zurück/Vor und Sitzung wiederherstellen: der Eintrag öffnet auf dem gewählten Tab.

**M2 Verlauf (US2)**: Eintrag dreimal ändern; Zeitleiste neuester oben, Stand wählen, ein
Geheimnis aufdecken (beim Wechsel wieder verborgen), ältesten Stand wiederherstellen (neuer
oberster Stand, nichts gekürzt).

**M3 Ordnen (US3, US4)**: Brotkrumen in drei Ordnerebenen anklicken; zwei Einträge auswählen,
ausschneiden, im anderen Ordner einfügen; einen Ordner mit Inhalt kopieren: der Dialog
fragt Titel, Verlauf, Verweise; Abbrechen legt nichts an; Bestätigen legt die Kopie an.
Einen Ordner in seinen Unterordner einfügen: abgelehnt. Langdruck auf dem Telefon beginnt die
Auswahl. Rechtsklick auf Eintrag, Ordner, leere Fläche; auf dem Telefon der Menüknopf an der
Zeile. Kürzel: Strg+A, X, C, V, Entf, Eingabe, Strg+F, Pfeile, Strg+B und Strg+Umschalt+C;
das Passwort ist danach in einem anderen Programm einfügbar und nach der eingestellten
Zeit wieder weg; Strg+C auf einem **Eintrag** legt **nichts** in die Zwischenablage des
Betriebssystems. Zweites Fenster des Passwortmanagers: Ausschneiden im einen, Einfügen im
anderen.

**M4 Verweise (US7)**: Eintrag „Konto“; in einem zweiten Eintrag „Verweis einfügen“ beim
Passwort: Konto, Passwort. Passwort in Konto ändern: das zweite zeigt, kopiert und benutzt
das neue. Verweis auf einen Eintrag, der selbst einen Verweis hat (Kette); einen Kreis
(A→B→A) speichern: abgelehnt. Quelle endgültig löschen: Warnung mit der Zahl der Ziele;
„in eigene Werte umwandeln“ lässt in den Zielen den Wert stehen. KeePass-Datei mit
`{REF:P@I:…}` importieren: der Verweis ist umgewandelt, der Bericht nennt die Zahlen.

**M5 Passkeys (US5)**: Es gibt keine Oberfläche zum Anlegen. Mit einer Test-Erweiterung oder
dem Rust-Integrationstest einen Passkey an einem Eintrag anlegen; im Tab Extra erscheint er
mit Gegenstellenname, Benutzer, Daten; umbenennen, löschen (die Bestätigung nennt die
Gegenstelle). Kopie mit „Passkeys per Verweis“: die Kopie zeigt ihn mit „Verweis auf …“, ohne
Umbenennen und Löschen, mit „Verweis lösen“.

**M6 Anhänge (US6)**: Drei Bilder und ein PDF anhängen (Dateidialog): vier Karten mit
Vorschau/Symbol, Name, Größe, Typ. Bild antippen: Lightbox, Pfeile/Wischen/Pfeiltasten, Zoom
mit Zwicken und Mausrad, Escape schließt, der Fokus kehrt auf die Karte zurück; PDF antippen:
Speichern unter. Ein beschädigtes Bild: Symbol und Meldung. Eintrag mit 30 Bildern: nur die
sichtbaren Vorschaubilder laden nach.

**M7 Breite und Barrierefreiheit (FR-042, SC-003)**: 360 px: kein waagerechtes Scrollen in
Tabs, Verlauf, Brotkrumen, Auswahlleiste, Karten, Lightbox; alle Aktionen eines Kontextmenüs
sind über Auswahlleiste oder Seite des Eintrags erreichbar; Tastaturbedienung der Tabs, Menüs
und Karten; Deutsch und Englisch.
