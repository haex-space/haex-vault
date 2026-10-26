# Quickstart: Kontakte teilen und der Kontakt „Ich“ (Spec 060)

Zum Prüfen der fertigen Umsetzung. `cargo`-Läufe in diesem Projekt bringen den Rechner in OOM, wenn sie
ungebremst laufen: gezielt und mit `-j 4`, vorher `df -h` (alte `.worktrees/*/target`
füllen die Platte).

## Automatische Prüfungen

```bash
# Rust, gezielt
cargo test --manifest-path src-tauri/Cargo.toml -j 4 contacts::
cargo test --manifest-path src-tauri/Cargo.toml -j 4 identity::migrations_contacts
cargo test --manifest-path src-tauri/Cargo.toml -j 4 sync::foreign_auth
cargo test --manifest-path src-tauri/Cargo.toml -j 4 chat::tools   # confirms_itself

# Frontend
pnpm check:contacts
pnpm check:agent-actions
pnpm typecheck:scripts
pnpm lint
pnpm typecheck

# Rust-Lints, auch für Android (cfg-Zweige)
pnpm lint:rust
cargo clippy --manifest-path src-tauri/Cargo.toml --target aarch64-linux-android --all-targets -- -D warnings
```

Nach `cargo test` die ts-rs-Bindings von Leerzeichen am Zeilenende befreien, wie die CI es tut:
`sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.

## Von Hand (zwei Vaults A und B)

1. **Ich**: In A die Kontakte öffnen. Ganz oben steht „Ich“. Vorname „Anna“, Nachname „Beispiel“, zwei
   Telefonnummern, eine E-Mail-Adresse, Geburtstag eintragen. Der Anzeigename ist gesperrt, die Liste
   zeigt weiter „Ich“, die Suche nach „Beispiel“ findet ihn. Löschen gibt es nicht.
2. **Teilen mit Abwahl**: Bei „Ich“ „Teilen“. Alles ist angehakt. Eine Telefonnummer, den Geburtstag und
   ein Nostr-Relay abwählen. Der Link ändert sich mit der Auswahl; der QR-Code erscheint nur, solange der
   Text in 2 048 Zeichen passt.
3. **Einlesen**: Den Link in B einfügen (Kontakte → Einlesen). Die Vorschau zeigt Name, die gewählten
   Angaben, die `npub` in Kurzform und den Hinweis zu „bestätigt“. Als neuen Kontakt speichern und an diesem
   Kontakt die Test-Handynummer ergänzen. Die abgewählten Angaben fehlen.
4. **Nichts geht verloren**: In A eine neue Weitergabe nur mit der E-Mail-Adresse erzeugen (die Karte
   abwählen) und sie in B beim in Schritt 3 angelegten Kontakt einlesen. Die Handynummer bleibt. Eine zweite
   E-Mail-Adresse zeigt „Hinzufügen“ (vorgewählt) oder „Aktualisieren“.
5. **Identität**: In einer dritten Vault C eine Karte für „Anna“ erzeugen und in B zu Annas Kontakt
   (mit bestätigter Identität aus A) hinzufügen wollen. Die Vorschau lässt es nicht zu.
6. **Fremden Kontakt teilen**: In B Annas Kontakt teilen. Die Zeile „holzi-Adressen (bestätigt)“ ist da,
   ohne Einzelauswahl von Geräten. Einen Handykontakt ohne holzi-Adressen teilen: derselbe Dialog, die
   Vorschau in C meldet „nicht über holzi erreichbar“.
7. **Deep Link**: Den Link als Nachricht in einer anderen App öffnen. holzi kommt nach vorn (Entsperren
   zuerst, wenn die Vault gesperrt ist) und zeigt die Vorschau; es ist nichts gespeichert. Auf Android
   zusätzlich den QR-Code mit der Systemkamera scannen. Auf macOS zusätzlich mit zwei Prozessen prüfen,
   dass ein Link, den der Prozess ohne offene Vault empfängt, im Prozess mit offener Vault erscheint.
8. **Agent**: Im Chat „gib mir den Kontakt von Anna zum Weiterschicken“. Es erscheint genau eine Abfrage
   (der Teilen-Dialog mit dem Vorschlag). Eine Angabe abwählen und freigeben: der Agent erhält den Text
   ohne sie. Ablehnen: der Agent erhält nichts. Im Plan-Modus lehnt der Chat die Aktion ab.

## E2E

`scripts/with-nix-host-bridge.sh node scripts/e2e/cli.ts` mit den Szenen `contacts-share` (neu),
`agent-contacts` (erweitert) und dem Android-Deep-Link-Szenario. In `gh pr checks` zuerst nachsehen, ob
die CI den E2E schon gelaufen ist.
