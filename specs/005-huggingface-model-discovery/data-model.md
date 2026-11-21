# Phase 1 Data Model: Freie Hugging-Face-Modellsuche und Installation

## Neue Runtime-Entitäten

### `HuggingFaceSearchQuery`

Nicht persistierte, validierte Suchanfrage.

| Feld    | Typ       | Beschreibung                                                                                                         |
| ------- | --------- | -------------------------------------------------------------------------------------------------------------------- |
| `query` | `string?` | Optionaler getrimmter Suchbegriff, mindestens zwei Zeichen wenn gesetzt. Ohne Query wird die Top-10-Ansicht geladen. |
| `page`  | `number`  | Positive Seite, standardmäßig 1.                                                                                     |
| `limit` | `number`  | Server-/Client-seitig begrenzte Trefferzahl, maximal 20.                                                             |

Die Suche akzeptiert keinen freien URL- oder Dateipfad als Ersatz für einen
Suchbegriff. Ein Repository-ID-ähnlicher Suchbegriff bleibt erlaubt. Eine
fehlende Query ist ausschließlich für die initiale Top-10-Ansicht zulässig;
der Default sortiert öffentliche Treffer nach Downloads absteigend und
begrenzt sie auf zehn Repositorys. Eine gesetzte Query liefert bis zu 20
Treffer.

### `HuggingFaceModelResult`

Normalisierte Darstellung eines öffentlichen Hugging-Face-Repositories.

| Feld             | Typ                          | Nullable | Beschreibung                                                               |
| ---------------- | ---------------------------- | -------: | -------------------------------------------------------------------------- |
| `repoId`         | `string`                     |     nein | Kanonische Repository-ID `owner/name`.                                     |
| `displayName`    | `string`                     |     nein | Anzeigename mit Fallback auf `repoId`.                                     |
| `author`         | `string`                     |       ja | Autor/Owner, sofern Hub ihn liefert.                                       |
| `license`        | `string`                     |       ja | Lizenzkennung oder `null`; UI zeigt dann „nicht angegeben“.                |
| `downloads`      | `number`                     |       ja | Hub-Metrik, nur als Zusatzinformation.                                     |
| `files`          | `HuggingFaceFileCandidate[]` |     nein | Nur installierbare GGUF-Kandidaten.                                        |
| `sourceRevision` | `string`                     |       ja | Aufgelöste Commit-SHA des Detailstands, falls verfügbar.                   |
| `revisionRef`    | `string`                     |       ja | Branch oder Tag, dessen aktuellen Stand die Verwaltung später prüfen kann. |

Repositories ohne `files` werden aus dem installierbaren Ergebnisstrom
ausgeschlossen.

### `HuggingFaceFileCandidate`

Konkrete installierbare Datei innerhalb eines Repositories.

| Feld                 | Typ                                   | Nullable | Beschreibung                                                                                            |
| -------------------- | ------------------------------------- | -------: | ------------------------------------------------------------------------------------------------------- |
| `repoId`             | `string`                              |     nein | Quelle des Artefakts.                                                                                   |
| `filename`           | `string`                              |     nein | Einzelner Dateiname ohne Pfadtraversal.                                                                 |
| `revision`           | `string`                              |     nein | Vor dem Download aufgelöste konkrete Commit-SHA.                                                        |
| `revisionRef`        | `string`                              |       ja | Ursprünglich ausgewählter Branch/Tag; `null` bei direktem SHA-Pin.                                      |
| `sizeBytes`          | `number`                              |       ja | Hub-Größe, falls vorhanden.                                                                             |
| `quantization`       | `string`                              |       ja | Aus Metadaten/Dateiname normalisierte Quantisierung.                                                    |
| `contextWindow`      | `number`                              |       ja | Nur wenn belastbar aus Metadaten ableitbar.                                                             |
| `tokenizerRepo`      | `string`                              |       ja | Sicher ermitteltes Tokenizer-Repository.                                                                |
| `tokenizerRequired`  | `boolean`                             |     nein | `true`, wenn Nutzerangabe vor Installation nötig ist.                                                   |
| `fit`                | `fits \| tight \| too_big \| unknown` |     nein | Bestehender Hardware-Fit-Klassifikator.                                                                 |
| `catalogMatch`       | `boolean`                             |     nein | Exakter Repository-/Dateiname-Treffer eines Katalogeintrags.                                            |
| `catalogEntryId`     | `string`                              |       ja | Verlinkbarer Katalogeintrag bei `catalogMatch`.                                                         |
| `metadataProvenance` | `object`                              |     nein | Herkunft von Quantisierung/Kontext: `hub_metadata`, `filename_heuristic`, `gguf_header` oder `unknown`. |

