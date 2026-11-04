# Tauri-Vertrag: Hugging-Face-Discovery und Installation

Alle Rückgaben verwenden `camelCase`. Das Backend liefert strukturierte Werte
und Fehlercodes bzw. Parameter, keine lokalisierten UI-Texte.

## `search_huggingface_models`

**Args**:

```typescript
{
  query?: string,
  cursor?: string,
  limit?: number
}
```

**Returns**: `{ results: HuggingFaceModelResult[], nextCursor: string | null }`. Die
Liste ist seitenweise: `nextCursor` der letzten Seite kommt als `cursor` in den
nächsten Aufruf zurück; `null` heißt, die Liste endet.

Wenn `query` gesetzt ist, wird sie getrimmt und auf mindestens zwei Zeichen
validiert. Wenn `query` fehlt, liefert das Backend die meistgeladenen
öffentlichen GGUF-Chat-Modelle als initiale Entdeckungsansicht; Hugging Face
wird dafür mit `filter=gguf` und `filter=conversational` (Repositories mit
Chat-Template, auch multimodale) nach Downloads absteigend sortiert angefragt. Eine Seite hat höchstens 20 Treffer bei einer
Suche und 10 in der Entdeckungsansicht; der Cursor stammt aus dem `Link`-Header
des Hubs.
Beide Varianten werden deterministisch sortiert. Treffer ohne installierbare
GGUF-Datei werden nicht zurückgegeben; falls Datei-Details einen zweiten
Request benötigen, liefert der Command zunächst normalisierte Repository-
Treffer mit `files` nach der Detailanreicherung. Exakte Katalogtreffer tragen
`catalogMatch: true` und `catalogEntryId`, bleiben aber eigenständige
Suchresultate.

**Fehler**: strukturierte `InvalidInput`, `Network`, `Timeout`, `HttpStatus`
oder `RateLimited`-Fehler. Ein Fehler darf die vorherige Frontend-Ergebnisliste
nicht löschen.

## `get_huggingface_model_details`

**Args**:

```typescript
{
  repoId: string,
  revision?: string
}
```

**Returns**: ein `HuggingFaceModelResult` mit allen installierbaren
`HuggingFaceFileCandidate`s, Dateigrößen soweit verfügbar, normalisierter
Quantisierung, Kontextfenster, Metadaten-Provenienz und Tokenizer-Hinweisen.

Repository-ID und Revision werden vor dem HTTP-Aufruf validiert. Private oder
gated Antworten werden nicht als installierbares Ergebnis ausgegeben.

## `preview_huggingface_install`

**Args**:

```typescript
{
  repoId: string,
  filename: string,
  revision?: string,
  tokenizerRepo?: string,
  contextWindow?: number | null
}
```

**Returns**:

```typescript
{
  modelId: string,
  name: string,
  repoId: string,
  filename: string,
  revision: string,
  revisionRef: string | null,
  sizeBytes: number | null,
  quantization: string | null,
  contextWindow: number | null,
  tokenizerRepo: string | null,
  tokenizerRequired: boolean,
  catalogMatch: boolean,
  catalogEntryId: string | null,
  metadataProvenance: {
    quantization: 'hub_metadata' | 'filename_heuristic' | 'gguf_header' | 'unknown',
    contextWindow: 'hub_metadata' | 'filename_heuristic' | 'gguf_header' | 'unknown'
  },
  fit: 'fits' | 'tight' | 'too_big' | 'unknown',
  requiresExplicitTooBigConfirmation: boolean
}
```

Der Preview-Command lädt keine Modelldatei herunter und schreibt keine
Persistenz. Er validiert Quelle/Dateiname, löst eine mutable Revision in eine
konkrete Commit-SHA auf, berechnet die lokale ID und ermittelt die
Hardware-Warnung. Ein fehlendes Tokenizer-Repository wird als
`tokenizerRequired: true` ausgegeben. Eine nicht auflösbare Revision führt zu
einem strukturierten Fehler.

## `download_model_from_hf` (bestehend, erweitert)

Der bestehende Command bleibt der einzige Downloadpfad für Hugging-Face-GGUFs.
Die Argumente werden um Source-/Revision-Felder erweitert. Der Backend-Adapter
übernimmt vor dem Aufruf die normalisierte Vorschau: Er validiert Repository-ID
und Dateiname, löst die gewünschte Revision in eine Commit-SHA auf, übernimmt
die daraus berechnete `modelId` und erzeugt daraus `DownloadFromHfArgs`. Ein
vom Frontend gelieferter `id`-Wert ist nur zulässig, wenn er exakt dieser
Preview-`modelId` entspricht. Die Commit-SHA ist nach der Auflösung
verpflichtend:

```typescript
{
  id: string,
  name: string,
  hfRepo: string,
  hfFilename: string,
  hfRevision: string,
  hfRevisionRef?: string | null,
  tokenizerRepo: string,
  contextWindow?: number | null,
  forceTooBig?: boolean,
  forceRepair?: boolean
}
```

