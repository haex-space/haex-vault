# Data Model: ADR-Korpus bereinigen

## ADR-Inventareintrag

Jede bestehende ADR wird für die Bereinigung anhand dieser Felder bewertet:

| Feld                  | Bedeutung                                                              |
| --------------------- | ---------------------------------------------------------------------- |
| `adr_path`            | Projekt-relativer Pfad und eindeutige Datei, auch bei doppelter Nummer |
| `decision`            | Kurzbeschreibung der weiterhin geltenden Entscheidung                  |
| `spec_sources`        | Specs und Verträge, die die Entscheidung normativ tragen               |
| `inbound_references`  | Specs, Pläne, Aufgaben und ADRs, die auf diese ADR verweisen           |
| `unique_rationale`    | Gründe, Alternativen oder Folgen, die nicht in den Spec-Quellen stehen |
| `governance_required` | Ob eine Verfassungsregel eine ADR-Aufzeichnung verlangt                |
| `disposition`         | `keep`, `shorten`, `consolidate` oder `remove`                         |
| `required_updates`    | Verweisänderungen oder Inhalt, der vor einer Entfernung umzuziehen ist |

## Regeln

- `remove` ist nur gültig, wenn `unique_rationale` leer ist, `governance_required` falsch ist und alle eingehenden Links aktualisiert wurden.
- `shorten` lässt einen knappen Entscheidungsvermerk zurück und verweist auf die normative Spec.
- `consolidate` verschiebt fehlende normative Regeln und nötige Begründung in eine Spec, bevor der ADR-Eintrag entfernt wird.
- Technische Verträge, Formatdateien und Testvektoren sind eigene Entitäten und werden unabhängig von ADRs bewertet.