`filename` wird nicht als lokaler Pfad interpretiert. Die Download-URL wird im
Backend aus validierter Repository-ID, Revision und Dateiname aufgebaut.

### `InstalledModel`

Die bestehende lokale Modellansicht erhält die Integritätsdaten aus der
gemeinsamen Modellzeile.

| Feld              | Typ                                | Nullable | Beschreibung                                     |
| ----------------- | ---------------------------------- | -------: | ------------------------------------------------ |
| `id`              | `string`                           |     nein | Lokale Modell-ID.                                |
| `fileSha256`      | `string`                           |       ja | Erwarteter Hash der gespeicherten lokalen Datei. |
| `integrityStatus` | `verified \| untrusted \| unknown` |     nein | Sichtbarer Zustand für die Modellverwaltung.     |

`fileSha256` ist der vollständige SHA-256-Hash des Datei-Inhalts. Er wird nicht
aus Hugging-Face-Commit-SHAs, Dateigröße oder Dateiname abgeleitet.

### `ModelIntegrity`

Persistierter Zustand der Datei-Integritätsprüfung innerhalb von
`InstalledModel`.

| Feld           | Typ                                | Beschreibung                                                                                          |
| -------------- | ---------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `fileSha256`   | `string?`                          | Erwarteter SHA-256-Hash oder `null`, wenn für einen historischen Eintrag noch keiner gespeichert ist. |
| `status`       | `verified \| untrusted \| unknown` | Ergebnis bzw. bestätigter Ausnahmezustand der letzten Nutzerentscheidung.                             |
| `actualSha256` | `string?`                          | Nur transient im Fehlerdialog; wird nicht als neue Referenz persistiert.                              |

### `HuggingFaceInstallRequest`

Nicht persistierte Installationsabsicht aus der UI.

| Feld            | Typ        | Beschreibung                                                                                                                        |
| --------------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `repoId`        | `string`   | Validierte öffentliche Repository-ID.                                                                                               |
| `filename`      | `string`   | Aus dem Ergebnis ausgewählter GGUF-Dateiname.                                                                                       |
| `revision`      | `string?`  | Angeforderter Branch, Tag oder SHA; bei Abwesenheit wird der Default-Ref verwendet und anschließend auf eine SHA aufgelöst.         |
| `name`          | `string`   | Nutzer-/Hub-Anzeigename.                                                                                                            |
| `tokenizerRepo` | `string`   | Validiertes Repository für lokale Tokenisierung.                                                                                    |
| `contextWindow` | `number?`  | Bekannter Wert, sonst `null`.                                                                                                       |
| `forceTooBig`   | `boolean`  | Nur nach expliziter Warnungsbestätigung erlaubt.                                                                                    |
| `forceRepair`   | `boolean?` | Nur für die explizite Integritätsreparatur; umgeht den identischen Source-Short-Circuit und lädt die gespeicherte HF-Quelle erneut. |

Die Backend-Grenze löst `revision` bzw. den Default-Ref vor dem Download in eine
konkrete Commit-SHA auf und erzeugt daraus die bestehende interne
Download-Argumentform. Ein vom Frontend übergebener lokaler Pfad ist nicht Teil
dieses Contracts.

### `HuggingFaceUpdateStatus`

Nicht persistierter Status der Update-Prüfung für ein installiertes HF-Modell.