Verhalten:

1. Repository-ID, Dateiname, Commit-SHA, optionaler Upstream-Ref, Slug und
   Tokenizer werden an der Trust-Boundary validiert. Ein Download ohne
   aufgelöste Commit-SHA ist ungültig. `id` muss die aus demselben validierten
   Repository-/Dateiname-Paar abgeleitete `modelId` sein; abweichende IDs
   werden vor Netzwerkzugriff und Veröffentlichung abgelehnt.
2. `TooBig` wird ohne `forceTooBig === true` abgelehnt; `Fits`, `Tight` und
   `Unknown` behalten die im Preview ausgewiesene Semantik.
3. Der Download schreibt in eine temporäre Datei, emittiert den bestehenden
   `model-download-progress`-Payload und berechnet den vollständigen
   `fileSha256`. Die Datei wird erst über den gemeinsamen
   `download_to_file`-/`register_downloaded`-Pfad veröffentlicht. Bricht der
   Response-Body während desselben Vorgangs ab, darf `download_to_file` aus der
   `.part`-Datei fortsetzen; ein `Range`-Retry ist nur mit `ETag` oder
   `Last-Modified` der vorherigen Antwort und dem zugehörigen `If-Range`
   zulässig. Ohne Validator beginnt der Retry bei Byte 0.
   Eine `206 Partial Content`-Antwort wird nur angehängt, wenn ihr
   `Content-Range` am angeforderten Offset beginnt und Ende, Gesamtumfang sowie
   ein vorhandener `Content-Length` konsistent sind. Fehlende oder
   widersprüchliche Bereichsmetadaten verwerfen den Teilstand und lösen einen
   Neustart oder einen strukturierten Fehler aus; `Content-Length` allein darf
   bei einem Resume nicht als Gesamtumfang verwendet werden. Die Datei wird
   nur bei exakt vollständigem Gesamtumfang veröffentlicht. Diese Header werden
   nicht als Modellmetadaten persistiert.
4. `register_downloaded` macht Dateiveröffentlichung und
   `models_store::upsert_model` failure-atomic: Vor einem Update legt es die
   bisherige Datei und Metadaten in einem dauerhaft geschriebenen
   Recovery-Journal mit Backup ab. Das Journal enthält die alten und neuen
   SHA-/Source-Metadaten sowie den erwarteten Dateistatus. Erst nach
   erfolgreichem `upsert_model` wird die neue Datei atomar an den kanonischen
   Pfad publiziert; bei Fehlern oder Neustart stellt die Recovery entweder das
   alte Paar oder das vollständig neue Paar her. Ein erfolgreich abgeschlossener
   Vorgang entfernt Backup und Journal erst danach. So können weder neue Bytes
   mit alter SHA noch alte Bytes mit neuer SHA bestehen bleiben.
5. Nach erfolgreicher Transaktion wird genau eine gemeinsame `models`-Zeile
   mit Source-Metadaten sichtbar und `model-download-complete` emittiert.
6. Fehler hinterlassen keinen installierten Eintrag. Bereits installierte
   Modelle und deren Metadaten bleiben erhalten.
7. Eine identische bereits installierte Quelle wird idempotent behandelt. Ein
   vorhandener Slug mit anderer finaler GGUF-Datei wird als Konflikt abgelehnt.
   Die ausdrückliche Integritätsreparatur darf diese Abkürzung mit
   `forceRepair === true` umgehen und lädt die gespeicherte Quelle erneut.
8. Ein Update mit derselben Modell-ID ersetzt die bestehende Datei erst nach
   erfolgreicher atomarer Veröffentlichung und schreibt danach die neue SHA;
   bei Fehlern bleiben alte Datei und Metadaten gültig.

**Fehler**: `InvalidInput`, `UnsupportedFormat`, `TokenizerRequired`,
`HardwareConfirmationRequired`, `Network`, `Timeout`, `HttpStatus`, `Io` oder
`ModelRegistrationFailed` mit maschinenlesbaren Parametern.

## Bestehende Commands ohne Contract-Bruch

- `list_installed_models`: bleibt autoritativ für lokale Dateien und liefert
  auch frei installierte Modelle. Jeder Eintrag liefert zusätzlich
  `fileSha256` und `integrityStatus`.
- `load_model`: lädt über dieselbe lokale ID und denselben kanonischen Pfad;
  keine erneute Hub-Suche. Vor dem Load gilt die Integritätsprüfung oben.
- `delete_installed_model`: löscht die lokale Modell-Datei und den
  Installations-/Download-Datensatz, behält aber die Source-/Provider-
  Metadatenzeile gemäß Spec 002. Der fehlende lokale Pfad ist danach
  autoritativ: `list_installed_models` lässt den Eintrag aus, `load_model`
  liefert `ModelNotFound`, und eine spätere Installation darf dieselbe Quelle
  erneut registrieren.
