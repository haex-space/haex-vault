# Research: Zugangsdaten der Cloud-Speicher aus dem Passwortmanager wählen

Stand des Codes: `main` bei `26c33733` (2026-10-14). Zeilen- und Dateinamen beziehen sich darauf.

## R1 – Wie kommt die Wahl in den Service?

**Entscheidung**: Neue Form `entry { itemId }` in `CredentialsInput` (`remote_storage/model.rs`). Der Service
löst sie vor dem Test auf (`service_link.rs`): Er liest den Eintrag als `Caller::Internal { feature:
"storage" }`, bildet die Zugangsdaten mit `credentials::from_item` und testet mit ihnen. Beim Speichern
übernimmt die Verbindung `credentials_item_id = itemId`, statt `credentials::create` aufzurufen.

**Begründung**: `StorageAnswer::Confirm.credentials` (Fenster für Vorschläge von Erweiterungen) und
`ConnectionInput.credentials` tragen schon dieselbe Eingabeform; mit einer Variante gilt die Wahl an beiden
Stellen ohne zweiten Pfad (FR-013). `storage_dialog_resolve` ruft nur holzis Fenster auf; eine Erweiterung
erreicht ihn nicht (Bridge-Parameter mit `credentials` sind abgelehnt, 038 US3 Szenario 4), also kann sie
keinen Eintrag wählen (FR-011).

**Verworfen**: ein eigenes Feld `credentialsItemId` neben `credentials` – zwei Felder, die sich
ausschließen, und eine zweite Stelle im Dialog; ein eigener Command „Eintrag verknüpfen“ – umgeht den Test
vor dem Speichern (FR-004).

**Zugriff**: Der interne Aufrufer hat `ReadWrite` auf `Scope::All`; ein Eintrag mit fremdem Eigentümer, im
Papierkorb oder nicht vorhanden ist für ihn `PasswordsNotFound` (Z5, Z13, Z14). Das ist die Prüfung, dass
nur sichtbare Einträge gewählt werden; eine eigene Berechtigungsprüfung entfällt.

## R2 – Eine Regel für „Eintrag → Zugangsdaten“

**Entscheidung**: Der reine Teil von `credentials::read` (Benutzername/Passwort/Felder `sessionToken`,
`privateKey`, `passphrase` je Protokoll) wird `credentials::from_item(&SecretItem, Protocol) ->
Option<Credentials>`. `read`, `state_in` (Passung) und die Auswahlliste rufen sie.

**Begründung**: Die Regel „passender Eintrag“ der Spec ist genau diese Zuordnung; zwei Kopien würden
auseinanderlaufen (eine Umsetzung je Regel). Die Zuordnung bleibt wie in 038/050, damit vorhandene
Einträge weiter gelten.

## R3 – Auswahlliste

**Entscheidung**: Command `storage_credentials_candidates(protocol)`. Ablauf in einem Lesezugriff:
`items::headers_in_scope(q, Scope::All, Caller::User)` (ohne Papierkorb) → Vorfilter über Kopfdaten
(Benutzername nicht leer; für S3 und WebDAV `has_password`) → für die Übrigen `reveal::secret_item` und
`from_item`; Treffer werden als `CredentialsCandidate` (Kennung, Titel, Benutzername, Adresse, `tagged`,
`owned`) geliefert. Sortierung: getaggt zuerst, dann Titel ohne Beachtung der Schreibweise. Die Suche läuft
im Fenster über diese Liste (Titel, Benutzername, Adresse sind keine Geheimnisse).

**Begründung**: Ob ein SFTP-Eintrag einen Schlüssel hat, steht nur in den eigenen Feldern; die Kopfdaten
reichen nicht. Der Vorfilter hält die Zahl der gelesenen Geheimnisse klein. Kein Geheimnis verlässt Rust
(SC-005).

**Verworfen**: Kopfdaten über die vorhandenen Passwort-Commands holen und in TypeScript filtern – die
Passung wäre zweimal umgesetzt (R2); Suche je Tastendruck im Backend – die Liste ist klein genug, eine
Rundreise je Taste bringt nichts.

## R4 – Das Tag

**Entscheidung**: Tagname `haex-cloud-storage` als Konstante `credentials::TAG`.

- **Neu angelegt**: `ItemInput.tags = [TAG]` in `credentials::create`.
- **Vorhanden, von holzi angelegt**: beim Öffnen der Vault, in derselben Stelle wie
  `passwords::maintenance::start_after_open` (`instances/open.rs`, `instances/import.rs`), trägt
  `credentials::start_after_open` das Tag bei allen Einträgen mit Eigentümer `storage` nach
  (`tags::bulk_set`, abgeleitete Verknüpfungs-Kennungen, daher wiederholbar und syncfest; zwei Geräte
  gleichzeitig erzeugen höchstens zwei gleichnamige Tags, die `reconcile_tags` schon zusammenführt).
- **Gewählt**: nach dem Speichern der Verbindung `PasswordsService::set_tags(&Caller::User, [id], [TAG],
[])`.

