# Research: ADR-Korpus bereinigen

## Entscheidung 1: Specs tragen normative Anforderungen; ADRs halten Entscheidung und Begründung fest

**Entscheidung**: Eine Spec-Referenz allein belegt keine Redundanz. Entfernt wird eine ADR nur dann, wenn der gesamte noch relevante Inhalt bereits in Specs steht und keine Projektregel eine ADR-Aufzeichnung verlangt.

**Begründung**: Viele vorhandene Specs zitieren ADRs als übergreifende Vorgabe, Sicherheitsbegründung oder Abhängigkeit. Bei mehreren ADRs ist die Referenz Teil des laufenden Architekturverständnisses.

**Alternativen**:

- Alle ADRs löschen, weil Specs vorhanden sind: abgelehnt; dadurch gingen übergreifende Entscheidungen, Gründe und Verweise verloren.
- Alle ADRs unabhängig von Specs behalten: abgelehnt; das lässt nachweislich doppelte Detailtexte parallel bestehen.

## Entscheidung 2: HXEF-Formatvertrag ist separat

**Entscheidung**: `docs/formats/encrypted-folder-v1.md` und seine Testvektoren werden nicht gelöscht oder in eine ADR zurückverlagert.

**Begründung**: Das Dokument ist ein konkreter Formatvertrag mit Leser-/Schreiberregeln und Vektoren, keine bloße Entscheidungsbegründung. Spec 048 und weitere Sync-/Sharing-Arbeiten nutzen es.

## Audit-Inventar

Die folgenden Bewertungen beruhen auf dem Dokumentinhalt und Referenzen in `specs/` und `plans/` am Stand der Spec-Erstellung. „Behalten“ bedeutet: derzeit kein Löschkandidat; keine Behauptung, dass nie eine spätere Zusammenführung sinnvoll wäre.

| ADR                                            | Verweise¹ | Bewertung                                 | Befund                                                                                                                                                                                                                                                    |
| ---------------------------------------------- | --------: | ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0001 Device-scoped data                        |    55 / 6 | Behalten                                  | Übergreifende Daten-/Sync-Konvention; breit in Specs und Plänen referenziert.                                                                                                                                                                             |
| 0002 Local model profiles                      |    11 / 3 | Behalten                                  | Teilweise durch 0010 und 0023 überholt; Plattformgrenzen und Voreinstellungen werden weiter zitiert. Als historische Entscheidung klar gekennzeichnet.                                                                                                    |
| 0003 One vault session per process             |    24 / 5 | Behalten                                  | Prozess-/Sitzungsgrenze wirkt in Chat, Dateien und Sicherheit; viele Folgeverweise.                                                                                                                                                                       |
| 0004 Extension protocol split                  |    21 / 4 | Behalten                                  | Übergreifender Extension-Vertrag; Spec 018 löst eine Richtung, nicht die gesamte Entscheidung ab.                                                                                                                                                         |
| 0005 MCP transport and identity                |    12 / 2 | Behalten                                  | Eigenständige Transport- und Identitätsentscheidung mit Alternativen und Folgen.                                                                                                                                                                          |
| 0006 Built-in agent actions                    |    11 / 6 | Behalten                                  | Grundsatz für Werkzeugausführung; andere Specs markieren konkrete, begründete Ausnahmen.                                                                                                                                                                  |
| 0007 Secrets and grants                        |    19 / 5 | Behalten                                  | Sicherheitsgrenze, die von mehreren Specs und Verträgen vorausgesetzt wird.                                                                                                                                                                               |
| 0008 Extension signature and SQL authorizer    |     6 / 4 | Behalten                                  | Eigenständige Sicherheitsarchitektur mit zwei unabhängigen Prüfschritten.                                                                                                                                                                                 |
| 0009 References and passkey service            |     7 / 2 | Behalten                                  | Teilweise durch 0015 ergänzt/überholt; verbleibender Passkey- und Referenzkontext wird weiter erwähnt.                                                                                                                                                    |
| 0010 Android platform                          |     9 / 4 | Behalten                                  | Plattformübergreifende Architekturentscheidung und Begründung; in Android-Spec verknüpft.                                                                                                                                                                 |
| 0011 Native catalog actions                    |    14 / 3 | Behalten                                  | Quer verwendete Abweichung vom Ausführungsmodell aus 0006; aktiv in Specs referenziert.                                                                                                                                                                   |
| 0012 Public test signing keys                  |     3 / 2 | Behalten                                  | Sicherheitsentscheidung zu Testschlüsseln mit eigenen Folgen und Grenzen.                                                                                                                                                                                 |
| 0013 Encrypted folder format                   |     6 / 5 | Behalten; Formatvertrag separat           | Entscheidung verbindet mehrere Verbraucher; das Formatdokument ist normativ und bleibt separat.                                                                                                                                                           |
| 0014 Invitations from address book             |     9 / 3 | Behalten                                  | Kontakt-/Sync-Basisschichten werden von späteren Specs als Voraussetzung genutzt.                                                                                                                                                                         |
| 0015 File access between devices and contacts  |    14 / 2 | Behalten                                  | Eigene Freigabe- und Authentisierungsgrenze, die Spec 056 weiter konkretisiert.                                                                                                                                                                           |
| 0015 Passkey ceremony confirmation             |    14 / 2 | Behalten                                  | Eigenständige Passkey-Sicherheitsentscheidung; die doppelte Nummer wird in der Umsetzung nicht umnummeriert.                                                                                                                                              |
| 0016 Non-contact devices in public lobbies     |     3 / 2 | Behalten                                  | Eigenständiges Modell für öffentliche Lobbys und nicht verbundene Geräte.                                                                                                                                                                                 |
| 0017 Contacts share envelope                   |     1 / 2 | Behalten                                  | Vorgeschlagene Vertrauens-/Umschlagentscheidung; sie ist noch explizit in Folgedesigns referenziert.                                                                                                                                                      |
| 0018 Sync rules state stays local              |     9 / 2 | Behalten                                  | Übergreifende Geräte-/Sync-Grenze, die Sync-Specs voraussetzen.                                                                                                                                                                                           |
| 0019 Agent browser over CDP pipe               |     3 / 2 | Behalten                                  | Eigenständige Prozess- und Isolationsentscheidung für Browserwerkzeuge.                                                                                                                                                                                   |
| 0020 Spoken secrets bypass model               |     6 / 2 | Behalten                                  | Sicherheitsgrenze für Geheimnisse und lokale Sprachausgabe, in Voice-Specs weiter referenziert.                                                                                                                                                           |
| 0021 Parallel turns and orchestrator           |     3 / 2 | Behalten                                  | Eigenständige Orchestrierungs- und Berechtigungsgrenze; Spec 068 baut darauf auf.                                                                                                                                                                         |
| 0022 No mailto registration in Windows package |     0 / 1 | **Entfernt; konsolidiert in Spec 063 R5** | Der einzige exakte Pfadverweis in T026 wurde ersetzt. Die Windows-Ausnahme, Implementierung, Spike-Ergebnisse und langfristigen Plattformfolgen stehen in Spec 063 R5. ADR 0022 wurde entfernt, nachdem keine externen Verweise auf die Datei verblieben. |
| 0023 Local inference engine                    |     6 / 3 | Behalten                                  | Laufzeit-/Plattformentscheidung samt Einschränkungen und Amendments; Specs 070 und 043 referenzieren sie.                                                                                                                                                 |
| 0024 Chat over Nostr and Blossom               |     2 / 2 | Behalten                                  | Konkretes Cross-vault-Protokoll und Sicherheits-/Verfügbarkeitsfolgen; Spec 072 übernimmt seine Schichten.                                                                                                                                                |
| 0025 Entry types share identity                |     2 / 2 | Behalten                                  | Übergreifender Persistenz-/Sync-Vertrag für neue Typen, von Spec 092 verwendet.                                                                                                                                                                           |