- `download_model_from_catalog`: bleibt auf kuratierte Qwen3-Einträge begrenzt
  und verwendet weiterhin den gemeinsamen HF-Downloadpfad. Der Adapter ruft
  dafür dieselbe Preview-/Revisionsauflösung auf, übernimmt die aufgelöste
  Commit-SHA und die Preview-`modelId` in `DownloadFromHfArgs`; ein zweiter
  Datei-Downloadpfad ist nicht zulässig.
- `check_huggingface_model_updates`: prüft installierte HF-Modelle mit
  gespeichertem `hfRevisionRef` gegen Hugging Faces aktuellen Commit und
  liefert `HuggingFaceUpdateStatus[]`. Installierte HF-Modelle ohne
  `hfRevisionRef`, insbesondere direkte SHA-Pins, werden nicht in dieses Array
  aufgenommen, weil für sie kein verfolgbarer Branch oder Tag existiert. Das
  Frontend verwendet dieselbe Semantik: `checkUpdatesAsync()` liefert keinen
  Platzhalterstatus für solche Modelle; sie bleiben als installiertes Modell
  sichtbar, aber ohne automatische Update-Erwartung. Offline-/HTTP-Fehler sind
  retrybar und ändern keine lokalen Modelldaten.

`HuggingFaceUpdateStatus`:

```typescript
{
  modelId: string,
  repoId: string,
  revisionRef: string,
  installedRevision: string,
  latestRevision: string | null,
  updateAvailable: boolean,
  checkedAt: string,
  errorCode: string | null
}
```

## `load_model` und lokale Integritätsprüfung

`load_model` behält die bestehende Modell-ID als Argument. Für lokale Modelle
führt das Backend unmittelbar vor dem Runtime-Load diese Reihenfolge aus:

1. kanonische lokale Datei auflösen;
2. vollständigen SHA-256-Hash der Datei in einem Blocking-Task berechnen;
3. mit `models.file_sha256` vergleichen;
4. nur bei exakter Übereinstimmung den normalen Runtime-Loader starten.

Bei fehlender Datei, fehlendem Hash, Hash-Mismatch oder Hashing-Fehler startet
kein normaler Load. Stattdessen wird ein strukturierter Fehler geliefert:

```typescript
{
  code: 'ModelIntegrityMismatch' | 'ModelIntegrityUnknown' | 'ModelIntegrityError',
  modelId: string,
  expectedSha256: string | null,
  actualSha256: string | null,
  options: ['load_untrusted', 'repair_source', 'choose_other']
}
```

Die UI muss diesen Fehler als Entscheidungsdialog darstellen. Das Verbot gilt
für den normalen Ladepfad. Die explizite
Aktion `load_untrusted` ruft einen separaten, bestätigungspflichtigen Pfad
`load_model_with_integrity_override(modelId)` auf. Dieser markiert
`integrityStatus = 'untrusted'`, behält `fileSha256` unverändert und lädt die
aktuell vorgefundene Datei. `repair_source` verwendet bei HF-Modellen den
gespeicherten Source-Contract und öffnet bei importierten Modellen den lokalen
Dateiimport; `choose_other` öffnet den Modell-Picker. Keine dieser Aktionen darf
die erwartete SHA stillschweigend aktualisieren.

- `model-download-progress` und `model-download-complete`: Payload-Struktur
  bleibt kompatibel.

## Frontend-Composable-Vertrag

`useModels` bleibt der bestehende Installations- und Löschadapter:

```typescript
downloadFromHfAsync(args: HuggingFaceInstallRequest): Promise<InstalledModel>
deleteAsync(modelId: string): Promise<void>
```

`downloadFromHfAsync` ruft nach der Vorschau den gemeinsamen
`download_model_from_hf`-Command auf und mappt die UI-Absicht intern in
`DownloadFromHfArgs`. Dabei sind die Preview-`modelId` und die aufgelöste
Commit-SHA als `id` bzw. `hfRevision` verpflichtend; ein optionaler
`hfRevisionRef` wird ebenfalls übernommen. `deleteAsync` löscht die lokale
Datei und den Installations-/Download-Datensatz, behält die
Source-/Provider-Metadatenzeile und lässt fehlende Dateien aus der installierten
Liste aus; Laden liefert `ModelNotFound`, eine spätere Installation bleibt
zulässig.

`useHuggingFace()` kapselt:

```typescript
searchAsync(query?: string, page?: number): Promise<HuggingFaceModelResult[]>
detailsAsync(repoId: string, revision?: string): Promise<HuggingFaceModelResult>
previewInstallAsync(args: PreviewInstallArgs): Promise<InstallPreview>
downloadAsync(args: DownloadFromHfArgs): Promise<InstalledModel>
checkUpdatesAsync(): Promise<HuggingFaceUpdateStatus[]>
```

`checkUpdatesAsync()` enthält ausschließlich Modelle mit gespeichertem
`hfRevisionRef`; direkte SHA-Pins werden wie im Backend-Contract ausgelassen.

Alle Promises werden von aufrufenden Komponenten behandelt; bei unmount werden
Event-Listener wie beim bestehenden `useModels`-Composable entfernt.
