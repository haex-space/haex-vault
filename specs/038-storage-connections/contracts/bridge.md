# Contract: Bridge-Methoden `extension_remote_storage_*`

Die neun Methoden des vault-sdk (`src/api/remoteStorage.ts`), in haex-vault in
`src-tauri/src/extensions/remote_storage.rs`, registriert in `bridge/dispatch.rs` (aus der Liste `LATER`
entfernt). Fehlercodes wie in Spec 017 (`contracts/bridge.md`). „Lesen“/„Lesen und Schreiben“ =
Berechtigung der Art `remoteStorage` mit dem Speicher (`backendId`) oder `*` als Ziel; ohne passende
Berechtigung 1004 für eine Berechtigung im Zustand „fragen“, sonst 1002 (wie Spec 017).

| Methode                                   | Parameter                                                                                          | Recht                                                                                                                                                     | Antwort                                                                                                          | Dialog                                                                                                                                                                                                     |
| ----------------------------------------- | -------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `extension_remote_storage_list_backends`  | `{}`                                                                                               | Lesen je Speicher                                                                                                                                         | `[{id, type: "s3", name, providerName, bucket}]` nur für gedeckte Speicher; ohne jede Leseberechtigung 1004/1002 | nein                                                                                                                                                                                                       |
| `extension_remote_storage_add_backend`    | `{request: {name, type: "s3", config: {endpoint?, region?, bucket, pathStyle?}, sameProviderAs?}}` | eigener Endpunkt: `add` für dessen Host (FR-009b); AWS nur mit Region: `add` für `s3.<region>.amazonaws.com`; `sameProviderAs`: Lesen auf diesen Speicher | Eintrag wie in der Liste                                                                                         | ja: Vorschlag prüfen und bestätigen (oder vorhandene Verbindung wählen); für eine neue Verbindung danach Zugangsdaten im Fenster über der ganzen App; Test; danach Lesen und Schreiben für die Erweiterung |
| `extension_remote_storage_update_backend` | `{request: {backendId, name?, config?: {bucket?}}}`                                                | Lesen und Schreiben                                                                                                                                       | Eintrag wie in der Liste                                                                                         | ja: Änderung bestätigen; neue Zugangsdaten nur danach im Fenster über der ganzen App; Test bei neuem Bucket oder neuen Zugangsdaten                                                                        |
| `extension_remote_storage_test_backend`   | `{backendId}`                                                                                      | Lesen und Schreiben                                                                                                                                       | `null` oder 2002 mit `kind`                                                                                      | ja: Bestätigen des Tests                                                                                                                                                                                   |
| `extension_remote_storage_remove_backend` | `{backendId}`                                                                                      | Lesen und Schreiben                                                                                                                                       | `null`                                                                                                           | ja: nennt andere Erweiterungen mit Berechtigung                                                                                                                                                            |
| `extension_remote_storage_upload`         | `{request: {backendId, key, data (Base64)}}`                                                       | Lesen und Schreiben                                                                                                                                       | `null`                                                                                                           | nein                                                                                                                                                                                                       |
| `extension_remote_storage_download`       | `{request: {backendId, key}}`                                                                      | Lesen                                                                                                                                                     | Base64                                                                                                           | nein                                                                                                                                                                                                       |
| `extension_remote_storage_list`           | `{request: {backendId, prefix?}}`                                                                  | Lesen                                                                                                                                                     | `[{key, size, lastModified?}]`, Schlüssel ohne Präfix der Erweiterung                                            | nein                                                                                                                                                                                                       |
| `extension_remote_storage_delete`         | `{request: {backendId, key}}`                                                                      | Lesen und Schreiben                                                                                                                                       | `null`                                                                                                           | nein                                                                                                                                                                                                       |

## Regeln

- **Zugangsdaten nie über die Bridge** (FR-012, FR-013a): enthält `config` eines der Felder
  `accessKeyId`, `secretAccessKey`, `sessionToken`, antwortet haex-vault 3001 `credentials are entered in
haex-vault`, bevor ein Dialog erscheint. Keine Antwort und keine Fehlermeldung enthält Endpunkt, Region,
  Zugangsdaten, Kopfzeilen oder Antworttext des Anbieters.
