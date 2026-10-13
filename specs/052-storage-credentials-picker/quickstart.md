# Quickstart: Zugangsdaten aus dem Passwortmanager wählen

Prüfpfade für die Umsetzung. Voraussetzungen und Befehle wie in den Speicher-Szenarien von Spec 038 und 050
(`scripts/e2e/`, Docker für den WebDAV-Server).

## 1. Automatische Prüfungen (nur die berührten Ziele, `-j 4`)

```sh
cargo test -j 4 -p holzi --lib remote_storage::credentials
cargo test -j 4 -p holzi --lib remote_storage::service_link
cargo test -j 4 -p holzi --test passwords_owner --test passwords_access
node --test scripts/check-storage-credentials.ts
```

Erwartung: grün. Die Namen der Tests nennen das Verhalten (zum Beispiel
`a_chosen_entry_is_kept_when_the_connection_goes`, `a_failed_test_saves_and_tags_nothing`).

## 2. E2E `storage-credentials-entry` (Docker)

```sh
pnpm test:e2e --grep storage-credentials-entry
```

Das Szenario deckt US1 bis US3 ab:

1. Im Passwortmanager einen Eintrag mit Benutzername und Passwort des lokalen WebDAV-Servers anlegen.
2. Einstellungen → Cloud-Speicher → neue WebDAV-Verbindung → „Eintrag aus dem Passwortmanager“ → Eintrag
   wählen → „Testen und speichern“. Erwartung: Verbindung da, Anzahl der Einträge im Passwortmanager
   unverändert (SC-002), der Eintrag trägt das Tag `haex-cloud-storage`.
3. Passwort des Eintrags im Passwortmanager auf ein falsches ändern, „erneut testen“: Zugang abgelehnt;
   zurückändern: Test besteht (SC-004).
4. Eintrag löschen (Warnung nennt die Verbindung, SC-006): Speicher zeigt „neue Zugangsdaten nötig“; anderen
   Eintrag wählen: läuft wieder.
5. Eine zweite Verbindung mit neu eingegebenen Zugangsdaten: Eintrag „WebDAV: …“ mit Tag, Formular nennt
   ihn, „Eintrag öffnen“ zeigt ihn (US2). Verbindung entfernen: Eintrag weg; gewählter Eintrag der ersten
   Verbindung bleibt nach deren Entfernen.

## 3. Manuell (Telefon und Fenster für Erweiterungen)

- Android-Emulator (`holzi-emulator`): Auswahl bedienbar, Liste scrollt, Tastatur verdeckt das Suchfeld
  nicht (FR-015).
- Test-Erweiterung `extension-storage`: „Speicher hinzufügen“ ohne Zugangsdaten → im Fenster von holzi
  „Eintrag aus dem Passwortmanager“ wählen → Speicher angelegt, die Erweiterung sieht keine Kennung (US4).

## 4. Nach dem Öffnen einer älteren Vault

Vault mit Verbindungen aus Spec 038/050 öffnen: im Passwortmanager nach dem Tag filtern; alle Einträge
„S3: …“, „WebDAV: …“, „SFTP: …“ stehen darin (SC-003). Zweites Öffnen ändert nichts.