**Begründung**: Das Verknüpfen ist eine ausdrückliche Handlung des Nutzers in holzis eigenem Fenster; so
bekommt der interne Aufrufer keine Schreibrechte an Einträgen, die ihm nicht gehören (beantwortet die offene
Frage der Checkliste). Ein Fehler beim Taggen ist Komfort, nicht Sicherheit: Er wird mit Meldung ohne Wert
protokolliert, die Verbindung gilt (wie `delete_credentials_if_unused`).

**Verworfen**: das Tag beim Verknüpfen nicht zu setzen – die Spec (FR-006) will gewählte Einträge als
verwendet erkennbar machen; Tag wieder entfernen beim Lösen – der Nutzer kann es selbst entfernen, und ein
automatisches Entfernen überrascht bei einem Eintrag, den er selbst getaggt hat.

## R5 – „Neue Zugangsdaten nötig“ bei fehlenden Werten

**Entscheidung**: `credentials::state_in(q, item_id, protocol)`: ist der Eintrag da und nicht im Papierkorb,
aber `from_item` liefert nichts, ist der Zustand `Missing` (bisher `Present`). Drei Aufrufer ändern sich:
`service.rs` (`overview`, `save_connection_in`) und `service_host_key.rs`.

**Begründung**: FR-010. Heute meldet die Übersicht `Present`, der Aufruf scheitert aber mit `Missing`; der
Nutzer sähe keinen Hinweis. Wertet man bei einem teilweise synchronisierten Eintrag (eigenes Feld noch
nicht da) kurz `Missing`, ist das derselbe Zustand, den `read` schon heute meldet.

## R6 – Löschen und Entfernen

**Befund** (`credentials.rs`, `service/owned.rs`): `credentials::delete` ruft `delete_owned_item`; das löscht
nur Einträge mit `owner == "storage"`, alles andere ist `PasswordsNotFound` und wird als „schon weg“
behandelt. Das gilt für `remove_connection`, `delete_credentials_if_unused` und den Rückbau nach einem
fehlgeschlagenen `put_connection`.

**Entscheidung**: keine Änderung nötig; ein gewählter Eintrag bleibt immer (FR-012), ein von holzi
angelegter geht, wenn keine Verbindung ihn mehr nutzt. Der Test `service_link_tests.rs` sichert das, damit
eine spätere Änderung an `delete_owned_item` es nicht stillschweigend bricht. Der Rückbau nach einem
Fehler ruft `delete` nur für Einträge, die dieser Aufruf angelegt hat (nicht nach einer Wahl).

**Warnung vor dem Löschen**: `StorageUsage::uses` (`credentials.rs`) fragt `credentials_in_use`
nach jeder Kennung; das trifft gewählte Einträge ohne Änderung (034 FR-034). Test im Passwortmanager-Pfad.

## R7 – Oberfläche

**Entscheidung**:

- `CredentialsEntryPicker.vue` zeigt eine Liste (Group/Row/OptionRow, Stil COSMIC/GNOME), Suchfeld, leeren
  Zustand mit Erklärung (Vault ohne passende Einträge, FR-002); Auswahl setzt `entryId` im Entwurf.
- Quelle „Neu eingeben | Eintrag aus dem Passwortmanager“ als Auswahl im Formular; beim Anlegen
  vorgewählt „Neu“; beim Tauschen (`replaceCredentials`) beides möglich.
- Das Formular zeigt für eine vorhandene Verbindung `ConnectionView.credentialsEntry` (Titel) und „Eintrag
  öffnen“ → `openApp('system.passwords', '/entry/<id>')` (Pfade tragen nur Kennungen, 034 FR-039).
- Die Eingabe aus dem Entwurf baut `credentialsInput.ts` (rein, mit `check-storage-credentials.ts`
  geprüft); `ConnectionForm.vue` (476 Zeilen) schrumpft dadurch.
- Das Fenster `StorageCredentialsModal.vue` bekommt dieselbe Auswahl und sendet `entry`.

**Verworfen**: Auswahl als Dialog statt als Liste im Formular – mehr Tippen (SC-001), schlechter auf dem
Telefon.

## R8 – Tests

- Rust, eigene Dateien: `credentials_tests.rs` (Zuordnung je Protokoll, Passung, Tag bei Neuanlage,
  Nachtragen wiederholbar, Löschen lässt Einträge anderer Eigentümer), `service_link_tests.rs` (Wahl legt
  keinen Eintrag an, Fehltest speichert nichts und taggt nichts, Tausch löscht den alten eigenen Eintrag,
  Entfernen behält den gewählten, zwei Verbindungen eine Kennung, Papierkorb → `Missing`, Wert fehlt →
  `Missing`, Liste ohne Geheimnis in der Serialisierung, Papierkorb und Fremd-Eigentümer fehlen).
- Vorhanden und weiter grün: `passwords_owner.rs`, `passwords_access.rs` (Z14: Erweiterung mit `*` erreicht
  einen von holzi angelegten Eintrag nie).
- E2E `storage-credentials-entry`: WebDAV lokal (Docker): Eintrag im Passwortmanager anlegen, im Formular
  wählen, speichern; kein zweiter Eintrag; Passwort im Passwortmanager ändern → Test mit dem neuen Wert;
  Eintrag löschen → „neue Zugangsdaten nötig“; anderen wählen → läuft; Tag filtert den von holzi angelegten
  Eintrag.