- **Hinzufügen mit `sameProviderAs`** (Review 2026-10-06): Endpunkt, Region und Adressierung kommen aus
  der Verbindung des genannten Speichers; `config` trägt dann nur `bucket`. Enthält `config` daneben
  `endpoint`, `region` oder `pathStyle`, antwortet haex-vault 3001, bevor ein Dialog erscheint. Ohne
  `sameProviderAs` ist `region` Pflicht (sonst 3001).
- **Vorgeschlagener Endpunkt** (FR-009b, FR-017, R8, Betreiber 2026-10-06): braucht die Berechtigung
  `remoteStorage`/`add` mit dem Host des Endpunkts (`host:port`, `host` oder `*`; eine IPv6-Adresse als
  `[addr]:port` oder `[addr]`); ohne sie 1004 im
  Zustand „fragen“ (die Rückfrage nennt den Host), sonst 1002, jeweils vor dem Dialog. Mit ihr darf der
  Endpunkt lokal sein (Loopback, privat, auch `http`); Link-Local, unspezifizierte und Multicast-Adressen
  sowie `http` zu einem Host, der nicht lokal auflöst, → 3001 vor dem Dialog. Der Dialog zeigt einen
  lokalen oder unverschlüsselten Endpunkt als solchen. Die Verbindung bekommt `endpoint_scope` nach R8,
  und jede aufgelöste Adresse wird vor jedem Aufruf dagegen geprüft. Löst der Host beim Anlegen nach
  dem Dialog in einen anderen Bereich auf als den, den der Dialog gezeigt hat, → 3001, bevor ein Test
  die Zugangsdaten sendet; nichts wird angelegt. Eine vorhandene Verbindung steht nur zur Wahl, wenn
  Endpunkt, Region und Bereich passen.
- **AWS nur mit Region** (FR-009b, Betreiber in #285: nur „wenn die Berechtigung dafür besteht“): ohne
  `endpoint` braucht der Vorschlag `remoteStorage`/`add` mit `s3.<region>.amazonaws.com` (oder `*`) als
  Ziel; ohne sie 1004 im Zustand „fragen“ (die Rückfrage nennt diesen Host), sonst 1002, jeweils vor dem
  Dialog. Eine Region, die keinen solchen Host ergibt (nur Kleinbuchstaben, Ziffern, `-`), → 3001.
- **Bereich** (FR-010, R4): jeder `key` und `prefix` wird nach R5 geprüft (sonst 3001) und mit
  `haex-ext/<vault_id>/<extension_id>/` versehen; Schlüssel in Antworten ohne dieses Präfix.
- **Unbekannter oder nicht gedeckter Speicher**: 1001 für einen unbekannten, 1004/1002 für einen nicht
  gedeckten (eine Erweiterung ohne Leseberechtigung erfährt nicht, ob es ihn gibt: auch hier 1004/1002,
  nie 1001).
- **Grenzen** (R7): Hochladen über `max_response_bytes` und Herunterladen über `max_response_bytes / 4 *
3` → 7000; Frist `timeout_ms` je Aufruf → 7000; Auflisten über `max_rows` Objekte → 7000 mit Hinweis
  auf ein engeres Präfix.
- **Fehler beim Anbieter** (R7): 1001 `not found`; 2002 `{kind: "accessDenied"}`; 2002 `{kind:
"network"}`.
- **Dialog** (R6): Der Dialog über dem Tab fragt nur nach Bestätigung und enthält nie Felder für
  Zugangsdaten; die gibt der Nutzer nur im Fenster von haex-vault über der ganzen App ein (Review 2026-10-06).
  Abbruch in einem der beiden, Ablauf (300 s) oder Schließen des Rahmens → 1002 `cancelled by the user`;
  ein gescheiterter Test → 2002 mit `kind`, nichts angelegt.
- **Plattformen**: auf allen Plattformen gleich (FR-016); kein `cfg(desktop)`.
- **Entwicklerversionen** (R4, Review 2026-10-06): eigener Bereich `haex-ext-dev/<vault_id>/<dev_extension_id>/`
  aus der geräteeigenen `dev_extension_id`, nie der Bereich der installierten Erweiterung mit demselben
  Herausgeberschlüssel und Namen; sie sehen deren Objekte nicht.
