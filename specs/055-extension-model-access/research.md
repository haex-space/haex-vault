# Research: Erweiterungen nutzen holzis Modelle

Grundlage: `spec.md` (mit den Klärungen vom 2026-10-17), eine Code-Erkundung vom 2026-10-17 (Stellen unten
geprüft) und der Modelltest in `~/Projekte/holzi-ocr-eval` (nicht im Repo; Ergebnisse unter R10 und R11
zusammengefasst). Pfade `S/` stehen für `src-tauri/src/`.

## R1 — `agent_tasks` wird zu `model_jobs` umgebaut, nicht ergänzt

- **Entscheidung**: Das Modul `S/agent_tasks/` (Spec 047, PR #410) wird in `S/model_jobs/` umbenannt und
  verallgemeinert. Behalten werden: die Registry (jetzt `JobRegistry`, Besitzer-Schlüssel `JobOwner{extension_id, frame}`,
  Abbruch-Token, `cancel_owned_by`), die Haken beim Schließen und Neuladen eines Rahmens
  (`extensions/commands/frames.rs`), die Eintragung in `extensions/bridge/allowlist.rs`, das Abschluss-Ereignis
  samt Nutzlast und die Fehlercodes (Einwilligung, Fähigkeit, Grenze). Ersetzt werden: `TaskInput`,
  `definitions.rs`, `validation.rs` (alle fest auf Papiernotizen) und `fixture.rs`.
- **Begründung**: Der Rest ist Lebenszyklus und taugt für jeden Auftrag. Alles, was ein Modell berührt, ist
  Fixture (`bridge.rs`: „Real profile resolution is deliberately added behind the same lifecycle“). Weder
  das SDK noch das Frontend rufen `extension_ai_task_*` auf, deshalb entfällt eine Übergangslösung (holzi hat
  noch keine Nutzer).
- **Namen**: Methoden `extension_model_job_start`, `extension_model_job_cancel`, `extension_model_list`;
  Ereignis `haextension:model-job:completed`.
- **Zu beheben**: Fertige Einträge blieben in der Registry (`state.rs`, kein Entfernen). Das war ein
  Speicherleck und widerspricht FR-028; jetzt verschwindet der Eintrag, wenn der Auftrag endet (R9).
- **Verworfen**: Neben `agent_tasks` ein zweites Modul bauen. Es gäbe zwei Wege mit zwei Registries.

## R2 — Auftrag, Schritt und Eingabe

- **Entscheidung**: Ein Auftrag besteht aus 1 bis 4 Schritten. Ein Schritt trägt `id`, `needs` (Fähigkeiten,
  heute `image` oder `text`), `instruction` (Anweisung der Erweiterung, höchstens 16 KiB), `input` (Text,
  Bilder oder `from`), `format` (Name, Version, Schema), optional `effort` und `maxOutputTokens`. `from`
  verweist mit einem JSON-Pointer (RFC 6901) auf einen Teil des Ergebnisses eines früheren Schritts; dieser
  Teil wird dem Modell als Text übergeben. Vertrag: `contracts/job-format.md`.
- **Begründung**: FR-009 verlangt, dass ein Schritt das Ergebnis des vorigen bekommt. Ein JSON-Pointer ist
  die kleinste Form, die „nur die Textzeilen“ ausdrücken kann, ohne eine Abfragesprache einzuführen.
- **Verworfen**: (a) Das ganze Ergebnis weiterreichen: bei Seitenerkennung ginge dann auch das Bild-Gerüst in
  den Korrekturschritt. (b) Die Erweiterung steuert die Schritte einzeln mit eigenen Aufträgen: sie könnte dann
  zwischen den Schritten Modelle wechseln und FR-025 (alle Schritte vorab prüfen) nicht gelten.

## R3 — Berechtigungen: zwei neue Arten, keine Schemaänderung

- **Entscheidung**: `PermissionKind` bekommt `Model` (`model`) und `ModelDestination` (`modelDestination`);
  `ALL` wächst von 11 auf 13 (`S/extensions/permissions/model.rs:34`). Zeilen in `extension_permissions`:
  - `kind=model`, `target=<models.id>` oder `*`, `action=<höchster Aufwand>` aus `none | low | medium | high`
    (geordnet; höher deckt niedriger ab, `Action::covers`).
  - `kind=modelDestination`, `target=<provider id>`, `action=send`. Lokale Ziele brauchen keine Zeile.
- **Begründung**: Das Zeilenmodell (`kind, action, target, status`) trägt beides ohne Migration. Die Abfrage
  läuft über den vorhandenen Weg: Handler gibt `PermissionPromptRequired` (1004) mit Einzelheiten zurück,
  `bridge/permissions.rs::ask` fragt, `extension_permission_resolve` speichert mit `remember` eine Zeile oder
  hält die Entscheidung bis zum Ende der Tresorsitzung (`PermissionState::hold`). Der bestehende Holder ist
  derzeit prozessweit; die Lieferung muss ihn beim Schließen der Tresorsitzung leeren, damit FR-004 und FR-016
  tatsächlich „Sitzung oder dauerhaft“ bedeuten.
- **Folgen**: `Action` und `Target` brauchen Parser für beide Arten, `manifest_map.rs` bekommt „keine
  Manifestkategorie“ wie `AgentTool`, der Dialog (`PermissionRequestDialog.vue`) bekommt zwei Zweige, die
  Einstellungen listen beide Arten (`ExtensionPermissionsView.vue`).
- **Grenze des Weges**: Das Wiederholen nach 1004 setzt voraus, dass die Erweiterung die Methode erneut ruft.
  Mitten in einem laufenden Auftrag gibt es keinen wartenden Weg. Das ist gewollt: Alle Fragen stellt der
  Start (R4); Ausweichmodelle ohne Einwilligung werden ausgelassen (FR-027), nie mittendrin erfragt.
- **Verworfen**: Eine eigene Tabelle für Freigaben. Sie würde Sync, Widerruf und Anzeige neu bauen, die
  `extension_permissions` schon hat.

## R4 — Ablauf beim Start (Vorprüfung)

- **Entscheidung**: `extension_model_job_start` prüft vor jedem Modellaufruf in dieser Reihenfolge:
  1. Eingabe und Grenzen (R9). Verstoß: `LimitExceeded` mit dem Namen der Grenze.
  2. Je Schritt die Kandidatenliste: freigegebene Modelle der Erweiterung, in der Rangfolge (R5), die
     `needs` erfüllen. Die Erreichbarkeit wird erst beim Versuch geprüft und darf keinen Kandidaten vorab
     aus der Liste entfernen, damit Ausfälle in der Herkunft und im Ausweichweg sichtbar bleiben. Leere Liste:
     gibt es gar keine Freigabe oder passt keines zu `needs`, fragt der Start nach einer Freigabe (1004 `model`,
     mit Modellauswahl im Dialog); sonst `NoCapableModel`.
  3. Für den ersten Kandidaten jedes Schritts mit Ziel außerhalb des Geräts: fehlt die Einwilligung, fragt der
     Start (1004 `modelDestination`). Die Abfrage enthält eine Zusammenfassung der Daten dieses Schritts
     (Text-/Bildarten und Größen), Ziel und Ort; Inhalte werden nicht in der Berechtigungsentscheidung gespeichert.
  4. Ausweichkandidaten ohne Einwilligung fallen aus der Liste.
  5. Erst dann startet der Lauf im Hintergrund.
- **Begründung**: FR-025 (alle Schritte vorab) und FR-015/FR-017 (nie ohne Einwilligung, auch nicht nach einem
  Fehler). Alle Fragen liegen im synchronen Teil, wo der 1004-Weg funktioniert (der Handler ist `fn(&CallContext,
&Value)`, `dispatch.rs:35`).
- **Der Dialog** zeigt bei `model` die Modelle, die zu `needs` passen, mit Ort (R6) und einer Auswahl des
  höchsten Aufwands; Voreinstellung ist das Standardmodell des Nutzers, sofern es passt.

## R5 — Rangfolge pro Erweiterung

- **Entscheidung**: Neue Tabelle `extension_model_order(id, extension_id, model_id, position)`, vom Sync erfasst
  (Migration `0037_extension_model_access`, `HOLZI_TRIGGER_VERSION` 25; die Nummer 0036 war mit Spec 053 vergeben). Neue Freigaben hängen ans Ende.
  Ohne Eintrag gilt die Reihenfolge der Freigaben nach Anlagezeit.
- **Begründung**: FR-007 verlangt eine Rangfolge des Nutzers pro Erweiterung. Berechtigungszeilen haben keine
  Ordnung. Eine kleine Tabelle ist einfacher als eine Ordnungszahl, die in `action` oder `target` versteckt
  wird.
- **Verworfen**: Eine globale Rangfolge. Der Nutzer will pro Erweiterung bestimmen (Spec, Story 2).

## R6 — Ort eines Ziels

- **Entscheidung**: Jedes Modell hat einen Ort: `this_device`, `network` (anderes Gerät oder Server im
  eigenen Netz), `cloud`. Ableitung (`S/model_jobs/place.rs`): `ProviderKind::Local`, Loopback, `0.0.0.0`/`::`
  und genau der Name `localhost` sind `this_device`; RFC 1918, Link-Local, IPv6-ULA, `100.64.0.0/10` (Carrier-NAT,
  Tailscale), Namen auf `.local`, `.lan` und `.localhost` (nicht jeder Resolver hält RFC 6761 ein) und Namen
  ohne Punkt (`school-server`) sind `network`; eine IPv4-Adresse in IPv6 (`::ffff:10.0.0.1`) zählt als ihre
  IPv4-Adresse. Alles andere, eine unlesbare Adresse und alle `CliDelegate`-Anbieter sind `cloud`. Einwilligung
  (`modelDestination`) verlangt jeder Ort außer `this_device`; eine Fehleinordnung zwischen `network` und
  `cloud` ändert daher nur die Anzeige.
- **Begründung**: Die vorhandene `Locality` (`S/chat/tools/agent_task.rs`) kennt nur Local und Cloud und zählt
  jeden `ApiKey`-Anbieter als Cloud. Ein Ollama auf `localhost` verließe aber kein Byte das Gerät (SC-003),
  und ein Schul-Server im Heimnetz ist weder dieses Gerät noch Cloud. Die Anzeige braucht die Unterscheidung
  (Story 4: „das Ziel nennen“).
- **Grenze**: Ein Name, der in ein privates Netz auflöst, aber öffentlich erreichbar ist, wird nach der
  Adresse beurteilt, nicht nach dem Weg. Der Nutzer sieht Adresse und Ort im Dialog und kann ablehnen.

## R7 — Allgemeiner OpenAI-kompatibler Anbieter

- **Entscheidung**: Neuer Adapter `S/adapters/openai_compatible.rs`, Adapter-Kennung `openai_compatible`
  (Eintrag in `validate_adapter`, `build_adapter` und `add_provider` in `S/providers/mod.rs`). Umfang:
  - `GET {base}/models` für die Modelliste.
  - `POST {base}/chat/completions` mit `stream: true` (SSE-Zeilen `data:`, Ende `[DONE]`), `delta.content`,
    `delta.reasoning_content`, stückweise `tool_calls`, `usage`.
  - Bilder als `image_url` mit Base64-Data-URI.
  - `reasoning_effort` aus `ChatRequest.reasoning_option`.
  - `Authorization: Bearer` nur mit Schlüssel: der Schlüssel ist optional (`key_is_optional`), weil ein Ollama
    oder ein Schul-Server im eigenen Netz oft keinen will. Fehlerzuordnung wie bei Anthropic (401/403 →
    `InvalidCredentials`, 429/5xx vorübergehend, TLS über `transport_error`); ein Fehlerobjekt im Stream ist
    vorübergehend, wenn `type` oder `code` Überlast, Ratenlimit oder Serverfehler nennt oder `code` 429/5xx ist.
    Ein Datenstück, das kein JSON ist, beendet den Stream; Teilaufrufe ohne `index` (Gemini) werden nach `id`
    getrennt.
- **Fähigkeiten**: `/models` meldet sie fast nie. Der Adapter liefert `ModelCapabilities` unbestimmt; der Nutzer
  macht je Modell eines OpenAI-kompatiblen Anbieters Angaben (`set_model_user_capabilities`, FR-019): „nimmt
  Bilder an“ und die Aufwandstufen als freie Kennungen des Servers (höchstens 8, je höchstens 32 Zeichen, ohne
  Leerraum, keine doppelt, niedrigste zuerst). Die Angaben liegen in `ModelCapabilities.user`, überleben eine
  Aktualisierung der Modellliste und lassen sich zurücknehmen. Modelle anderer Anbieter nehmen keine Angabe an:
  ihre Fähigkeiten meldet der Anbieter selbst, und eine zurückgenommene Angabe brächte sie nicht zurück.
- **Gemini**: Google bietet eine OpenAI-kompatible Schnittstelle (Beta) unter
  `https://generativelanguage.googleapis.com/v1beta/openai/` mit Modellliste, Streaming, Bildern als
  `image_url`, `reasoning_effort` (`minimal`, `low`, `medium`, `high`) und Bearer-Schlüssel; geprüft an der
  Seite ai.google.dev/gemini-api/docs/openai am 2026-10-17. Damit braucht Gemini keinen eigenen Adapter.
- **Verworfen**: Je Anbieter ein eigener Adapter (Gemini, Ollama, vLLM). Die gemeinsame Schnittstelle deckt
  alle und den Schul-Server (Spec: „nur ein weiterer Anbieter“).

## R8 — Lokale Modelle mit Bildern

- **Befund**: `LocalModel::load` nutzt nur `GgufModelBuilder` (`S/llm/local/loader.rs`); `stream.rs::build_request`
  liest `Attachment`s nicht, Bilder fallen still weg; `ModelCapabilities::local()` setzt
  `accepted_attachment_kinds: Some(vec![])`. mistral.rs 0.8.1 hat Bildmodelle (`qwen3_vl`, `qwen2_5_vl`,
  `gemma3`, `mistral3` u. a.) über `MultimodalModelBuilder` und `RequestBuilder::add_image_message(role, text,
Vec<DynamicImage>)`; das GGUF-Format hat keine Bildmodelle, sie laden als Safetensors mit
  Quantisierung beim Laden (`with_isq`) oder als `.uqff`.
- **Plan vor dem Versuch** (nach T057 verworfen): ein zweiter Ladeweg `LocalModel::load_multimodal(dir, isq)`
  neben dem GGUF-Weg, Bildanhänge als `DynamicImage` über `add_image_message`, Katalogfelder `format`
  (`gguf` | `multimodal_dir`), `vision` und `isq` mit dem Download eines Verzeichnisses, Fähigkeiten lokaler
  Modelle aus dem Katalog. Vorher sollte ein Versuch zeigen, ob Qwen3-VL-4B über mistral.rs mit 4-Bit-ISQ auf
  der Entwicklungskarte (8 GB) läuft und eine Seite liest. Mit transformers und bitsandbytes (NF4) waren es
  3,1 GB Gewichte und 3,7 GB Spitze.
- **Messung (T057, 2026-10-21)**: `S/llm/local/multimodal_spike_tests.rs`, Qwen3-VL-4B-Instruct über
  `MultimodalModelBuilder`, Release-Build mit `llm-cuda` (ohne `flash-attn`), RTX A2000 Laptop 8 GB, gierig
  dekodiert, höchstens 3000 neue Tokens, gleiche Anweisung wie die Python-Messung. Jede Seite ergibt 3259
  Prompt-Tokens, die mit rund 180 Tokens/s gelesen werden.

  | ISQ                    | VRAM geladen / Spitze | Seite 007 (Text)           | Seite 005 (lange Liste)         | Seite 002 (Zeichnung)                          |
  | ---------------------- | --------------------- | -------------------------- | ------------------------------- | ---------------------------------------------- |
  | Q4K                    | 4,2 / 6,2 GB          | 60 s, 270 Tokens, richtig  | 684 s, Schleife bis 3000 Tokens | 685 s, Schleife aus Zeilenumbrüchen            |
  | Q8_0                   | 6,1 / 7,8 GB          | 113 s, 544 Tokens, richtig | 141 s, 690 Tokens, richtig      | nach 392 s kein Grafikspeicher mehr (Schleife) |
  | Python NF4 (Vergleich) | 3,1 / 3,7 GB          | 18,8 s                     | 23,2 s                          | 3,1 s (nur ein Bildbereich)                    |
  | Ollama Q4_K_M          | 5,1 / 5,5 GB          | 15 s, 543 Tokens, richtig  | 18 s, 688 Tokens, richtig       | 13 s, 409 Tokens, zehn Bildbereiche            |
  - Die Antworten erzeugt mistral.rs mit 4,5–6,5 Tokens/s, gleich bei Q4K und Q8_0, im Debug- und im
    Release-Build. Die Karte ist dabei zu 96–100 % ausgelastet und läuft am Leistungslimit (60 W). Es bremst
    also die Rechenarbeit auf der Karte, nicht der Prozessor. Das ist 3–6-mal langsamer als transformers mit NF4.
  - Wenn eine Seite gelesen wird, stimmt der Text mit der Python-Messung überein. Q4K neigt aber stärker zu
    Schleifen als NF4. Q8_0 lässt neben sich keinen Platz auf einer 8-GB-Karte.
  - Das Laden dauert 7–27 s; die Quantisierung beim Laden läuft auf allen Prozessorkernen.
  - Ollama v0.40.1 mit `qwen3-vl:4b-instruct` (Q4_K_M) über `/v1/chat/completions`, also den Weg des
    Adapters `openai_compatible`, mit `temperature: 0`, `max_tokens: 3000` und `OLLAMA_CONTEXT_LENGTH=8192`:
    alle drei Seiten als gültiges JSON und mit dem Text der Python-Messung, 33–38 Tokens/s über die ganze
    Seite. `qwen3-vl:4b` ohne Zusatz ist die denkende Fassung: Sie verbrauchte alle 3000 Tokens beim Denken und
    gab leeren Inhalt zurück.

- **Entscheidung (2026-10-21): Rückfall.** Lokale Bildmodelle laufen als OpenAI-kompatibler Anbieter auf
  `localhost` (Ollama oder llama.cpp-Server); holzi bekommt keinen zweiten Ladeweg. Der Ort ist
  `this_device` (R6), also ohne Einwilligung, und der Nutzer setzt am Modell „nimmt Bilder an“ (FR-019). Die
  Einrichtung ist Sache des Nutzers (`quickstart.md`). Das deckt FR-018 und FR-020 ab, ohne die Spec zu verletzen.
  - Begründung: mistral.rs liest richtig, braucht aber 4–8-mal so lange wie Ollama und mit Q8_0 fast die ganze
    Karte; Q4K gerät in Schleifen, die erst an der Längengrenze enden.
  - Der Versuch bleibt als ignorierter Test liegen (Q4K oder `HOLZI_SPIKE_ISQ_BITS=8`) und wird nach einem
    mistral.rs-Update wiederholt. Nicht versucht: das Feature `flash-attn`; das Erzeugen der Antwort hängt
    aber schon an der Karte.
  - Die Frage nach einem lokalen Platz (ein Auftrag verdrängt das Chatmodell aus `ChatState`) entfällt damit.
    Ollama hält sein Modell selbst und gibt es nach `OLLAMA_KEEP_ALIVE` (Voreinstellung 5 min) frei. Auf einer
    8-GB-Karte teilen sich holzis Chatmodell und das Bildmodell den Grafikspeicher.

## R9 — Grenzen und nichts speichern

- **Grenzen** (FR-029): Konstanten in `S/model_jobs/limits.rs`, Werte im Plan:
  - Bildgröße höchstens 25 MiB je Bild (wie `MAX_IMAGE_BYTES`), höchstens 8 Bilder je Auftrag, Anweisung höchstens
    16 KiB, Eingabetext höchstens 256 KiB je Schritt (`input_bytes`), Ergebnis höchstens 256 KiB.
  - Dauer: Voreinstellung 300 s je Auftrag (lokale Bildmodelle brauchten im Test bis zu 130 s je Seite).
    Der Nutzer ändert sie je Erweiterung in der vorhandenen Limits-Ansicht: neues Feld `model_job_timeout_ms` in
    `ExtensionLimits` (`S/extensions/registry/limits.rs`) mit Grenzen in `MIN`/`MAX`.
  - Höchstens 2 laufende Aufträge je Rahmen (`limits::MAX_ACTIVE_JOBS_PER_FRAME`; ersetzt mit der Registry
    `MAX_ACTIVE_TASKS_PER_FRAME` in `state.rs`, ebenso `MAX_IMAGE_BYTES` im alten `bridge.rs`).
    Überschreitung der Bildgrenzen lehnt vor der Verarbeitung ab; die Dauer bricht den Auftrag ab und gibt den
    Lauf frei (Abbruch über das Token, bei Anbietern durch Verwerfen des `AdapterStream`).
- **Nichts speichern** (FR-028, SC-011): Aufträge existieren nur im Arbeitsspeicher; die Registry entfernt einen
  Eintrag nach dem Abschluss-Ereignis. Bilder und Texte stehen in keinem `tracing`-Feld. Test: Nach einem Auftrag
  vergleicht ein Test den Inhalt aller Tabellen vorher und nachher (nur Berechtigungen dürfen sich ändern).

## R10 — Prüfung der Ausgabe

- **Entscheidung**: Ein allgemeiner Prüfer `S/model_jobs/guard.rs`, unabhängig vom Format:
  1. leer oder nur Leerraum;
  2. **Schleife** (Zahlen in `contracts/job-format.md`): zu wenig verschiedene Wörter bei langem Text, dieselbe
     Wortfolge oder dieselbe Zeile mehrfach hintereinander;
  3. **Schriftwechsel**: mindestens ein Fünftel der Buchstaben in einem Schriftsystem, das weder Latein ist noch in
     Anweisung oder Eingabe vorkommt (Beispiel: CJK in einer deutschen Anweisung); nur Buchstaben in Wörtern mit
     mindestens zwei Buchstaben dieser Schrift zählen, so dass `α = 30°` in einer Physiknotiz keiner ist;
  4. kein gültiges JSON oder Verstoß gegen das Schema (R11).
     Jeder Verstoß ist `Unusable` mit Grund; danach greift der Ausweichweg (FR-027).
- **Begründung aus dem Test**: Qwen3-VL-4B lief auf einem unscharfen Poster in eine Schleife („1.00 Uhr“
  hundertfach), PaddleOCR-VL lieferte dort englische Sätze, chinesische Zeichen und erfundene Formeln. Die
  Schleife fängt Regel 2, die chinesischen Zeichen fängt Regel 3. Die erfundenen englischen Sätze und Formeln
  fängt keine Regel; deshalb bleibt die Prüfung ausdrücklich auf „offensichtlich kaputt“ begrenzt (Spec,
  Randfälle) und verspricht keine Richtigkeit.
- **Für lokale Modelle** zusätzlich: Wiederholungsstrafe 1,1 und ein hartes `maxOutputTokens` (der Test zeigte
  unbegrenzte Schleifen ohne Obergrenze). Wo mistral.rs es kann, begrenzt `RequestBuilder::set_constraint` die
  Ausgabe auf das Schema (`Constraint::JsonSchema`, 0.8.1) und verhindert Schleifen vor allem bei JSON.
- **Verworfen**: Ein zweites Modell prüft das erste. Es verdoppelt Kosten und Zeit für einen kleinen Gewinn.

## R11 — Schema-Prüfer: ein eigener, größerer als der der Aktionen

- **Befund (geprüft am 2026-10-17)**: Der Schema-Prüfer von Spec 018 liegt nicht in `extensions/agent_tools/defs.rs`,
  sondern in `chat/tools/action_tool.rs::schema_in_subset` und kennt nur `type`, `description`, `properties`,
  `required`, `items`, `enum`. Das Frontend spiegelt ihn in `src/lib/actions/schema.ts`; er darf nicht wachsen.
  `chat/eval/scoring.rs::valid_against` prüft einen Wert, gehört aber zur Bewertung der Modelle.
- **Entscheidung**: Aufträge bekommen `model_jobs/schema.rs` mit `check_subset(schema)` und `validate(schema,
value)`. Die Teilmenge ist größer (Grenzen, `additionalProperties`, `null`-Paar, `enum` ohne `type`), der Fehler
  nennt den ersten Verstoß mit Pfad. Ein Objekt mit `properties` erlaubt keine weiteren Schlüssel, außer
  `additionalProperties` ist `true`.
- **Begründung**: Die Seitenerkennung braucht `maxItems`, `minimum`, `maximum` und optionale Felder. Den
  Aktionsprüfer dafür zu erweitern ändert eine Teilmenge, die Manifeste und Frontend teilen.
- **Verworfen**: Eine vollständige JSON-Schema-Bibliothek (Verweise, Rekursion, mehr Angriffsfläche).

## R12 — Anweisung der Erweiterung ist nicht vertraut

- **Entscheidung** (FR-026): Aufträge laufen ohne Werkzeuge. `ChatRequest.tools` bleibt leer; es gibt keinen
  Dateizugriff und keinen Befehl. Das Modell sieht nur `instruction`, `input` und das Schema. Text im Foto kann
  die Ausgabe verändern, aber nie eine Handlung auslösen; die Ausgabe wird gegen das Schema geprüft und geht
  nur an die Erweiterung.
- **Befehlszeilen-Agenten** (`cli_delegate`, etwa Claude Code oder Codex) waren bis 2026-10-23 nie Kandidaten
  eines Auftrags, weil sie mit eigenen Werkzeugen und Dateizugriff arbeiten. Seit der Klärung vom 2026-10-23
  sind sie Kandidaten mit dem Zugriff, den sie in holzi haben (FR-032, R16). Für alle anderen Modelle gilt
  weiter: keine Werkzeuge.
- **Zugang zu Schlüsseln**: Adapter werden bei Bedarf aus `providers`+`models` gebaut (`build_adapter`); die
  Erweiterung sieht weder Schlüssel noch Adressen (FR-021). Fehlertexte des Anbieters werden vor der Rückgabe
  bereinigt (`S/model_jobs/errors.rs::sanitize`): der Schlüssel und die Adresse des Anbieters, die der Aufrufer
  kennt, URLs, IP-Adressen, Host mit Port, Namen des eigenen Netzes und schlüsselartige Zeichenketten werden
  ersetzt, der Rest auf 200 Zeichen gekürzt. Den Antwortkörper gibt der Lauf nie weiter, er nimmt nur die
  Meldung des Adapters.

## R13 — SDK und Erweiterung

- **Entscheidung**: Das vault-sdk (eigenes Repo) bekommt `sdk.models.list()`, `sdk.models.run(job)` und
  `sdk.models.cancel(id)` mit Ereignis. Es ist eine eigene Lieferung mit eigenem PR; holzi pinnt die veröffentlichte
  Version (Verfassung IV). Die Seitenerkennung in haex-notes (Spec 046) ist ein dritter Schritt im Repo
  `haextension` und gehört nicht zu dieser Spec.
- **Test ohne SDK**: Eine Test-Erweiterung im Repo ruft die Brücke direkt wie `agent-tools.xt` in Spec 018.

## R14 — Lieferungen

|     | Inhalt                                                                                                                        | Wert allein                                       |
| --- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| A   | Anbieter `openai_compatible` (R7), „nimmt Bilder an“ je Modell                                                                | Nutzbar im Chat, auch für Gemini und Schul-Server |
| B   | Auftragskern (R1–R6, R9–R12): Brücke, Berechtigungsarten, Rangfolge, Wächter, Grenzen, Ausweichweg, Anwendung auf Fernmodelle | Erweiterungen können Fernmodelle nutzen           |
| C   | Lokale Bilder (R8): Versuch, Ladeweg, Katalog, Fähigkeiten                                                                    | Vollständig lokale Auswertung                     |
| D   | Oberfläche: Freigabe- und Einwilligungsdialog, Modelle und Rangfolge in den Einstellungen, Dauer                              | Der Nutzer bestimmt alles selbst                  |
| E   | vault-sdk und Test-Erweiterung (R13)                                                                                          | Entwickler können es nutzen                       |
| F   | Jedes Modell aus holzi (US8, R15, R16): lokal installierte Modelle und Abos als Quellen von Aufträgen                         | Kein zweites Einrichten als Modell-Server nötig   |
| G   | Weg zur Freigabe (US9, R17): Modell-Seite aus der Erweiterung öffnen, Freigabe vor dem ersten Auftrag                         | Kein Suchen nach der richtigen Einstellung        |

B hängt von A nur für die Fernmodelle ab; C ist unabhängig und der größte Unsicherheitsfaktor, daher
frühzeitig der Versuch. Reihenfolge: A, B (mit D für die Dialoge), C, E. F kam am 2026-10-23 dazu und baut
auf B und D auf; lokale Modelle (R15) und Abos (R16) sind zwei getrennte PRs. G (R17) kam am selben Tag dazu
und hängt nur von B und D ab: holzi zuerst, dann vault-sdk, dann haex-notes.

## R15 — Lokal installierte Modelle als Quelle

- **Befund**: `candidates::job_models` listet lokale Modelle schon für die Freigabe; erst der Lauf scheitert,
  weil `model_source::Providers` für `ProviderKind::Local` „local models cannot take jobs yet“ liefert. Der
  Chat hält genau ein lokales Modell im Speicher (`chat/model_loading.rs::load_model_inner`, nur mit
  `llm-cpu`); ein zweites daneben passt auf einer 8-GB-Karte nicht.
- **Entscheidung**: Ein lokales Modell wird Quelle für Aufträge über den einen Platz des Chats, wie T062 es
  für Bildmodelle geplant hatte: Ist es schon geladen, nutzt der Auftrag es zwischen zwei Chat-Zügen; sonst
  wartet der Auftrag, bis kein Zug läuft, lädt es (mit der Prüfsumme wie im Chat) und der Chat lädt sein
  Modell beim nächsten Zug wieder (FR-033). Die Wartezeit zählt zur Höchstdauer. Fähigkeiten wie im Chat:
  GGUF-Modelle nehmen heute keine Bilder (`ModelCapabilities::local()` sagt für alle „keine“), Bildschritte
  übergehen sie (FR-019). Das ist der Stand des Ladewegs, kein Grundsatz: Sobald ein lokales Modell Bilder
  lesen kann, kommt die Fähigkeit aus dem Modell oder dem Katalog (`vision`, T061) statt pauschal, und es
  erscheint für Bildschritte wie jedes andere; die Auswahl dafür hängt nur an `fits`, nicht an der Quelle.
  Offen bleibt ein Weg, der schnell genug ist (R8: mistral.rs war 4–8× langsamer als Ollama). Ohne `llm-cpu`
  gibt es keine lokalen Modelle und damit keinen Kandidaten.
- **Verworfen**: Ein eigener Platz für Aufträge (doppelter Grafikspeicher); den Chat für einen Auftrag
  unterbrechen (verfälscht den Zug, FR-033).

## R16 — Abos als Quelle

- **Befund**: `candidates::job_models` filtert `ProviderKind::CliDelegate` heraus, `model_source::Providers`
  lehnt sie ab. `build_adapter` baut einen Abo-Adapter nur mit `DelegateChatContext` (Freigaben im Chat,
  Ereignisse, Kindprozesse). Jeder Aufruf läuft in einem eigenen temporären Verzeichnis
  (`cli_delegate/claude.rs`, `codex.rs`, `current_dir(tmp)`); Anhänge liegen dort als Datei, die der Agent
  mit seinem Lesewerkzeug öffnet. Die Antwort kommt als Text, den die Prüfung des Auftrags wie jede andere
  Ausgabe gegen das Format prüft (R2, R5).
- **Entscheidung** (Clarifications 2026-10-23, korrigiert 2026-10-24): Abos sind Kandidaten wie andere Modelle
  (`candidates::job_models` filtert sie nicht mehr). Der Adapter wird mit einem Kontext ohne Chat gebaut
  (`model_jobs/delegate.rs`: keine wartenden Freigaben, kein Ereignis, kein Tresor, Kindprozesse in der
  Registry der Tresorsitzung wie im Chat) und fragt den Agenten mit `Ungated`, ohne Thread. Der Plan, den
  Modus des Chats zu übernehmen und `standard` als `gated_permissive` zu behandeln, scheitert an
  `approval_bridge.rs`: `gated_permissive` protokolliert jeden Aufruf in einem Chat-Thread und verweigert, wenn
  kein Thread da ist (Test `gated_permissive_denies_when_there_is_no_thread_to_audit_against`); ein Auftrag
  darf nichts speichern (FR-028) und hat keinen Thread. `standard` würde auf eine Freigabe warten, die
  niemand erteilt. Den Dateizugriff braucht Claude Code schon, um ein Bild zu lesen. Der Ort ist `cloud`
  (Dienst des Anbieters), also mit Einwilligung (R6). Fähigkeiten: Claude Code nimmt Bilder (Anhänge liegen als
  Datei im Arbeitsverzeichnis, die Modelle melden Bildfähigkeit über die Modell-API); Codex schreibt keine
  Anhänge und meldet nichts, es gilt daher nur für Textschritte, bis der Nutzer „Nimmt Bilder an“ setzt. Der
  Aufwand geht als `--effort` mit. Die Freigabe zeigt beim Abo einen Hinweis auf Datei- und Befehlszugriff
  (FR-032, `agent` in `ExtensionModelChoice` und `ExtensionModelEntry`).
- **Verworfen**: Abos ohne Werkzeuge (`claude --tools ""`); der Nutzer will ihnen den vollen Zugriff lassen,
  und für Codex gibt es laut `codex exec --help` (0.147.0) keinen Schalter ohne Shell.

## R17 — Weg zur Freigabe aus der Erweiterung

- **Befund**: Eine Erweiterung kann holzis Einstellungen nicht öffnen; das vault-sdk hat dafür nichts, und
  `DeepLink` (Benachrichtigungen) sowie die Tab-API führen nur innerhalb der eigenen Erweiterung. Die Freigabe
  entsteht erst in der Vorprüfung des ersten Auftrags (`preflight.rs::ask_for_model`). haex-notes konnte beim
  Hinweis `no_capable_model` daher nur sagen, wo der Nutzer suchen soll (haextension#132).
- **Entscheidung**: Zwei Methoden (`contracts/bridge.md`). `extension_model_settings_open` meldet per Ereignis
  an das Frontend, das die Einstellungen bei `extensions.detail` der aufrufenden Erweiterung öffnet oder nach
  vorn holt, wie `answerAgentFrameAsync` in `useExtensionHost.ts` einen Tab öffnet.
  `extension_model_access_request` nutzt die Vorprüfung eines Schritts ohne Auftrag: dieselbe Frage, dieselben
  Codes, keine Einwilligung für ein Ziel, nichts gesendet. Im vault-sdk werden daraus
  `models.openSettingsAsync()` und `models.requestAccessAsync({ needs, effort })`; das SDK wiederholt den Aufruf
  nach der Antwort des Nutzers wie bei `runAsync`.
- **Verworfen**: Ein Auswahlmenü in jeder Erweiterung, das holzi nur bestätigt. Die Erweiterung würde bestimmen,
  was der Nutzer sieht und was vorausgewählt ist, und bräuchte die Liste aller Modelle des Tresors; heute kennt
  sie nur ihre eigenen Freigaben (FR-030).
