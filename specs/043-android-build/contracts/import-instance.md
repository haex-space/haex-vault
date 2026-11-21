# Contract: `import_instance`

**Anforderungen**: FR-002, FR-002a, Edge Cases „keine Tresordatei“, „gibt es schon“, „gleicher
Name“ | **Research**: R8 | **Plattformen**: Desktop und Android

## Command

```text
import_instance(file: PickedFile, passphrase: String) -> InstanceInfo
```

- `file`: gewählte Datei aus dem Dateidialog ([picked-file.md](./picked-file.md)).
- Ergebnis: derselbe `InstanceInfo` wie nach `create_instance`; der Tresor ist danach offen und
  als aktiver Tresor veröffentlicht, `instance-list-changed` ist gesendet.
- Vorbedingung wie `create_instance` und `open_instance`: Dieser Prozess hält noch keine
  Tresor-Sitzung (Spec 013); sonst `VaultSessionActive`.

## Ablauf

1. Namen bilden ([data-model.md](../data-model.md#übernahme-einer-tresordatei-fr-002a-vertrag-import-instancemd)),
   freien Namen wählen.
2. `.pending`-Marker atomar anlegen (`create_new`); scheitert das, nächsten Namen nehmen.
3. Gewählte Datei über `files::picked` in `<name>.db.<zufall>.tmp` kopieren, dann umbenennen.
   Ein voller Speicher (`ENOSPC`) beim Kopieren wird `NotEnoughSpace`; eine Vorprüfung entfällt,
   weil Anbieter die Größe nicht verlässlich melden.
4. Mit `vault_config(.., create_if_missing = false)` entsperren; `maintenance::run_after_open`
   und `sync::genesis::run_after_open(.., allow_genesis = false)` wie beim Öffnen.
5. Den Fingerabdruck der Vault-Identität (SHA-256 des öffentlichen Schlüssels der
   Vault-Identität) mit den Begleitdateien `<name>.db.vault-id` der anderen Tresore dieser
   Installation vergleichen. Gleich → `AlreadyOnThisDevice`. Die Begleitdatei schreibt haex-vault
   bei jedem Anlegen, Öffnen und Übernehmen; ein Tresor, der seit dem Update nicht geöffnet
   wurde, hat noch keine und wird erst nach seinem nächsten Öffnen erkannt.
6. Marker entfernen, veröffentlichen, Sitzungsdienste starten wie in `open.rs:143-168`.

## Fehler

| Fehler                | Wann                                               | Oberfläche                                              |
| --------------------- | -------------------------------------------------- | ------------------------------------------------------- |
| `WrongPassphrase`     | Entsperren scheitert mit SQLITE_NOTADB             | „Passwort falsch oder keine Tresordatei von haex-vault“ |
| `NotAVault`           | Datei ist SQLite, aber ohne haex-vault-Kennzeichen | „Das ist keine Tresordatei von haex-vault“              |
| `AlreadyOnThisDevice` | Schritt 5                                          | „Diesen Tresor gibt es auf diesem Gerät schon: <Name>“  |
| `NotEnoughSpace`      | Schritt 3                                          | „Nicht genug freier Speicher“                           |
| `Unreadable`          | gewählte Datei nicht lesbar                        | „Die Datei lässt sich nicht lesen“                      |

Jeder Fehler nach Schritt 2 baut vollständig zurück: `.db`, `.db.pending`, `.db.lock`,
`.db-wal`, `.db-shm`, `.db.vault-id`, `.tmp`. Die gewählte Datei wird nur gelesen. Derselbe Rückbau-Helfer
ersetzt den in `create.rs:107-130` (der heute `.db.lock` liegen lässt).

## Oberfläche

- Tresorauswahl (`src/pages/index.vue`): Knopf „Vault-Datei öffnen“ (die Oberfläche sagt
  „Vault“) unter „Instanz anlegen“ (hervorgehoben) und „Mit einer Vault verknüpfen“, schwächer
  gewichtet.
- Ablauf: Dateidialog (Filter `*.db` am Desktop; auf Android alle Dateien, weil Anbieter keine
  Endungen zuverlässig melden) → Sheet `ImportVaultSheet.vue` mit Dateiname, Passwortfeld,
  „Öffnen“; Fehler im Sheet, das Sheet bleibt offen.
- Abbruch des Dialogs: nichts passiert.

## Tests

- Rust: `instances/import_tests.rs` (Name aus dem Dateinamen, Nummer bei belegtem Namen,
  Reservieren über den Marker), `tests/vault_file_import.rs` über die Mock-Laufzeit (Erfolg;
  falsches Passwort; keine SQLite-Datei; fehlende Datei; Kopie eines vorhandenen Tresors;
  Namenskonflikt `-2`; Rückbau hinterlässt keine Datei; Original unverändert per Prüfsumme),
  `instances/cleanup_tests.rs`, `instances/vault_id_tests.rs`.
- e2e (Desktop und Android): `vault-file-import` — Tresor auf Gerät A anlegen, stoppen,
  Datei bereitstellen (Desktop: Pfad; Android: per `run-as` und Test-Seam für den Dialog),
  übernehmen, Einträge sichtbar; zweiter Versuch derselben Datei → „gibt es schon“.
