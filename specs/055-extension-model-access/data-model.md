# Data Model: Erweiterungen nutzen holzis Modelle

Nur der kleinste gespeicherte Teil ist neu: Berechtigungen, eine Rangfolge und ein Grenzwert. Aufträge selbst
werden nie gespeichert (FR-028).

## Gespeichert

### `extension_permissions` (vorhanden): zwei neue Arten, keine Schemaänderung

| Art                | `target`             | `action`                              | Bedeutung                                                                                   |
| ------------------ | -------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------- |
| `model`            | `models.id` oder `*` | `none` \| `low` \| `medium` \| `high` | Die Erweiterung darf dieses Modell mit höchstens diesem Aufwand nutzen. `*` = jedes Modell. |
| `modelDestination` | `providers.id`       | `send`                                | Die Erweiterung darf Bilder und Texte an das Ziel dieses Anbieters senden.                  |

- `status` ist `granted`, `denied` oder `ask`; `denied` schlägt `granted` (wie in `evaluate.rs`).
- Aufwand ist geordnet: `none` < `low` < `medium` < `high`. Eine Zeile mit höherem Wert deckt niedrigere ab
  (`Action::covers`). Die Zuordnung auf die Stufen des Modells steht in `contracts/job-format.md` §Aufwand.
- Beide Arten sind vault-weit (nicht gerätegebunden); sie synchronisieren wie jede Berechtigung
  (`extension_permissions`, Entwicklungsversionen in `dev_extension_permissions_no_sync`).
- „Nur für die Sitzung“ erzeugt keine Zeile. Die Entscheidung liegt im Arbeitsspeicher (`PermissionState::hold`)
  und endet beim Schließen der Tresorsitzung; die Lieferung muss den derzeit prozessweiten Holder an diesen
  Lebenszyklus binden.
- Lokale Ziele (`this_device`) haben nie eine `modelDestination`-Zeile.

### `extension_model_order` (neu, CRDT-getrackt)

| Spalte         | Typ     |               |
| -------------- | ------- | ------------- |
| `extension_id` | TEXT    | Erweiterung   |
| `model_id`     | TEXT    | `models.id`   |
| `position`     | INTEGER | 0 = bevorzugt |

- Primärschlüssel `(extension_id, model_id)`. Neue Freigaben hängen ans Ende; ohne Zeile ordnet die Anlagezeit
  der Freigabe.
- Löscht der Nutzer ein Modell oder widerruft die Freigabe, bleibt die Zeile bis zum nächsten Aufräumen
  folgenlos (nur freigegebene Modelle zählen).
- Migration `0036_extension_model_order`; CRDT-Tracking und `HOLZI_TRIGGER_VERSION` wie bei den anderen
  synchronisierten Tabellen.

### `ExtensionLimits` (vorhanden): ein Feld mehr

| Feld                   | Typ | Voreinstellung | Grenzen (`MIN`/`MAX`) |
| ---------------------- | --- | -------------- | --------------------- |
| `model_job_timeout_ms` | u32 | 300 000        | 10 000 bis 1 800 000  |

Gleiche Ablage wie die übrigen Grenzwerte (`extensions/registry/limits.rs`); die Einstellungen zeigen es in
`ExtensionLimitsView.vue`.

### `providers` und `models` (vorhanden)

- `providers.adapter` bekommt den Wert `openai_compatible` (neben `anthropic`, `claude`, `codex`).
  `base_url` und `credentials` wie bei `api_key`-Anbietern; `credentials` darf fehlen (ein Server im eigenen Netz
  will oft keinen Schlüssel), dann geht keine `Authorization` mit.
- `models.capabilities_json` (`ModelCapabilities`) trägt ein Feld `user` mit der Nutzerangabe („nimmt Bilder
  an“, Aufwandstufen). Die Angabe wird über die gewöhnlichen Felder gelegt (`with_user_applied`), damit alle
  Leser sie sehen, und überlebt das Auffrischen der Modellliste: `replace_provider_models` mischt sie unter die
  neue Auskunft des Anbieters. Eine leere Angabe nimmt sie samt den gesetzten Werten zurück.

### Katalog lokaler Modelle (`S/catalog/model_catalog.json`)

Ein Eintrag bekommt `format` (`gguf` | `multimodal_dir`), `vision` (bool) und bei `multimodal_dir` `isq`
(Quantisierung beim Laden). Bei `multimodal_dir` ist `hf_filename` ein Verzeichnis. Die Tabelle
`device_downloaded_models_no_sync` führt dann `relative_path` auf ein Verzeichnis und `size_bytes` als Summe.

## Nur im Arbeitsspeicher

### `JobRecord` (Registry `model_jobs::state`)

| Feld       | Bedeutung                                           |
| ---------- | --------------------------------------------------- |
| `id`       | UUID des Auftrags                                   |
| `owner`    | `{extension_id, frame}` (wie `TaskOwner`)           |
| `status`   | `running` \| `completed` \| `failed` \| `cancelled` |
| `cancel`   | Abbruch-Token                                       |
| `deadline` | Zeitpunkt, an dem der Auftrag abbricht              |

- Der Eintrag verschwindet nach dem Abschluss-Ereignis. Er trägt keine Eingabe, keine Anweisung, kein
  Ergebnis und keine Herkunft.
- Zustände: `running` → `completed` | `failed` | `cancelled`; kein Zurück.

### Auftrag, Schritt, Herkunft (nur in der Anfrage und im Ergebnis)

Aufbau und Beispiele stehen in `contracts/job-format.md`. Sie liegen in der Anfrage der Erweiterung und im
Ergebnis an sie; holzi hält sie nur für die Dauer des Laufs.

## Zuordnung zu den Anforderungen der Spec

| Spec                   | Hier                                                                                          |
| ---------------------- | --------------------------------------------------------------------------------------------- |
| FR-003, FR-005, FR-006 | `extension_permissions` Art `model`; Wirkung ab nächster Anfrage, weil jede Anfrage neu prüft |
| FR-007, FR-027         | `extension_model_order`, Kandidatenliste je Schritt                                           |
| FR-015, FR-016, FR-017 | `extension_permissions` Art `modelDestination`                                                |
| FR-018, FR-019, FR-020 | `providers.adapter`, `capabilities_json`, Katalogfelder                                       |
| FR-028                 | Nur `JobRecord` im Arbeitsspeicher, nach Abschluss entfernt                                   |
| FR-029                 | `ExtensionLimits.model_job_timeout_ms`, feste Obergrenzen in `limits.rs`                      |
