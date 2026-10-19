# Vertrag: Beispielsatz-Satz und Messbericht

**Spec**: [../spec.md](../spec.md) (FR-019 bis FR-022) | **Datenmodell**: [../data-model.md](../data-model.md)

## Dateien

- `src-tauri/src/chat/eval/eval_set.json`: der versionierte Satz, eingebettet
  per `include_str!`.
- `src-tauri/src/chat/eval/tools.json`: Schnappschuss der
  `AgentActionDef`-Liste aus dem Katalog, erzeugt von
  `scripts/export-eval-tools.ts`. `pnpm check:agent-actions` schlägt fehl,
  wenn der Schnappschuss vom Katalog abweicht.
- Bericht: `target/eval/<modell>.json` (nicht versioniert).

## `eval_set.json`

```json
{
  "version": 2,
  "sentences": [
    {
      "id": "read-tabs-de-1",
      "lang": "de",
      "kind": "read",
      "text": "Welche Tabs sind gerade offen?",
      "expect": [{ "tool": "wm_state_get", "args": {} }],
      "selfTest": true
    },
    {
      "id": "change-scheme-en-1",
      "lang": "en",
      "kind": "change",
      "text": "Switch to the dark color scheme.",
      "expect": [
        {
          "tool": "settings_appearance_setColorScheme",
          "args": { "scheme": "dark" }
        }
      ],
      "selfTest": false
    },
    {
      "id": "smalltalk-de-1",
      "lang": "de",
      "kind": "smalltalk",
      "text": "Wie wird das Wetter morgen?",
      "expect": "none",
      "selfTest": true
    },
    {
      "id": "read-settings-en-1",
      "lang": "en",
      "kind": "read",
      "text": "What are the current settings?",
      "expect": [{ "tool": "settings_get", "args": {} }],
      "selfTest": true
    },
    {
      "id": "change-scheme-de-1",
      "lang": "de",
      "kind": "change",
      "text": "Stelle auf das helle Farbschema um.",
      "expect": [
        {
          "tool": "settings_appearance_setColorScheme",
          "args": { "scheme": "light" }
        }
      ],
      "selfTest": true
    },
    {
      "id": "smalltalk-en-1",
      "lang": "en",
      "kind": "smalltalk",
      "text": "Tell me a joke.",
      "expect": "none",
      "selfTest": true
    }
  ]
}
```

Regeln:

- `lang` ∈ `de`, `en`; `kind` ∈ `read`, `change`, `smalltalk`, `clarify`, `extension`. Ein Satz der
  Art `extension` (Version 4, Spec 018 FR-019) erwartet ein Werkzeug einer Erweiterung aus
  `extension_tools.json` (Name `x_<slug>_<tool>`) statt eines Eintrags von `tools.json`; der Lauf bietet
  dafür im ersten Schritt das passende Angebot und `run_command` an (Spec 018 R14).
- `expect` ist `"none"` (kein Werkzeugaufruf erwartet) oder eine nicht leere
  Liste; die Liste darf mehrere Werkzeugaufrufe eines Batches enthalten. Die
  Reihenfolge zählt nicht, aber die beobachtete Menge muss am bewerteten
  Aktionsschritt exakt mit der erwarteten Menge übereinstimmen: fehlende,
  doppelte und unerwartete Aufrufe sind ein Fehler. Der Lauf bewertet bis zu
  **zwei Schritte**: liegt die erwartete Aktion nicht im Kernangebot, muss der
  erste Schritt genau einen `find_actions`-Aufruf enthalten; der Lauf führt die
  reine Suche über `tools.json` aus (keine Aktion, keine Nebenwirkung), bietet
  die Treffer an und bewertet den vollständigen Aufruf-Batch im zweiten Schritt.
  Längere Ketten sowie Text nach dem bewerteten Schritt werden nicht bewertet.
- `args` enthält nur die Felder, auf die es ankommt; zusätzliche, schema-gültige
  Felder des Modells schaden nicht.