| Feld                | Typ       | Nullable | Beschreibung                                                                                        |
| ------------------- | --------- | -------: | --------------------------------------------------------------------------------------------------- |
| `modelId`           | `string`  |     nein | Deterministische, kollisionsresistente lokale Modell-ID aus Repository-ID und Dateiname.            |
| `repoId`            | `string`  |     nein | Öffentliches HF-Repository.                                                                         |
| `revisionRef`       | `string`  |     nein | Geprüfter Branch oder Tag; Statusobjekte existieren nur für Modelle mit gespeichertem Upstream-Ref. |
| `installedRevision` | `string`  |     nein | Persistierte Commit-SHA.                                                                            |
| `latestRevision`    | `string`  |       ja | Aktuelle Commit-SHA, falls erfolgreich ermittelt.                                                   |
| `updateAvailable`   | `boolean` |     nein | `true`, wenn `latestRevision` von `installedRevision` abweicht.                                     |
| `checkedAt`         | `string`  |     nein | Zeitpunkt der letzten Prüfung als ISO-8601-Wert.                                                    |
| `errorCode`         | `string`  |       ja | Retrybarer Fehler bei Offline-/HTTP-/Hub-Problemen.                                                 |

## Persistenzänderung: `models`

Die bestehende `models`-Zeile bleibt die gemeinsame Metadaten-Entität für
Provider- und lokale Modelle. Die Implementierung prüft vor der Migration, ob
folgende Quellefelder bereits vorhanden sind; fehlende Felder werden als
additive, CRDT-kompatible Migration ergänzt:

| Feld               | Typ    |     Nullable | Beschreibung                                                                                                                      |
| ------------------ | ------ | -----------: | --------------------------------------------------------------------------------------------------------------------------------- |
| `hf_repo`          | `TEXT` |           ja | Hugging-Face-Repository für lokale GGUFs. Für API-Provider `NULL`.                                                                |
| `hf_filename`      | `TEXT` |           ja | Ausgewählte GGUF-Datei. Für API-Provider `NULL`.                                                                                  |
| `hf_revision`      | `TEXT` |           ja | Beim Download verwendete konkrete Commit-SHA.                                                                                     |
| `hf_revision_ref`  | `TEXT` |           ja | Optionaler Branch/Tag für spätere Update-Prüfungen.                                                                               |
| `file_sha256`      | `TEXT` |           ja | Erwarteter SHA-256-Hash der vollständigen lokalen Modell-Datei, exakt 64 Hex-Zeichen.                                             |
| `integrity_status` | `TEXT` | nein/Default | `verified`, `untrusted` oder `unknown`; ein bewusst akzeptierter Mismatch wird als `untrusted` markiert.                          |
| `source_kind`      | `TEXT` | nein/Default | `catalog`, `huggingface`, `imported` oder `provider`; bestehende Provider-Zeilen werden mit dem Literal `provider` zurückgefüllt. |

`tokenizer_repo` und `context_window` werden weiterverwendet. Die bestehende
`models.id` bleibt die Referenz für Preferences, Chat und lokalen Dateispeicher.
Für freie HF-Modelle wird sie als `hf-` plus kleingeschriebener Hex-SHA-256-
Digest der versionierten, längenpräfixierten UTF-8-Kodierung von `(repoId,
filename)` gebildet. Die kanonische Kodierung besteht aus dem festen Präfix
`haex-hf-model-id-v1`, je einem Big-Endian-`u32`-Byte-Längenpräfix und den
jeweiligen UTF-8-Bytes von `repoId` und `filename`. Bei bereits vorhandener ID muss zusätzlich das
persistierte Repository-/Dateiname-Paar übereinstimmen; andernfalls wird der
Download als Konflikt abgelehnt.

Die additive Migration setzt für alle bereits vorhandenen Modellzeilen ohne
`source_kind`, die zu einem bestehenden API-/Provider-Modell gehören,
`source_kind = 'provider'`. Neue Provider-Schreibpfade verwenden denselben
Default explizit; Katalog-, HF- und Import-Schreibpfade setzen jeweils
`catalog`, `huggingface` bzw. `imported`. Listing und Persistenz geben den
gespeicherten Wert unverändert weiter und dürfen Provider-Zeilen nicht als
`catalog` oder `huggingface` klassifizieren.

Persistenzregeln:

- Ein erfolgreicher Download veröffentlicht Datei und Metadaten als eine
  journalierte, failure-atomare Transaktion; kein Zwischenzustand mit nur
  neuer Datei oder nur neuen Metadaten darf nach außen sichtbar bleiben.