¹ Format: Anzahl der Dateien in `specs/`, `plans/`, `docs/` und `README.md` mit ADR-Nummer / Anzahl der Projektdateien mit exaktem ADR-Dateinamen. Die Zählung ist der Ausgangsstand vor Änderungen und schließt Spec 094 aus. Für die beiden Dateien 0015 sind die Nummernverweise mehrdeutig und deshalb für beide gleich; die exakten Dateinamenverweise unterscheiden sie.

## Konsolidierung von ADR 0022

Spec 063 R5 enthält die Entscheidung, dass Windows `mailto` nicht registriert, weil der Tauri-Bundler den System-Klassenschlüssel ändern und beim Deinstallieren löschen würde. Sie dokumentiert auch die genaue Konfigurationsabweichung, das Verhalten von `register_for_development`, die Linux-/macOS-Grenzen und die Quellen des Spikes. FR-016 beschreibt die Produktanforderung; `tasks.md` T019/T026 dokumentiert die Umsetzung und den Anlass für die ADR.

Die zusätzliche Langzeitfolge aus ADR 0022 — neue systemweite Schemata nicht ohne Plattformprüfung in Windows registrieren und die Bundler-Annahme bei einem Upstream-Upgrade erneut prüfen — wurde in Spec 063 R5 übernommen. T026 bewahrt den historischen Spike-Befund ohne Verweis auf die entfernte Datei.

## Ergebnis nach Umsetzung

- `docs/adr/` enthielt zu Beginn 26 Dateien. Eine wurde entfernt; die verbleibenden 25 sind in der Tabelle einzeln als beibehalten bewertet.
- Die Dateipfad-Suche findet keinen Verweis auf die entfernte ADR-Datei außerhalb dieses Inventars.
- `docs/formats/encrypted-folder-v1.md`, der Abschnitt mit Testvektoren und die Verweise aus Spec 048 sind weiterhin vorhanden.
- `python3 scripts/ci/check-docs.py` meldet „Documentation checks passed“; `git diff --check` ist sauber.
