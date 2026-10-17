# Contract: Brücke für Modelle (Erweiterung → holzi)

Drei Methoden der Erweiterungs-Brücke (Spec 017, `extensions/bridge/allowlist.rs`), ein Ereignis. Ersetzt
`extension_ai_task_start`, `extension_ai_task_cancel` und `haextension:ai-task:completed` aus Spec 047 (kein
Aufrufer vorhanden). Das Format von Auftrag und Ergebnis steht in `job-format.md`.

Alle Methoden prüfen, dass die Erweiterung aktiv ist (`dispatch::call`), und gelten nur für den aufrufenden
Rahmen.

## `extension_model_list`

Liefert die für die Erweiterung freigegebenen Modelle. Ohne Freigabe: leere Liste, kein Fehler (FR-030).

```text
params: {}
result: {
  models: [ {
    id: string,            // models.id, nur zum Anzeigen und Wiedererkennen
    name: string,          // Anzeigename des Modells
    provider: string,      // Anzeigename des Anbieters
    place: "this_device" | "network" | "cloud",
    capabilities: { image: boolean, effort: "none" | "managed" | "levels" },
    maxEffort: "none" | "low" | "medium" | "high"   // höchster freigegebener Aufwand
  } ]
}
```

- Es kommen weder Adresse noch Schlüssel noch andere Zugangsdaten vor (FR-021).
- `effort: "levels"` heißt, `low`/`medium`/`high` wirken; `managed` heißt, das Modell regelt es selbst.
- Die Liste dient der Anzeige. Das Modell wählt holzi je Schritt nach der Rangfolge (FR-007).

## `extension_model_job_start`

```text
params: { job: Job }                       // siehe job-format.md
result: { jobId: string, status: "running" }
```

Der Rückgabewert kommt sofort; das Ergebnis folgt als Ereignis.

Fehler bei der Vorprüfung kommen als Antwort der Methode (`BridgeError`), nicht als Ereignis. Die Zahlencodes
sind die festen von `ExtensionErrorCode` (das vault-sdk liest sie); der genauere Grund steht in `details.reason`:

| `code`                          | `details.reason`                                                          | Bedeutung                                                                                                                 |
| ------------------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| 3001 `Validation`               | `invalid_input`                                                           | Form von `job` falsch (Schritte, `id`, Schema außerhalb der Teilmenge, `from` verweist ins Leere, Bild nicht dekodierbar) |
| 7000 `LimitExceeded`            | `image_bytes`, `image_count`, `instruction_bytes`, `steps`, `active_jobs` | Grenze überschritten; `details.limit` nennt sie                                                                           |
| 8001 `NotAvailable`             | `no_capable_model`                                                        | Für einen Schritt gibt es kein freigegebenes Modell mit den `needs`                                                       |
| 1002 `PermissionDenied`         | `consent_denied`                                                          | Der Nutzer hat die Freigabe oder die Einwilligung abgelehnt                                                               |
| 1004 `PermissionPromptRequired` |                                                                           | holzi fragt den Nutzer; die Erweiterung ruft nach `extension:permission-resolved` erneut                                  |
| 1000 `SecurityViolation`        |                                                                           | `cancel` mit einer fremden Auftragskennung                                                                                |

`PermissionPromptRequired` trägt `details`:

- `{ resourceType: "model", action: "<Aufwand>", target: "<models.id>" }`: Freigabe eines Modells.
- `modelDestination` trägt für die Einwilligung zusätzlich die Datenübersicht:

  ```text
  { resourceType: "modelDestination", action: "send", target: "<providers.id>",
    transfer: { kinds: ["text"|"image"], textBytes: number,
                imageCount: number, imageBytes: number } }
  ```

  Sie nennt die übertragenen Datentypen und Größen, aber keine Inhalte.

Eine Ablehnung des Nutzers liefert `consent_denied` (1002); bleibt danach kein Modell übrig, `no_capable_model` (8001).

## `extension_model_job_cancel`

```text
params: { jobId: string }
result: { cancelled: boolean }
```

Falscher Besitzer (anderer Rahmen oder andere Erweiterung): `SecurityViolation` (1000). Unbekannte Kennung:
`{cancelled: false}`. Bei Erfolg folgt ein Ereignis mit Status `cancelled`. Ein Abbruch ändert keine Daten der
Erweiterung (FR-022).

## Ereignis `haextension:model-job:completed`

Kommt über den vorhandenen Weg (`extensions/bridge/events.rs::emit_to_frames`) an die offenen Rahmen der
Erweiterung.

```text
data: {
  jobId: string,
  status: "completed" | "failed" | "cancelled",
  result?: { steps: [ { id: string, result: <Wert nach format.schema> } ], final: <Ergebnis des letzten Schritts> },
  provenance: [ { stepId: string, formatName: string, formatVersion: number,
                  attempts: [ { model: string, place: "this_device"|"network"|"cloud",
                                effort: "none"|"low"|"medium"|"high"|"managed",
                                outcome: "used" | "unusable" | "unavailable" | "timeout", reason?: string } ] } ],
  error?: { code: "Unusable"|"Unavailable"|"Timeout"|"Cancelled"|"ConsentDenied"|"NoCapableModel",
            message: string, stepId?: string }
}
```

- `provenance` gibt je Schritt alle versuchten Modelle mit Ausgang an (FR-023, FR-027). `effort` ist der
  tatsächlich verwendete Aufwand nach dem Deckeln (FR-006).
- Fehlercodes des Ereignisses: `Unusable` (Wächter oder Schema, Grund in `message`), `Unavailable` (kein
  Modell erreichbar), `Timeout`, `Cancelled`, `ConsentDenied` und `NoCapableModel` (eine Freigabe wurde während
  des Auftrags entzogen). `Unusable` ist von den anderen unterscheidbar
  (FR-011). `message` enthält nie Adresse, Schlüssel oder Antwortkörper des Anbieters.
- Nach dem Ereignis kennt holzi den Auftrag nicht mehr; die Erweiterung hält Ergebnis und Herkunft selbst.

## Lebenszyklus

- Schließt oder lädt ein Rahmen neu, bricht `cancel_owned_by(TaskOwner)` alle Aufträge dieses Rahmens ab
  (vorhandener Haken in `extensions/commands/frames.rs`); es folgt kein Ereignis an den toten Rahmen.
- Höchstens 2 laufende Aufträge je Rahmen (`MAX_ACTIVE_TASKS_PER_FRAME`); ein dritter: `LimitExceeded`
  (`activeJobs`).
- Entfernt der Nutzer eine Freigabe, während ein Auftrag läuft, endet der laufende Modellaufruf wie vorgesehen;
  der nächste Schritt startet nicht mehr und das Ereignis meldet `NoCapableModel` für diesen Schritt (Randfall
  der Spec).
- Ein Wechsel des Rahmens oder Neustart von holzi verwirft laufende Aufträge ohne Spur.

## Prüfung (Vertragstests)

- Methoden in `METHODS` eingetragen; Besitzerprüfung beim Abbruch; Nutzlast des Ereignisses gegen das Format
  oben; kein Feld der Antworten enthält einen Schlüssel oder eine Adresse (Test mit präparierten Anbieterdaten).
