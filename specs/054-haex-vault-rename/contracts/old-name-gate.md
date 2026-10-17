# Vertrag: Prüfung des alten Namens (CI)

Setzt FR-004 und FR-005 um. Landet in Stufe S8, nachdem S2–S7 den Namen entfernt haben.

## Aufruf und Ort

- Skript: `scripts/ci/check-old-name.py` (neben `check-docs.py`), Test: `scripts/ci/test_check_old_name.py`.
- Aufruf im Auftrag „Documentation consistency“ von `.github/workflows/ci.yml`, damit keine neue
  Pflichtprüfung für `main` nötig ist. Es läuft auf `pull_request` und auf `push`.
- Ausnahmeliste: `scripts/ci/old-name-allowlist.txt` (Format unten).
- Lokal: `python3 scripts/ci/check-old-name.py` ohne Argumente; Rückgabewert 0 = sauber.

## Was geprüft wird

Alle von Git **verfolgten** Dateien (`git ls-files`), jeweils zweimal:

1. der **Pfad** (Verzeichnis- und Dateiname),
2. der **Inhalt**, nur von Textdateien (Binärdateien werden nach einer NUL-Byte-Probe
   übersprungen, ihr Pfad wird trotzdem geprüft).

Gesucht wird `holzi` ohne Beachtung der Groß-/Kleinschreibung, **auch als Teil eines Wortes**
(`HolziError`, `HOLZI_X`, `x-holzi`).

Nicht geprüft: Git-Historie, Tags, Branches, nicht verfolgte Dateien, `node_modules`,
`src-tauri/target`.

## Ausnahmeliste

Eine Zeile pro Eintrag, Felder durch `|` getrennt, Leerzeilen und Zeilen mit `#` am Anfang sind
Kommentare:

```text
<kind> | <pattern> | <reason> | [until]
```

| Feld      | Regeln                                                                                                                                                                                                                                              |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `kind`    | `path` (Glob auf den ganzen Pfad: alle Treffer in diesen Dateien sind erlaubt), `pathname` (Glob auf den Pfad: nur der Pfadname darf den Namen tragen, der Inhalt wird geprüft), `line` (Regex auf eine Zeile: erlaubt diese Zeile, in jeder Datei) |
| `pattern` | Glob (`path`, `pathname`) mit `**` für Verzeichnisse oder Regex (`line`); ein leeres Muster ist ein Fehler                                                                                                                                          |
| `reason`  | Pflicht, nicht leer: ein Satz, warum der Name dort bleibt                                                                                                                                                                                           |
| `until`   | optional, `S10`: nach dieser Stufe wird der Eintrag entfernt; die Prüfung kennt das Feld nur als Hinweis in der Ausgabe                                                                                                                             |

Die Liste ist **abschließend**: Ein Eintrag ohne Treffer ist ein Fehler („unbenutzter Eintrag“),
damit sie nicht über die Zeit verwaist. Die drei Gate-Metadateien selbst (`check-old-name.py`,
`test_check_old_name.py` und `old-name-allowlist.txt`) stehen als explizite `path`-Einträge in der
Liste, weil Implementierung, Tests und Konfiguration den Suchbegriff absichtlich als Prüfdatum
enthalten. Der Anfangsinhalt steht in
[data-model.md](../data-model.md).

## Ausgabe und Rückgabewert

| Fall                                                        | Ausgabe                                                                                                | Rückgabewert |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | ------------ |
| keine Treffer außerhalb der Liste, kein unbenutzter Eintrag | eine Zeile „old name: clean (N files, M allowed)“                                                      | 0            |
| Treffer außerhalb der Liste                                 | je Treffer `path:line: <Zeile>` (Pfadtreffer: `path: <name>`), am Ende Summe und Hinweis auf die Liste | 1            |
| unbenutzter oder fehlerhafter Eintrag der Liste             | Zeile der Liste und Grund                                                                              | 1            |
| Aufruf ohne Git-Repo                                        | Fehlermeldung                                                                                          | 2            |

Die Ausgabe nennt Treffer in Textdateien mit Zeilennummer und höchstens 200 Zeichen der Zeile;
nach 200 Treffern bricht die Liste ab und nennt die Gesamtzahl.

## Tests (`test_check_old_name.py`)

Jeder Test baut ein kleines Git-Repo in einem temporären Verzeichnis:

1. sauberes Repo → 0.
2. „holzi“ in einer Textdatei (Kleinschreibung, Großschreibung, Teil eines Wortes) → 1, mit Pfad
   und Zeile in der Ausgabe.
3. „holzi“ nur im Dateinamen → 1; dieselbe Datei mit `pathname`-Eintrag → 0, aber bei „holzi“ im
   Inhalt der Datei → 1.
4. Binärdatei mit dem Namen im Inhalt → 0; im Pfad → 1.
5. `path`-Eintrag deckt alle Treffer der Datei; `line`-Eintrag deckt nur die Zeile.
6. Eintrag ohne `reason`, mit leerem Muster oder ohne Treffer → 1 mit Begründung.
7. Datei, die nicht verfolgt wird → ignoriert.

## Wirkung auf den Ablauf

Ab S8 ist die Prüfung Teil von „Documentation consistency“ (Pflichtprüfung für `main`). Ein PR, der
den Namen neu einführt, scheitert dort mit den Fundstellen (SC-001). S10 entfernt die zwei Einträge
mit `until: S10`; danach bleiben die sechs dauerhaften Einträge für Spec 054, Verzeichnisname 032,
die drei Gate-Metadateien und die zwei Hinweissätze „früher holzi“ im README und in `CONTEXT.md`.