- Ein fehlgeschlagener Download darf keine neue `models`-Zeile hinterlassen.
- Ein Katalogeintrag bleibt `source_kind = catalog` und erhält seine bekannten
  Katalogfelder.
- Ein lokaler Import bleibt `source_kind = imported`; HF-Felder bleiben leer.
- Das Löschen der lokalen Datei entfernt nicht die synchronisierte Metadaten-
  zeile, entsprechend Spec 002.
- Source-Metadaten enthalten keine Tokens, Zugangsdaten oder lokalen absoluten
  Pfade.
- Ein HF-Modell mit `hf_revision_ref` kann einen transienten
  `HuggingFaceUpdateStatus` erhalten; `latestRevision` und
  `updateAvailable` werden nicht als neue lokale Modelldatei persistiert.
- Ein Update verwendet dieselbe `models.id`; die alte Datei und SHA bleiben bis
  zum erfolgreichen atomaren Austausch gültig.
- Der Updatepfad schreibt vor `upsert_model` ein Recovery-Journal mit Backup
  und räumt es erst nach konsistenter Dateipublikation und Metadatenprüfung auf;
  der Start bereinigt unvollständige Transaktionen durch Wiederherstellung des
  alten oder Abschluss des vollständig neuen Datei-/Metadatenpaars.
- Ein lokaler Load liest die kanonische Datei und `file_sha256` in derselben
  Ladeoperation; der berechnete Hash wird nicht aus Dateigröße, mtime oder
  Dateiname abgeleitet.
- Ein Hash-Mismatch oder ein Fehler beim Lesen/Hashen führt zu einem
  Integritätsdialog und verhindert den normalen Runtime-Load.
- Bei ausdrücklicher Wahl „trotzdem laden“ bleibt `file_sha256` unverändert und
  `integrity_status` wird auf `untrusted` gesetzt. Die nächste Prüfung darf
  erneut nach einer Entscheidung fragen.
- Das Setzen einer neuen erwarteten SHA ist nur Bestandteil eines erfolgreichen
  Downloads, Updates oder Imports; ein Load darf sie niemals aktualisieren.

## Zustandsübergänge

```text
idle
  └─ submit valid query → searching → results | search_error

results
  └─ select repository → details_loading → file_candidates | details_error

file_candidates
  ├─ select compatible file → preview
  ├─ select too_big file → warning → explicit_confirm | cancelled
  └─ missing tokenizer → tokenizer_input → preview

preview
  └─ resolve revision → downloading → publishing → installed | download_error

installed
  └─ verify file before load → verified → runtime_load
                             ├─ integrity_unknown → user_decision
                             ├─ integrity_mismatch → user_decision
                             └─ integrity_error → user_decision

user_decision
  ├─ load anyway → untrusted → runtime_load
  ├─ repair source → downloading/importing → publishing → verified
  └─ choose another model → model_picker

update_available
  └─ explicit install → preview → resolve revision → downloading → publishing
     → installed | download_error
```

`download_error` und `search_error` bewahren die zuvor vorhandenen Ergebnis-
und Installationsdaten. `installed` aktualisiert die gemeinsame lokale Liste;
es gibt keine eigene Hugging-Face-Registry.

## Beziehungen und Invarianten

- `HuggingFaceModelResult` 1:N `HuggingFaceFileCandidate`.
- Ein `HuggingFaceFileCandidate` 1:1 zu einer finalen lokalen GGUF-Datei nach
  erfolgreicher Installation.
- `InstalledModel` bleibt dieselbe Frontend-Entität wie bei Katalogmodellen.
- Jeder lokale Slug enthält höchstens eine kanonische finale GGUF-Datei gemäß
  `models::paths::canonical_model_file`.
- Kein Suchresultat darf direkt als lokaler Dateipfad verwendet werden.
- Eine installierte Datei muss ohne erneute Hub-Anfrage geladen werden können.
- `chat.last_active_model_id`, Defaults und Resolver-Fallback ändern sich durch
  die Installation allein nicht.
- Eine erfolgreiche HF-Installation speichert immer eine Commit-SHA; ein
  mutable `revisionRef` ist optional und dient ausschließlich der
  Update-Erkennung.
- Ein Update ersetzt die Datei unter derselben Modell-ID erst nach erfolgreicher
  temporärer Übertragung und Validierung.