- `selfTest: true` markiert die Teilmenge (≈ 5 Sätze, jeweils Deutsch und
  Englisch, mit mindestens einem `smalltalk`), die der Selbsttest in der App
  nutzt.
- Zielwerte, die von Laufzeitdaten abhängen (Tab-IDs), kommen nicht vor: Sätze
  mit Ziel bewerten nur die Wahl der Aktion und der festen Felder.
- Jeder `expect.tool` muss in `tools.json` vorkommen, bei der Art `extension` in
  `extension_tools.json` (Prüfung in `check:agent-actions`).
- Erhöhung von `version` bei jeder inhaltlichen Änderung; der Selbsttest wird
  dann einmal neu durchgeführt, wenn die Änderung die `selfTest`-Teilmenge
  betrifft.

## Bewertung je Satz

| Ergebnis     | Bedingung                                                                                                                                                        |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pass`       | erwartete und beobachtete Aufrufmenge stimmen exakt überein, jede Eingabe ist schema-gültig und die erwarteten `args` stimmen; bei `"none"`: kein Werkzeugaufruf |
| `wrong_tool` | Aufrufmenge enthält eine andere, fehlende, doppelte oder zusätzliche Aktion                                                                                      |
| `bad_args`   | Aufrufnamen stimmen, aber eine Eingabe ist ungültig oder falsch                                                                                                  |
| `missed`     | erwartet Aufruf, Modell antwortet nur mit Text                                                                                                                   |
| `spurious`   | erwartet `"none"`, Modell ruft trotzdem ein Werkzeug                                                                                                             |
| `not_found`  | die Suche des Modells hat die erwartete Aktion nicht geliefert (zählt gegen die `reachRate`, nicht als falscher Aufruf)                                          |

## `EvalReport`

```json
{
  "setVersion": 2,
  "model": "qwen3-4b-instruct-q4_k_m",
  "total": { "pass": 22, "of": 28, "rate": 0.786 },
  "perLang": {
    "de": { "pass": 11, "of": 14, "rate": 0.786 },
    "en": { "pass": 11, "of": 14, "rate": 0.786 }
  },
  "perKind": {
    "read": { "...": "..." },
    "change": { "...": "..." },
    "smalltalk": { "...": "..." }
  },
  "reachRate": 0.964,
  "extraSteps": 7,
  "spuriousCalls": 1,
  "offered": { "pass": 11, "of": 12, "rate": 0.917 },
  "called": { "pass": 9, "of": 12, "rate": 0.75 },
  "runCommandCalls": 0,
  "extensionSentences": [
    {
      "id": "extension-mail-de-1",
      "offered": true,
      "called": true,
      "ranCommand": false
    }
  ],
  "failures": [
    {
      "id": "change-scheme-en-1",
      "result": "bad_args",
      "got": { "tool": "...", "args": {} }
    }
  ]
}
```

- `rate` = `pass / of`; `reachRate` = Anteil der Sätze mit erwarteter Aktion,
  bei denen die Aktion spätestens im zweiten Schritt zur Verfügung stand
  (Kernangebot oder Suche, SC-005); `extraSteps` = Zahl der Sätze, die einen
  zusätzlichen Suchschritt brauchten (höchstens einer je Satz).
- `offered`, `called` und `extensionSentences` (Spec 018) betreffen nur Sätze der Art `extension`:
  `offered` = das erwartete Werkzeug lag im Angebot des ersten Schritts (das passende Angebot, ohne Modell
  messbar, SC-001), `called` = das Modell hat das erwartete Werkzeug aufgerufen (SC-002; die Eingabe
  beurteilt `total`). `runCommandCalls` zählt Sätze jeder Art, in denen das Modell `run_command`
  versuchte (SC-003).
- Zwei Läufe mit demselben Modell und festem Sampler weichen um höchstens 10
  Prozentpunkte in `total.rate` ab (SC-008). Bei Modellen, die deterministisches
  Sampling nicht zulassen (Cloud), ist das die Mess­grenze und der Bericht
  vermerkt `"deterministic": false`.
