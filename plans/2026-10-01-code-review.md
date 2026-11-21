# HaexVault – umfassendes Code-Review vom 2026-10-01

Stand: `f2ac1befff48feac53e197efcc20f73b3695bc64` (`main`). Alle Fundstellen beziehen sich auf diesen Stand. Quellcode und bestehende Tests wurden nicht geändert. Dieser Bericht ist eine Beratungsgrundlage; nummerierte Specs bleiben normativ.

## Ergebnis und Einordnung

29 Findings betreffen Datenintegrität, Nebenläufigkeit, mehrere Chat-Instanzen, Fehlerverträge, Duplikationen, Testlücken und verbindliche Coding-Regeln. Die höchste Priorität haben der Resync nach Unterbrechung, konkurrierende Resync-Löschungen, die Geräteentfernung, die Veröffentlichung von Modell-Updates und die falsche Zuordnung von Chat-Aktionen/Freigaben.

Die Codebasis besitzt bereits erhebliche Absicherung: getrennte Domänenmodule, ein Vault-Gateway, prozessübergreifende GGUF-Publication-Locks, gezielte Frontend-Harnesses, Rust-Integrationstests und eine mehrteilige CI. Die wesentlichen Schwächen liegen an Übergängen zwischen diesen Modulen: Datenbank ↔ Dateisystem, Transaktionsentscheidung ↔ späterer Commit, Singleton-Store ↔ mehrere Komponenten und UI-Fehleranzeige ↔ maschinenlesbares Action-Ergebnis.

**Priorität:** P1 = vor verlässlicher Nutzung des betroffenen Features beheben; P2 = regulärer Bugfix bzw. wichtige technische Schuld; P3 = nachrangige Wartbarkeit/Regelkonformität. Kein P0 festgestellt. **Aufwand:** S = Stunden, M = ungefähr ein Arbeitstag, L = mehrere Tage, jeweils einschließlich Tests. **Risiko** bezeichnet das Risiko der Korrektur. **Sicherheit:** hoch = konkreter Codepfad bestätigt; mittel = Teil der fachlichen Bewertung noch zu klären. Statisch bestätigte Race-Szenarien sind keine Behauptung eines beobachteten Produktionsvorfalls.

| ID  | Priorität | Finding                                                            | Aufwand | Risiko         |
| --- | --------- | ------------------------------------------------------------------ | ------- | -------------- |
| R01 | P1        | Snapshot-Fortschritt wird vor vollständigem Resync veröffentlicht  | M       | hoch           |
| R02 | P1        | Resync-Löschung entfernt zwischenzeitliche lokale Änderungen       | M       | mittel         |
| R03 | P1        | Geräteentfernung und Empfangsprüfung sind nicht atomar             | M       | mittel         |
| R04 | P1        | Modell-Update besitzt kein dauerhaftes Recovery-Journal            | L       | hoch           |
| R05 | P1        | Chat-Aktionen erreichen den ersten statt des auslösenden Tabs      | M       | mittel         |
| R06 | P1        | Veraltete Thread-Ladung zeigt Freigaben im falschen Gespräch       | S       | niedrig        |
| R07 | P1        | Schließen eines Chats entfernt globale Modell-Listener             | S–M     | mittel         |
| R08 | P2        | Fremde Chat-Ereignisse hinterlassen Freigaben und wachsende Puffer | M–L     | mittel         |
| R09 | P2        | Fehler nach Send-Staging hinterlassen einen Phantom-Erfolg         | M       | mittel         |
| R10 | P2        | Tool-Call und Tool-Result werden separat committed                 | M       | mittel         |
| R11 | P2        | Parallele STT-Downloads schreiben dieselbe temporäre Datei         | M       | mittel         |
| R12 | P2        | Claude-Startfehler lassen Approval-Listener weiterlaufen           | M       | niedrig–mittel |
| R13 | P2        | Link-Abbruch wirkt nicht über den gesamten Austausch               | M       | mittel         |
| R14 | P2        | Alter Link-Task kann einen neuen Code-Slot löschen                 | S       | niedrig        |
| R15 | P2        | Fehlgeschlagene Actions melden Erfolg                              | M       | mittel         |
| R16 | P2        | Chat scrollt den ersten global gefundenen Nachrichtencontainer     | S       | niedrig        |
| R17 | P2        | Modellinventar ist doppelt implementiert und bereits divergent     | M       | mittel         |
| R18 | P2        | Blockierende Dateizugriffe auf Async-Executor-Threads              | M       | niedrig–mittel |
| R19 | P3        | SQLite-UUID-Decoding ist dupliziert und verschluckt teils Fehler   | S       | niedrig–mittel |
| R20 | P3        | Permission-Leseregel ist doppelt implementiert                     | S       | niedrig        |
| R21 | P3        | InstalledModel-Payload wird an drei Stellen manuell projiziert     | S       | niedrig        |
| R22 | P3        | STT-Testkörper steht in einer Produktionsdatei                     | S       | niedrig        |
| R23 | P3        | Locktests ersetzen Synchronisierung durch beliebige Sleeps         | S–M     | niedrig        |
| R24 | P3        | Mehrere große Dateien erfüllen die Ausnahmevoraussetzungen nicht   | M       | mittel         |
| R25 | P2        | Multi-Chat-Tests umgehen die fehlerhaften Integrationsgrenzen      | M       | niedrig        |
| R26 | P2        | Der versprochene Binding-Drift-Check fehlt in CI                   | S       | niedrig        |
| R27 | P3        | Vorhandene Checks sind nicht vollständig an CI angeschlossen       | S       | niedrig        |
| R28 | P3        | Nichtleere beschädigte STT-Dateien werden nicht repariert          | M       | niedrig–mittel |
| R29 | P3        | Aktuelle Merge-Titel verletzen die Conventional-Commit-Regel       | S       | niedrig        |

## Datenintegrität und Autorisierung

### R01 – Snapshot-Fortschritt erst nach vollständig abgeschlossenem Resync veröffentlichen

- **Belege:** `src-tauri/src/sync/inbound.rs:233`, `:254`, `:275`; `src-tauri/src/sync/session.rs:318`; `src-tauri/src/sync/resync_tests.rs:156`.
- **Problem:** Snapshot-Inboxen übernehmen zwar am Ende nicht sofort `page.served`, erhöhen den dauerhaften Fortschritt aber trotzdem aus den angewendeten Gruppen. Die Bereinigung fehlender Zeilen erfolgt erst danach. Enthält der Snapshot die neueste Live-Zelle eines Ursprungs, kann dessen Fortschritt bereits den Senderstand erreichen.
- **Folge:** Bei Abbruch zwischen Seiten-Commit und Pruning sieht der nächste Verbindungsaufbau keinen Rückstand mehr. Lokal verbliebene, beim Sender längst gelöschte Zeilen werden nicht bereinigt und können wieder verbreitet werden. Der vorhandene Test zur Fortschrittssperre enthält keine Änderungen und deckt diesen Fall nicht ab.
- **Lösung:** Einen dauerhaften Resync-Zustand einführen oder sämtliche Snapshot-Fortschritte bis zum Abschluss zurückhalten. Den eigenen Ursprung berücksichtigen: `Replica::progress()` leitet dessen Fortschritt auch aus vorhandenen Zellen ab. Wiederaufnahme muss die Bereinigung unabhängig von einem bereits angeglichenen Vektor erzwingen.
- **Regression:** Nichtleerer Snapshot mit neuester Live-Zelle und einer lokal verbliebenen gelöschten Zeile; Abbruch nach Seiten-Commit, vor Pruning; Neustart. Die alte Zeile muss verschwinden und darf nicht zurücksynchronisiert werden.
- **Bewertung:** Sicherheit hoch, statische Ablaufanalyse; M, Korrekturrisiko hoch. Vertragsabweichung zum Resync-Verhalten in `specs/024-own-device-sync/contracts/sync-protocol.md`; Testempfehlung aus `coding-guideline-rust/rust-testing`.

### R02 – Löschkandidaten unmittelbar in der Schreibtransaktion erneut prüfen

- **Belege:** `src-tauri/src/sync/resync.rs:104`, `:113`, `:122`, `:126`; `src-tauri/src/sync/replica.rs:11`.
- **Problem:** `prune_absent` berechnet Löschkandidaten außerhalb der späteren Schreibtransaktion. Der DELETE prüft anschließend nur den Primärschlüssel. Lokale Schreibvorgänge nehmen ausdrücklich nicht den Exchange-Lock.
- **Folge:** Bearbeitet der Nutzer eine zunächst löschbare Zeile zwischen Scan und DELETE, wird auch diese neuere, noch nicht synchronisierte Änderung gelöscht.
- **Lösung:** Innerhalb der Schreibtransaktion aktuelle Zellen-HLCs erneut gegen `served` prüfen. Alternativ Scan und Entscheidung vollständig in eine atomare Datenbankoperation verlegen. Ein zusätzlicher globaler Lock ist ohne Einbeziehung aller lokalen Writer keine Lösung.
- **Regression:** Per Barrier nach Kandidatenscan eine lokale Änderung schreiben, dann Pruning fortsetzen. Änderung und Zeile müssen erhalten bleiben.
- **Bewertung:** Sicherheit hoch, statisch; M, Risiko mittel. Relevante SHOULD-Regel: konkurrierende Schreib-/Persistenzgrenzen gezielt testen.

### R03 – Empfangsprüfung und Geräteentfernung gemeinsam schützen

- **Belege:** `src-tauri/src/sync/inbound.rs:155`, `:190`, `:264`; `src-tauri/src/sync/removal.rs:69`.
- **Problem:** Der Empfang liest Entfernungsgrenzen und bewertet Änderungen vor Erwerb des Exchange-Locks. Die Entfernung publiziert ihre Grenze unter diesem Lock.
- **Folge:** Empfang liest alte Grenzen → Entfernung committed → Empfang nimmt den Lock und übernimmt bereits vorab geprüfte Änderungen oberhalb der neuen Grenze. Auch über andere Geräte weitergeleitete Änderungen des entfernten Ursprungs sind betroffen.
- **Lösung:** Policy-Lesen, Zulassung und Anwendung gemeinsam schützen oder die Zulassung unter dem Lock erneut prüfen. Zurückgehaltene Gruppen bei späterer Anwendung ebenfalls gegen aktuelle Grenzen prüfen; Lock-Reihenfolge auf Deadlocks kontrollieren.
- **Regression:** Empfang nach Policy-Lesen anhalten, Gerät entfernen, Empfang fortsetzen. Änderungen oberhalb des Limits müssen abgewiesen werden, solche am Limit bleiben zulässig.
- **Bewertung:** Sicherheit hoch, statisch; M, Risiko mittel. Abweichung von FR-027/R5 sowie der dokumentierten Ausschlussgarantie in `removal.rs`; keine Behauptung einer extern getesteten Ausnutzung.

### R04 – Modellveröffentlichung braucht das bereits spezifizierte Recovery-Journal

- **Belege:** `src-tauri/src/models/commands.rs:751`, `:760`, `:831`, `:841`; `src-tauri/src/models/import.rs:88`; `specs/005-huggingface-model-discovery/contracts/tauri-commands.md:152`.
- **Problem:** Neue Bytes werden vor dem Datenbank-Commit veröffentlicht. Der alte Pfad wird zuvor in eine zufällig benannte Backup-Datei umbenannt. Die Rückrollinformationen existieren nur im laufenden Aufruf. Startup-Cleanup löscht ein Backup bereits, wenn irgendeine finale GGUF im Slug liegt.
- **Folge:** Prozessende nach Publish und vor DB-Commit hinterlässt neue Bytes mit alter SHA/Revision und kann beim Neustart die letzte passende alte Kopie löschen. Prozessende zwischen den beiden Renames hinterlässt keine kanonische GGUF; das aufbewahrte Backup wird nicht automatisch wiederhergestellt.
- **Lösung:** Den normativen Journal-Vertrag unter dem vorhandenen Publication-Lock umsetzen: alte/neue SHA- und Source-Metadaten sowie Operationsphase dauerhaft speichern, Recovery eindeutig auf ein vollständiges altes oder neues Paar führen, erst dann Journal/Backup entfernen. Gleichzeitige Nutzung durch andere Prozesse berücksichtigen.
- **Regression:** Prozessunterbrechung an jeder Grenze zwischen Journal, Backup, Metadaten-Commit, Publish und Cleanup. Nach Neustart muss immer ein konsistentes Datei-/SHA-/Revision-Paar vorliegen.
- **Bewertung:** Sicherheit hoch, statisch und gegen Spec geprüft; L, Risiko hoch. Keine akzeptierte Architekturabweichung: die Spec verlangt ausdrücklich dauerhafte Recovery.

## Chat, Actions und Lebenszyklen

### R05 – Aktionen an die auslösende Chat-Instanz binden

- **Belege:** `src/lib/actions/chatActions.ts:31`, `src/stores/wmNavigation.ts:132`, `src/lib/actions/handlers.ts:53`, `src/composables/useChatTab.ts:161`, `:186`.
- **Problem:** Chat-Aktionen sind tabgebunden, haben aber `target: 'none'`. Der Runner sucht über alle Chat-Tabs und nimmt den ersten registrierten Handler. Die UI übermittelt keine auslösende Tab-ID.
- **Folge:** „Senden“ im zweiten Chat kann Eingabe und Unterhaltung des ersten Chats verwenden. Entwürfe werden im falschen Tab verändert; weitere tabgebundene Aktionen teilen denselben Routingfehler.
- **Lösung:** Die auslösende Tab-ID durch Aufrufkontext, Runner und Handlerauflösung führen. Für automatische Aufrufer eine eindeutige Zielregel festlegen. Vorhandene Registry erweitern, keine zweite Dispatch-Schicht hinzufügen.
- **Regression:** Zwei Chats mit verschiedenen Threads/Entwürfen. Senden, Öffnen, Retry und Freigabe aus B dürfen nur B bedienen. Den tatsächlichen UI→Runner→Handler-Pfad verwenden.
- **Nachweis:** Ausgeführt mit realem `createWmNavigation` und realer Registry: aktiver Tab `second`, ausgeführter Handler `first`, Action-Ergebnis erfolgreich.
- **Bewertung:** Sicherheit hoch, zur Laufzeit bestätigt; M, Risiko mittel. Fehlende Abstraktion für Instanzzugehörigkeit, keine Notwendigkeit eines neuen allgemeinen Frameworks.

### R06 – Nach Thread-Ladung prüfen, ob die Auswahl noch aktuell ist

- **Belege:** `src/composables/useThreadSidebar.ts:290`, `:293`, `:301`.
- **Problem:** `selectThread` setzt den aktiven Thread, wartet auf Nachrichten und übernimmt danach Freigaben ohne erneute Prüfung der Auswahl.
- **Folge:** Auswahl A→B bei langsamer Antwort für A zeigt Werkzeugfreigaben aus A unter dem sichtbaren Gespräch B. Dieser Fehler benötigt keine mehreren Tabs.
- **Lösung:** Auswahlgeneration oder aktive Thread-ID nach dem Await kontrollieren. Cachefüllung für A darf erfolgen; sichtbare Freigaben und Scrollzustand dürfen nur zur aktuellen Auswahl wechseln.
- **Regression:** Kontrollierte Promises für A→B und A→B→A, einschließlich einer während des Ladens eintreffenden Freigabe.
- **Nachweis:** Zurückgehaltene Antwort für A nach abgeschlossenem Wechsel zu B aufgelöst: aktiver Thread blieb B, angezeigte Freigabe stammte aus A.
- **Bewertung:** Sicherheit hoch, zur Laufzeit bestätigt; S, Risiko niedrig. SHOULD: asynchrone Reihenfolge und sichtbare Seiteneffekte explizit absichern.

### R07 – Modell-Listener von der Vault-Sitzung besitzen lassen

- **Belege:** `src/components/apps/ChatApp.vue:361`, `:379`; `src/stores/models.ts:388`, `:427`.
- **Problem:** Jede Chat-Komponente startet Listener im gemeinsamen Pinia-Store. Jede beendet beim Unmount sämtliche Listener dieses Stores. Start ist nicht idempotent.
- **Folge:** Ereignisse werden bei mehreren Chats mehrfach verarbeitet; das Schließen eines Chats entfernt danach auch die Abonnements der verbleibenden Chats. Laufende Modellladungen können visuell hängen bleiben.
- **Lösung:** Eine idempotente Subscription für die Vault-Sitzung aus dem Workspace starten, analog zum vorhandenen Vault-Data-Lebenszyklus. Einzelne Komponenten dürfen gemeinsame Listener nicht beenden.
- **Regression:** Zwei Verbraucher starten, einen schließen, danach Ladefortschritt und Fehler senden. Der zweite empfängt jedes Ereignis genau einmal. Zusätzlich Unmount während ausstehender Registrierung prüfen.
- **Nachweis:** Zweifacher Start der realen Store-Instanz registrierte sechs Modell-Subscriptions; ein Stop entfernte alle sechs.
- **Bewertung:** Sicherheit hoch, zur Laufzeit bestätigt; S–M, Risiko mittel. Gemeinsamer Zustand und komponentenlokale Lebensdauer sind falsch gekoppelt.

### R08 – Fremde Ereignisse begrenzen und terminale Freigaben bereinigen

- **Belege:** `src/composables/useChatTranscript.ts:132`, `:335`, `:352`; `src/components/apps/ChatApp.vue:353`.
- **Problem:** Jeder Chat puffert Tokens unbekannter Nachrichten und Freigaben anderer Threads. Deren terminales Ereignis wird ignoriert, wenn der Thread nicht der lokal streamende Thread ist. Cleanup findet erst im übersprungenen Pfad statt.
- **Folge:** Eine in A erledigte Freigabe taucht später in B erneut auf. Nicht beteiligte Tabs signalisieren Aufmerksamkeit. Puffer fremder Turns wachsen mit der Sitzungsdauer; Thread-Löschung kann wegen veralteter Pending-Zustände unnötig warten.
- **Lösung:** Turn-/Freigabedaten sitzungsweit verwalten und threadbezogen projizieren oder alle Ereignisse konsequent zuordnen. Terminalereignisse müssen Freigaben/Puffer ihres Threads auch bei Nicht-Eigentümern bereinigen. Vorab-Tokenpuffer auf die eigene Send-Aufnahme begrenzen.
- **Regression:** Freigabe in A erledigen, B später auf denselben Thread wechseln: keine alte Freigabe. Viele fremde abgeschlossene Turns dürfen keinen stetigen Pufferzuwachs erzeugen.
- **Nachweis:** Reales Chat-Harness: fremde Freigabe → fremdes Turn-Ende → Thread auswählen; eine veraltete Freigabe blieb sichtbar.
- **Bewertung:** Sicherheit hoch; Freigabefehler zur Laufzeit, Pufferproblem statisch bestätigt. M–L, Risiko mittel; vorhandene Pre-Send-Race-Behandlung muss erhalten bleiben.

### R09 – Den gesamten Send-Start nach Staging gemeinsam absichern

- **Belege:** `src-tauri/src/chat/commands.rs:352`, `:407`, `:552`, `:596`; `src-tauri/src/chat/send_admission.rs:73`.
- **Problem:** Nach Persistierung der User-Nachricht verlassen mehrere Fehler die Funktion mit `?`. Rollback existiert nur für Streamstart- und Spawnfehler. Besonders relevant ist der Preference-Commit nach erfolgreichem Adapterstart.
- **Folge:** Send meldet Fehler, der Stream wird verworfen, Nachricht und Idempotency-Key bleiben jedoch bestehen. Derselbe Retry-Key liefert anschließend einen Duplicate-Erfolg mit IDs, ohne einen Turn zu starten.
- **Lösung:** Den Abschnitt zwischen Staging und Übergabe an `run_turn` mit einem gemeinsamen Fehlerpfad versehen. Alternativ den angenommenen Auftrag explizit terminal fehlschlagen lassen; keinen unvollständigen impliziten Zustand hinterlassen.
- **Regression:** Fehler bei History-Lesen, Attachment-Worker und Preference-Commit injizieren; Retry muss einen gültigen Auftrag starten oder einen expliziten terminalen Zustand liefern.
- **Bewertung:** Sicherheit hoch, statisch; M, Risiko mittel. Idempotenz und Aufräumen als zusammenhängende Verantwortung behandeln.

### R10 – Call/Result-Paare atomar persistieren

- **Belege:** `src-tauri/src/chat/turn/tool_round.rs:350`, `:355`, `:378`; `src-tauri/src/chat/turn/persist.rs:25`. Vorbild: `src-tauri/src/adapters/cli_delegate/approval_bridge.rs:227`.
- **Problem:** Bereits ausgeführte Tools werden als Call und Result in zwei separaten Transaktionen gespeichert. Nach dem ersten Commit wird außerdem bereits das Call-Ereignis emittiert.
- **Folge:** Scheitert der zweite Commit, bleibt ein unbeantworteter Tool-Call im Verlauf. Das tatsächliche Ergebnis fehlt; spätere Provider-Anfragen können wegen unvollständiger Tool-History scheitern. Die Provider-Ablehnung wurde nicht live getestet.
- **Lösung:** Mindestens ein Call/Result-Paar in einer `VaultDb::write`-Transaktion speichern. Events erst nach Commit emittieren. Das bestehende atomare Delegate-Auditpaar liefert die lokale Konvention. Externe Tool-Seiteneffekte lassen sich dadurch nicht rückgängig machen; der Fehlerzustand muss weiterhin ehrlich sichtbar sein.
- **Regression:** Zweites Insert fehlschlagen lassen: kein halbes Paar und keine vorzeitig als persistiert gemeldeten Events. Parent-/Timestamp-Kette kontrollieren.
- **Bewertung:** Sicherheit hoch für den Teil-Commit; M, Risiko mittel.

### R11 – STT-Installation prozessübergreifend serialisieren

- **Belege:** `src-tauri/src/stt/local.rs:84`, `:336`; `src-tauri/src/stt/commands.rs:140`; `src-tauri/src/models/download.rs:76`.
- **Problem:** Completeness-Check und STT-Download laufen ohne Publication-Lock. Der Downloader verwendet denselben `.part`-Pfad und öffnet ihn truncierend. Laden und explizite Installation erreichen beide diesen Pfad.
- **Folge:** Parallele Installationen, insbesondere aus zwei App-Prozessen, können dieselbe temporäre Datei truncieren oder umbenennen. Ein offener Writer kann nach dem Publish des anderen noch in dieselbe Datei schreiben.
- **Lösung:** Vor Completeness-Check und Transfer den bestehenden prozessübergreifenden Publication-Lock für den STT-Slug verwenden bzw. passend erweitern. Cancellation wartender Aufrufe sauber erhalten.
- **Regression:** Zwei kontrollierte Downloads, zusätzlich zwei Prozesse, gleicher Slug. Beide sehen anschließend dieselben vollständigen Bytes; Abbruch eines Wartenden beeinflusst den aktiven Transfer nicht.
- **Bewertung:** Sicherheit hoch, statisch; M, Risiko mittel. Fehlende Nutzung einer bereits vorhandenen Domänenabstraktion.

### R12 – Approval-Listener bei frühem Claude-Startfehler aufräumen

- **Belege:** `src-tauri/src/adapters/cli_delegate/claude.rs:374`, `:382`, `:393`, `:416`; `src-tauri/src/adapters/cli_delegate/approval_bridge.rs:283`; `src-tauri/src/adapters/cli_delegate/mod.rs:124`.
- **Problem:** Der Listener startet vor Config-Schreiben und CLI-Spawn. Bei nachfolgendem Fehler wird sein `JoinHandle` fallengelassen, wodurch der Task detached weiterläuft. Sein Context hält einen `VaultDb`.
- **Folge:** Etwa ein fehlendes Binary hinterlässt einen Accept-Task und Vault-Referenzen. Wiederholte Startversuche sammeln Ressourcen; sauberer Vault-Drain wird erschwert und kann seine Eskalationsgrenze erreichen.
- **Lösung:** Einen Task-Owner mit Abbruch beim Drop verwenden und Eigentümerschaft erst bei erfolgreichem Start übertragen. Auch Verbindungstasks diesem Owner zuordnen.
- **Regression:** Fehlendes Binary und Config-Schreibfehler nach Listenerstart auslösen. Anschließend müssen Tasks/Listener beendet und Vault-Referenzen freigegeben sein.
- **Bewertung:** Sicherheit hoch, statisch; M, Risiko niedrig–mittel. Cleanup-Empfehlung; die Python-spezifische Cleanup-MUST-Regel wird hier ausdrücklich nicht auf Rust übertragen.

### R13 – Link-Cancellation über den ganzen Austausch durchsetzen

- **Belege:** `src-tauri/src/sync/link/host_task.rs:265`, `:286`, `:301`; `src-tauri/src/sync/link/host.rs:97`, `:153`, `:170`, `:175`.
- **Problem:** Cancellation wirkt beim Rendezvous und beim Entscheidungs-Future. Das gesamte `host::run` ist jedoch nicht überwacht. Beim Proof-Warten wird die Entscheidung noch nicht gepollt; nach Zustimmung ist sie bereits verbraucht.
- **Folge:** Abbruch oder Ersetzen des Codes kann den sichtbaren Slot entfernen, während der alte Transfer weiterläuft und das Gerät später aufnimmt. Ein Peer kann vor der Entscheidung beim Proof hängen bleiben, ohne dass der Decision-Timeout greift.
- **Lösung:** Den gesamten Austausch mit Cancellation und phasengerechten Timeouts beaufsichtigen; Streams/Verbindung schließen. Die irreversible Grenze bereits dauerhaft angenommener Transfers explizit definieren und in Status/Abbruchsemantik abbilden.
- **Regression:** Peer beim Proof und während Snapshot anhalten, dann abbrechen oder neuen Code erzeugen. Alter Ablauf muss enden. Separater Test für bereits dauerhaft abgeschlossenen Transfer.
- **Bewertung:** Sicherheit hoch, statisch; M, Risiko mittel. Pending-Link-Recovery und Aufnahmegarantien erhalten.

### R14 – Link-ID-Prüfung und Slotwechsel unter einem Lock ausführen

- **Belege:** `src-tauri/src/sync/link/host_task.rs:225`, `:231`, `:131`.
- **Problem:** `finish` prüft seine ID unter einem temporären Lock und nimmt zum Setzen von `Idle` einen zweiten Lock.
- **Folge:** Ein inzwischen neu eingesetzter Code wird vom alten Task überschrieben. Sein Status und Abbruchhandle gehen verloren, während seine Aufgabe noch läuft.
- **Lösung:** Prüfung und Wechsel mit demselben MutexGuard; Event erst nach Freigabe senden.
- **Regression:** Kontrollierte Überlappung von `finish(old)` und `replace(new)`; der neue Slot muss erhalten bleiben.
- **Bewertung:** Sicherheit hoch, statisch; S, Risiko niedrig.

### R15 – Action-Ergebnisse müssen tatsächliche Fehler transportieren

- **Belege:** `src/stores/models.ts:205`, `:235`; `src/stores/chatActionHandlers.ts:23`; `src/composables/useChatTab.ts:161`.
- **Problem:** Store-/UI-Operationen fangen Fehler ab und erfüllen ihr Promise. Action-Handler geben danach unabhängig davon `{done: true}` zurück.
- **Folge:** Automatische Aufrufer erhalten einen erfolgreichen Modellwechsel oder Download, obwohl nur ein Fehlerbanner gesetzt wurde und das bisherige Modell aktiv bleibt. Entsprechende Send-Pfade besitzen denselben Vertragsbruch.
- **Lösung:** Domänenoperationen geben einen strukturierten Ausgang zurück oder werfen; die UI zeigt ihn an, der Runner transportiert ihn. No-ops wie „beschäftigt“ ebenfalls explizit modellieren. Fehleranzeige und Fehlerpropagation dürfen nebeneinander bestehen.
- **Regression:** IPC-Ladefehler → `runAction` liefert `ok: false`, bisheriges Modell bleibt erhalten. Download-/Send-/Abort-Fehler ergänzen.
- **Nachweis:** Reales Store-/Handler-Paar meldete `{done: true}`, während `lastError` den injizierten Modellladefehler enthielt.
- **Bewertung:** Sicherheit hoch, zur Laufzeit bestätigt; M, Risiko mittel. SHOULD_NOT aus `coding-guideline-javascript-typescript/javascript-typescript`: Fehler nicht durch Catch verbergen.

### R16 – Scrollen auf die eigene Chat-Komponente begrenzen

- **Belege:** `src/components/apps/ChatApp.vue:219`; `src/lib/wm/apps.ts:38`.
- **Problem:** `document.querySelector('[data-messages-scroll]')` findet den ersten Container im gesamten Dokument, obwohl Chat mehrere Instanzen erlaubt.
- **Folge:** Tokens/Threadwechsel in B scrollen A; B bleibt an der falschen Position und A springt beim Lesen.
- **Lösung:** Instanzlokalen Template-Ref auf den eigenen Nachrichtencontainer verwenden.
- **Regression:** Zwei gerenderte Panels mit verschiedenen Scrollpositionen. Ereignis für B verändert ausschließlich B.
- **Bewertung:** Sicherheit hoch, statisch; S, Risiko niedrig.

## Duplikationen, Abstraktionen und Coding-Regeln

### R17 – Ein gemeinsames Modellinventar statt divergierender Kopien

- **Belege:** `src/components/settings/DefaultModelSetting.vue:41`, `:90`, `:104`, `:144`; `src/composables/useModelInventory.ts:90`, `:175`.
- **Problem:** Default-Einstellung und Chat bauen eigene Modellgruppen/Caches. Die Einstellung berücksichtigt nur `api_key`, der Chat auch `cli_delegate`. Bei Providerfehlern unterscheiden sich die Erhaltungsregeln. Die Einstellung beobachtet `preferences` und `providers`, aber nicht `models`.
- **Folge:** Modelländerungen werden uneinheitlich sichtbar; dieselbe Inventarregel muss mehrfach gepflegt werden. Verbundene Delegate-Modelle fehlen in der Default-Auswahl. Ob ihre Auswahl ausdrücklich gewollt ist, muss vor Änderung mit dem Produktvertrag abgeglichen werden; die Cache-/Aktualisierungsdivergenz besteht unabhängig davon.
- **Lösung:** Das vorhandene Inventar für tatsächlich installierte/konfigurierte Modelle wiederverwenden. UI-spezifische Optionen wie „Keins“ und Platzhalter separat projizieren. Initialisierung darf nicht davon abhängen, dass zuvor ein Chat geöffnet wurde.
- **Regression:** Lokale/API-/Delegate-Modelle, temporärer Providerfehler und Installation/Entfernung bei geöffneter Einstellung; beabsichtigte Auswahlunterschiede explizit prüfen.
- **Bewertung:** Sicherheit hoch für Duplikation/Invalidierung, mittel für gewünschte Delegate-Auswahl; M, Risiko mittel. SHOULD `general-coding`: gemeinsame fachliche Regeln konsolidieren.

### R18 – Blockierendes I/O aus Async-Executor-Threads verschieben

- **Belege:** `src-tauri/src/adapters/cli_delegate/claude.rs:318`, `:358`; `src-tauri/src/adapters/cli_delegate/codex.rs:330`; `src-tauri/src/models/commands.rs:573`.
- **Problem:** Async-Funktionen schreiben Anhänge/Auth-Konfiguration mit `std::fs` bzw. scannen Modellverzeichnisse synchron. Der spätere Blocking-Scan in `list_installed_models` schützt den bereits vorher ausgeführten Root-Scan nicht.
- **Folge:** Große Anhänge oder langsame Datenträger blockieren Executor-Threads und verzögern Requests/Abbruch. Das Ausmaß ist nicht benchmarked.
- **Lösung:** Zusammenhängende Sandbox-Vorbereitung und Dateiscans vollständig in das bestehende `spawn_blocking` verschieben oder Tokio-Datei-APIs nutzen. Cancellation und Eigentümerschaft des Tasks erhalten.
- **Regression:** Instrumentierter langsamer I/O-Pfad; parallel muss ein unabhängiger Request bzw. Abbruch fortschreiten können.
- **Bewertung:** Sicherheit hoch; M, Risiko niedrig–mittel. **MUST_NOT-Verstoß** gegen `com.github.haexmas.atoms.coding-guideline-rust/rust`: „do not perform blocking file, process, or CPU-heavy work directly on an async executor thread“.

### R19 – SQLite-UUID-Decoding und Fehlersemantik vereinheitlichen

- **Belege:** `src-tauri/src/storage/chat_messages.rs:308`, `src-tauri/src/storage/chat_threads.rs:144`, `src-tauri/src/storage/providers.rs:264`, `src-tauri/src/storage/models.rs:479`; abweichendes Verschlucken in `storage/providers.rs:172`, `:215`, `:236`.
- **Problem:** Derselbe Decoder wird kopiert bzw. inline nachgebaut. Andere Lookups machen aus ungültigen UUIDs mit `.ok()` ein fehlendes Ergebnis.
- **Folge:** Fehlerhafte gespeicherte Identitäten erscheinen je nach API als Konvertierungsfehler oder als „Provider fehlt“. Das erschwert Diagnose und kann unnötige Neuanlagepfade aktivieren.
- **Lösung:** Einen kleinen Storage-Decoder mit Spaltenindex und ursprünglicher Fehlerursache wiederverwenden. Fehlende Zeile von ungültigem Inhalt unterscheiden.
- **Regression:** Keine Zeile → `None`; vorhandene Zeile mit ungültiger UUID → strukturierter Konvertierungsfehler, konsistent bei Einzel-/Listenabfrage.
- **Bewertung:** Sicherheit hoch; S, Risiko niedrig–mittel. SHOULD zur Konsolidierung, zusätzlich Abweichung von der Rust-Regel zum Erhalt ursprünglicher Fehler an den `.ok()`-Stellen.

### R20 – Permission-Leseregel gemeinsam halten

- **Belege:** `src-tauri/src/chat/turn/tool_round.rs:72`; `src-tauri/src/adapters/cli_delegate/approval_bridge.rs:251`.
- **Problem:** Preference-Key, Vault-Scope, Parsing und Fehlerfallback für `chat.permission_mode` sind doppelt implementiert.
- **Folge:** Eine künftige Änderung an einer sicherheitsrelevanten Regel kann Built-in- und Delegate-Tools auseinanderlaufen lassen. Aktuell ist keine unterschiedliche Entscheidung dieser beiden Kopien nachgewiesen.
- **Lösung:** Bestehendes Permission-Domänenmodul um die gemeinsame Leseregel erweitern; keine globale Utils-Sammlung.
- **Regression:** Fehlender/ungültiger Wert und Lesefehler ergeben auf beiden Wegen dieselbe festgelegte Entscheidung.
- **Bewertung:** Sicherheit hoch; S, Risiko niedrig. **SHOULD-Empfehlung**, kein behaupteter rückwirkender Verstoß gegen Graphify-Authoring.

### R21 – InstalledModel-Payload einmal projizieren

- **Belege:** `src-tauri/src/models/commands.rs:401`, `:606`, `:783`.
- **Problem:** Listing, Idempotenz-Rückgabe und Neuregistrierung kopieren dieselbe größere Feldprojektion in `InstalledModelPayload`.
- **Folge:** Neue Capability-/Source-/Integritätsfelder müssen dreifach angepasst werden. Die drei Pfade können bei späteren Änderungen unterschiedliche Verträge liefern.
- **Lösung:** Kleine domäneneigene Conversion aus `ModelRow` plus Dateipfad/-größe. Downloadorchestrierung nicht in diesen Mapper ziehen.
- **Regression:** Alle drei Aufrufpfade müssen für dieselben Ausgangsdaten denselben Payload liefern.
- **Bewertung:** Sicherheit hoch; S, Risiko niedrig. **SHOULD-Empfehlung** zur tatsächlichen Duplikation, kein vorhandener Funktionsfehler behauptet.

### R22 – Inline-STT-Test in separate Testdatei verschieben

- **Belege:** `src-tauri/src/stt/local.rs:427`; vorhandene Datei `src-tauri/src/stt/local_tests.rs`.
- **Problem:** Das Produktionsmodul enthält einen vollständigen Inline-Testkörper für die Mel-Filterbank.
- **Lösung:** Test in passende separate Datei verschieben; im Produktionsmodul nur Testmodul-Deklaration belassen. Den vorhandenen Test unverändert unter `llm-cpu` ausführen.
- **Bewertung:** Sicherheit hoch; S, Risiko niedrig. **MUST-Verstoß** gegen `coding-guideline-rust/rust-testing` und `general-coding/general-coding`: Testkörper müssen außerhalb von Produktionsdateien liegen. Reine `#[cfg(test)] mod ...;`-Deklarationen sind ausdrücklich erlaubt.

### R23 – Locktests deterministisch synchronisieren und Umgebung wiederherstellen

- **Belege:** `src-tauri/src/models/paths_tests.rs:45`, `:80`, `:29`, `:60`.
- **Problem:** 50-ms-Sleeps sollen sicherstellen, dass der zweite Task bereits wartet. Ein nicht gestarteter Task erfüllt `!is_finished()` jedoch ebenfalls. Globale Umgebungswerte werden am Ende gelöscht statt auf den vorherigen Wert zurückgesetzt; bei Panic bleibt die Änderung aktiv.
- **Folge:** Der Test kann grün werden, ohne tatsächliche Contention zu beweisen. Fehlgeschlagene Tests können andere Tests beeinflussen.
- **Lösung:** Barrier/Notify oder kontrolliertes Future-Polling bis zum echten Wartepunkt. Pfade direkt injizieren; falls Environment nötig bleibt, vorherigen Wert per Guard wiederherstellen.
- **Regression:** Konkurrenz und Cancellation ohne beliebige Wartezeit beweisen; wiederholte parallele Ausführung darf keine Umgebungsreste hinterlassen.
- **Bewertung:** Sicherheit hoch; S–M, Risiko niedrig. **MUST_NOT-Verstoß** gegen `coding-guideline-rust/rust-testing`: „Do not use arbitrary sleeps to wait for a task in async tests.“ Sleeps unter bewusst pausierter Testzeit bzw. zum Testen eines Timers wurden nicht pauschal beanstandet.

### R24 – Große Dateien brauchen eine vollständige Ausnahme oder einen fachlichen Split

Die aktive Regel verlangt bei klar über 500 Zeilen entweder einen Split oder dokumentierte Begründung **und** konkreten späteren Splitplan. Sie verlangt ausdrücklich keine künstliche Zerstückelung.

| Datei                                                   | Zeilen | Bewertung am Review-Stand                                                                                                       |
| ------------------------------------------------------- | -----: | ------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/src/adapters/cli_delegate/codex.rs`          |    580 | Keine entsprechende Ausnahme/Splitplanung in Datei oder geprüften Review-/Plan-Dokumenten gefunden                              |
| `src-tauri/src/identity/migrations.rs`                  |    571 | Historische Migrationssammlung ist kohärent, aber formale Ausnahme mit Splitplan fehlt                                          |
| `src-tauri/tests/vault_lifecycle_close.rs`              |    548 | Keine entsprechende Ausnahme mit Splitplan gefunden                                                                             |
| `src-tauri/src/adapters/cli_delegate/autonomy_tests.rs` |    522 | Keine entsprechende Ausnahme mit Splitplan gefunden                                                                             |
| `src-tauri/src/adapters/cli_delegate/claude.rs:8`       |    650 | Begründung vorhanden, jedoch nur allgemeines „revisit“, kein konkreter Splitplan; bezeichnet die Grenze außerdem als rein weich |

- **Lösung:** Nach Verantwortlichkeiten aufteilen, z. B. Protokollübersetzung gegenüber Prozesssteuerung oder unabhängige Testgruppen. Für die kohärente Migrationshistorie eine begründete Ausnahme samt konkreter Modul-/Dateiaufteilung für später formulieren; bereits ausgelieferte SQL-Inhalte/Checksummen dabei unverändert lassen.
- **Regression:** Vorhandene Tests und Migrationsprüfungen nach rein mechanischem Split unverändert bestehen lassen. Kein neues Testframework.
- **Bewertung:** Sicherheit hoch für Zeilenzahlen, mittel für das Nichtvorhandensein einer anderweitigen Ausnahme; M, Risiko mittel. **MUST-Anforderung derzeit nicht ausreichend belegt erfüllt** (`general-coding/general-coding`).
- **Bewusst akzeptiert:** `models/huggingface.rs` (1248), `models/commands.rs` (920), `chat/commands.rs` (739), `chat/model_loading.rs` (706), `providers/mod.rs` (523), `useChat.ts` (590), `chat_tool_loop_permissions.rs` (1115), `check-chat-state.ts` (1203) haben dokumentierte Gründe und konkrete Splitpläne. Dateien mit 501–503 Zeilen sind nicht als eigenständiger Verstoß gewertet.

## Tests, CI und verbleibende Funktionslücken

### R25 – Multi-Chat-Integration tatsächlich testen

- **Belege:** `scripts/e2e/scenarios/chat-multi-instance.test.ts:16`; `scripts/lib/chat-state-harness.ts:369`, `:440`.
- **Problem:** Das E2E-Szenario prüft primär Tabs/Fenster und verschiedene IDs. Der Chat-Harness erstellt pro Test eine neue Pinia-Instanz und ersetzt `runAction` durch bedingungslosen Erfolg.
- **Folge:** R05/R07 bleiben unsichtbar: Tests umgehen sowohl echtes Dispatch als auch geteilten Store-Lebenszyklus. Das ist eine konkrete Integrationslücke, keine pauschale Kritik an den zahlreichen vorhandenen Tests.
- **Lösung:** Einen gezielten Test mit gemeinsamem Pinia, realem Runner und zwei Chat-Instanzen ergänzen; nur Tauri-IPC/Ereignisquelle faken. E2E echte Nachrichten in mehreren Tabs senden und einen Tab während Modellladung schließen.
- **Regression:** Die in R05/R07 beschriebenen Reproduktionen müssen vor Korrektur rot werden. Bestehende Node-/E2E-Infrastruktur nutzen.
- **Bewertung:** Sicherheit hoch; M, Risiko niedrig. **SHOULD/SHOULD_NOT-Abweichung** aus `javascript-typescript-testing`: relevante Integrationsgrenzen testen, die zu prüfende Einheit nicht durch triviale Fakes ersetzen.

### R26 – Generated-Binding-Drift in CI erkennen

- **Belege:** `.gitignore:39`, `eslint.config.mjs:7`, `specs/001-frontend-onboarding/contracts/types.md:67` behaupten bzw. verlangen Drift-Prüfung. `.github/workflows/ci.yml:196` führt Rust-Tests aus, enthält aber keinen Vergleich frisch exportierter Bindings mit Git. `package.json:40` definiert den Exportbefehl.
- **Problem:** Rust-Tests können Bindings regenerieren; ohne anschließenden Diff-Check bleibt Drift grün. Frontend- und Rust-Prüfungen laufen zudem in separaten Jobs/Checkouts.
- **Folge:** Frontend kompiliert gegen veraltete eingecheckte Typen, während Rust neue Typen exportiert. Beide Jobs können unabhängig erfolgreich sein.
- **Lösung:** In einem CI-Job den vorhandenen Export inklusive Normalisierung ausführen und `git diff --exit-code -- src/types/bindings` prüfen; neue ungetrackte Bindings ebenfalls erkennen. Frontend anschließend gegen diese Typen prüfen oder die Fehlergrenze ausdrücklich über den Diff-Check garantieren.
- **Regression:** Rust-Feld ändern, Bindings unverändert lassen: CI muss scheitern. Frisch regenerierte und eingecheckte Bindings müssen bestehen.
- **Bewertung:** Sicherheit hoch; S, Risiko niedrig. Konkrete Spec-/Dokumentationsabweichung, keine Behauptung bereits beobachteter fehlerhafter Bindings.

### R27 – Vorhandene Tests vollständig an CI anschließen

- **Belege:** `package.json:17` bietet `check:chat-model-search`; `.github/workflows/ci.yml:37` ff. ruft ihn nicht auf. `scripts/ci/test_check_docs.py` existiert, während CI nur `scripts/ci/check-docs.py` ausführt.
- **Folge:** Suchlogik und der Dokumentationschecker selbst können regressieren, ohne dass die vorgesehenen Tests den Merge blockieren.
- **Lösung:** Beide vorhandenen Prüfungen in passende CI-Jobs aufnehmen; die Testliste an einer leicht überprüfbaren Stelle pflegen.
- **Regression:** Gezielt verletzte Suchinvariante bzw. fehlerhafte Checker-Regel muss den jeweiligen CI-Schritt fehlschlagen lassen.
- **Bewertung:** Sicherheit hoch; S, Risiko niedrig. SHOULD: vorhandene Tests tatsächlich ausführen; keine neue Suite erforderlich.

### R28 – Beschädigte nichtleere STT-Dateien reparierbar machen

- **Belege:** `src-tauri/src/stt/local.rs:301`, `:336`; `src-tauri/src/stt/commands.rs:122`.
- **Problem:** „Vollständig“ bedeutet lediglich reguläre Datei mit Größe > 0. `ensure_model_files` überspringt solche Dateien auch dann, wenn ihr Inhalt ungültig ist.
- **Folge:** Eine beschädigte, nichtleere `config.json`/Tokenizerdatei verhindert Laden; erneutes „Download/Repair“ ändert sie nicht. Für Gewichte gilt dieselbe fehlende Integritätsentscheidung.
- **Lösung:** Expliziten Repair-Pfad oder geeignete Format-/Integritätsprüfung ergänzen. Kleine JSON-Dateien validieren; große Gewichte nicht bei jedem UI-Refresh komplett hashen. Unter dem Lock aus R11 atomar ersetzen.
- **Regression:** Nichtleere ungültige Config als Fixture; Installation/Reparatur muss gültige Daten wiederherstellen oder einen ausdrücklichen Reparaturbedarf melden.
- **Bewertung:** Sicherheit hoch, statisch; M, Risiko niedrig–mittel. Teilweise bekannter, im Code kommentierter Prüfungsumfang; konkrete Lücke zwischen Completeness-Heuristik und Reparaturverhalten.

### R29 – Conventional-Commit-Regel auch bei Merge-Titeln einhalten

- **Belege:** `.spaex/constitution.md:64`; aktuelle Merge-Commits `f2ac1be`, `f1081c2`, `8573557`, `cc02f71`, `b78d343` vom 2026-10-01 verwenden den automatisch erzeugten Titel `Merge pull request ...`. Der entsprechende Regelbaustein ist spätestens seit Commit `055a411` vom 2026-09-19 vorhanden.
- **Problem:** Die aktive Regel verbietet ausdrücklich die Ausnahme für automatisch erzeugte Merge-Titel. Dies sind keine vor der Einführung liegenden Altcommits.
- **Folge:** Changelog-/Versionsauswertung kann den fachlichen Typ eines Merge-Commits nicht aus dem vorgeschriebenen Format ableiten; die verbindliche Prozessregel wird verletzt.
- **Lösung:** Zukünftige Merge-Commit-Titel bewusst als Conventional Commit setzen oder den erlaubten Rebase-Merge verwenden. Validierung auf Merge-Commits ausdehnen. Keine rückwirkende Umschreibung veröffentlichter History für diesen Review-Befund.
- **Regression:** Einen zulässigen Merge-Titel und einen GitHub-Standardtitel gegen den gewählten Validator prüfen; letzterer muss abgewiesen werden.
- **Bewertung:** Sicherheit hoch; S, Risiko niedrig für künftige Durchsetzung. **MUST-Verstoß** gegen `spaex-constitution/conventional-commit-messages` und `spaex-constitution/merge-strategy-no-squash`.

## Zusammenfassung der Atoms-/Coding-Regelprüfung

Grundlage sind `.spaex/manifest.json` (Atoms-Revision `764e209c32aaa9c329045b986e7ed68c79316205`), die zusammengesetzte `.spaex/constitution.md` und unabhängig davon `.specify/memory/constitution.md`. Root-`AGENTS.md`/`CLAUDE.md` liegen im geprüften Checkout nicht vor. Die übergebenen Repository-Instruktionen wurden berücksichtigt. Keine direkten Widersprüche zwischen diesen Vorgaben festgestellt, die für dieses reine Review eine Richtungsentscheidung erfordern.

| Regelquelle                                                                | Verbindlichkeit   | Ergebnis                                                                              |
| -------------------------------------------------------------------------- | ----------------- | ------------------------------------------------------------------------------------- |
| `coding-guideline-rust/rust` – Async nicht blockieren                      | MUST_NOT          | Verletzt: R18                                                                         |
| `general-coding/general-coding` + `rust-testing` – Tests separat           | MUST              | Verletzt: R22                                                                         |
| `rust-testing` – keine beliebigen Sleeps zur Task-Synchronisierung         | MUST_NOT          | Verletzt: R23                                                                         |
| `general-coding/general-coding` – große Dateien begründen und Split planen | MUST              | Fehlende/unvollständige Nachweise: R24; vorhandene Ausnahmen respektiert              |
| `spaex-constitution` – Conventional Commits einschließlich Merges          | MUST              | Verletzt: R29                                                                         |
| `general-coding` – gemeinsame Regeln, klare Zuständigkeiten                | SHOULD            | Konkrete Verbesserungen R05/R07/R17/R19/R20/R21                                       |
| `javascript-typescript` – Fehler sichtbar propagieren                      | SHOULD/SHOULD_NOT | R15                                                                                   |
| JS-/Rust-Testregeln – Fehlergrenzen, Concurrency, echte Verträge           | SHOULD            | R01–R15 und R25–R27 benennen konkrete fehlende Regressionen                           |
| `general-coding` – keine Abstraktion allein wegen optischer Ähnlichkeit    | MUST_NOT          | Empfehlungen beschränken sich auf belegte gemeinsame Domänenregeln                    |
| Graphify vor neuem Authoring                                               | MUST/SHOULD       | Historische Einhaltung aus Quellcode nicht beweisbar; kein rückwirkender Vorwurf      |
| Worktree, PR, Review von Harness-Änderungen                                | MUST              | Bericht separat im Topic-Worktree; keine Harness-/Quelländerung oder Veröffentlichung |
| Keine Secrets in Git                                                       | MUST_NOT          | Kein vollständiger History-Secret-Scan; daher keine globale Konformitätsaussage       |

Die in R01–R17 gefundenen Bugs sind nicht automatisch jeweils ein zusätzlicher MUST-Verstoß. Architektur- und Testempfehlungen bleiben Empfehlungen, wenn der zugehörige Atomtext nur SHOULD verlangt. Neue Architekturentscheidungen, die Core Principles materiell verändern, benötigen weiterhin ein ADR; die hier vorgeschlagenen Fehlerkorrekturen sind nicht pauschal Verfassungsänderungen.

## Ausgeführte Verifikation und Grenzen

### Bestandene Prüfungen

- Node-Testlauf mit `--test --test-reporter=dot`: Chat-State/-Navigation/-Live-Data/-Tool-Notice, WM-Navigation/-Actions/-Nav-Store/-Keys/-State/-Geometry/-Persistence/-Session, Agent-Actions, Settings, Vault-Data, Chat-Modellsuche und alle `scripts/e2e/lib/*.test.ts`; Exit 0.
- `node scripts/check-vault-lifecycle.ts`: 10 Tests bestanden.
- `node scripts/check-vault-passphrase-lifetime.ts`: 7 Tests bestanden.
- `node scripts/check-vue-templates.ts`: 77 Templates kompiliert; Regeln zu direkten Writes und Palettenfarben bestanden.
- `pnpm exec vue-tsc --noEmit -p tsconfig.json`: bestanden gegen die vorhandene Nuxt-Konfiguration. Kein frischer Nuxt-Prepare-Lauf.
- `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`: bestanden.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: bestanden.
- `python3 -B scripts/ci/check-docs.py`: bestanden.
- `python3 -B -m unittest discover -s scripts/ci -p 'test_*.py'`: 1 Test bestanden.
- Fünf gezielte Laufzeitproben mit vorhandenen Frontend-Modulen/Harnesses bestätigten R05, R06, R07, R08 und R15. Keine neuen Quell-/Testdateien dafür geschrieben. Diese Proben ersetzen keine dauerhaft eingecheckten Regressionstests.

Die Lifecycle-Harnesses meldeten `useTemplateRef()` ohne aktive Komponenteninstanz. Ihre Tests bestanden trotzdem; tatsächliches DOM-/Mehrkomponentenverhalten wird dadurch nicht bewiesen.

### Rust-Testbaseline

`scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --locked --offline -- --skip export_bindings` bestand mit Exit 0, einschließlich der aktivierten Unit- und Integrationstests sowie Doc-Test-Prüfung. Der Unit-Test-Binary meldete 730 gestartete Tests; hinzu kamen die separaten Integrationstest-Binaries, darunter Sync-Copy/-Devices/-Link/-Removal und Vault-Lebenszyklus. Explizit ignorierte Tests blieben ignoriert. Binding-Exporttests sind bewusst ausgeschlossen, weil sie eingecheckte Dateien überschreiben können. Lokale Sync-Integrationstests sind kein Ersatz für Tests auf zwei physischen Geräten.

### Dependency-Audits

`pnpm audit --prod --json` meldete 7 Advisories: 3 hoch, 2 mittel, 2 niedrig; keine kritischen. Die hohen Meldungen betreffen `devalue@5.9.2` (`pnpm-lock.yaml:2687`). Die Primärmeldungen nennen Korrekturen ab 5.9.3: [Speicheroffenlegung bei Serialisierung](https://github.com/sveltejs/devalue/security/advisories/GHSA-j22f-vq7h-c4qm), [quadratische Ausgabe](https://github.com/sveltejs/devalue/security/advisories/GHSA-mcm9-63f2-9j32), [unbehandelte Async-Rejection](https://github.com/sveltejs/devalue/security/advisories/GHSA-x5rw-q4pp-hg5g).

Das sind **keine drei nachgewiesenen haex-vault-Sicherheitslücken**: HaexVault verwendet `ssr: false` und liefert einen statischen Tauri-Frontend-Build aus. Ein erreichbarer Pfad mit angreiferkontrollierten Daten in den betroffenen Serialisierungsfunktionen wurde hier nicht nachgewiesen. Abhängigkeiten in einem separaten Update aktualisieren und die tatsächliche Laufzeit-/Build-Erreichbarkeit prüfen; keine pauschale SSR-Server-Gefährdung behaupten.

`cargo audit --file src-tauri/Cargo.lock --json` meldete `RUSTSEC-2026-0285` für `rustls 0.23.44`, außerdem fünf Unmaintained- und eine Unsound-Warnung. Die [Rustls-Primärmeldung](https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc) bewertet den Protokollfehler als mittel und nennt 0.23.45 als korrigierte Version. Der Handshake-Transcript bleibt authentifiziert; die Meldung belegt keine beliebige Manipulation oder Übernahme eines Handshakes. Rustls liegt in HaexVault im Netzwerkpfad. Das Lockfile-Update getrennt prüfen und anschließend Audit sowie Netzwerk-/Sync-Tests ausführen. Warnungen wurden nicht als zusätzliche bestätigte haex-vault-Bugs gezählt.

### Nicht vollständig verifiziert

- Kein kompletter visueller GUI-Durchlauf, keine Verbindung zwischen zwei physischen Geräten, kein neuer Mehrprozess-Crash-Recovery-Test für die gefundenen Lücken.
- Keine Live-Provider-, GPU-/Metal-/CUDA-, Whisper-/Mikrofon- oder Mobilgeräteprüfung.
- Kein vollständiger Default-Feature-Rust-Testlauf und kein separater Clippy-Lauf für beide Feature-Sätze in diesem Review.
- Kein vollständiger Git-History-Secret-Scan und kein Audit sämtlicher transitiver Dependency-Implementierungen.
- Breite Erfassung von 590 Quell-/Test-/Skriptdateien in `src`, `src-tauri/src`, `src-tauri/tests`, `scripts`; vertiefte Prüfung der genannten Kernpfade. Keine Behauptung einer lückenlosen Zeile-für-Zeile-Prüfung aller Dateien.
- Begrenzte Graphify-Abfragen lieferten überwiegend fachfremde oder veraltete Zuordnungen; daher gezielte Quellprüfung als Fallback. `ast-grep` war nicht verfügbar; `sg` ist hier das Unix-Gruppenprogramm. Keine neuen benannten Codeartefakte erstellt. Der Beratungs-Worktree enthält keinen eigenen ignorierten Graph-Snapshot; vor späterer Implementierung muss dieser gemäß Projektworkflow bereitstehen.

## Erwogen und bewusst nicht als Finding gewertet

- Ein Vault je Prozess und die in-process Action-Bridge sind akzeptierte Entscheidungen aus ADR-0003/0006.
- Verschlüsselte Anbieter-Zugangsdaten in SQLCipher sind ausdrücklich vorgesehen; daraus folgt kein Zwang zu einer zusätzlichen OS-Keychain-Schicht.
- Allgemein fehlende UI-Invalidierung nach partiellen Sync-Pulls trifft nicht zu: `vault_events` übernimmt Commit-Benachrichtigungen auch unabhängig vom abschließenden Sync-Callback.
- Noch nicht implementierte spätere Extension-/MCP-/Sharing-Features und ausdrücklich vertagte Link-Resume-Funktionen sind kein aktueller Bug.
- Große Dateien mit vollständiger Ausnahme/Splitplan wurden nicht allein ihrer Länge wegen beanstandet; ähnliche Test-Arrange-Blöcke sind nicht automatisch schädliche Duplikation.
- Dokumentiertes `noImplicitAny: false` in `tsconfig.scripts.json` ist eine ausdrücklich erlaubte Migrationsausnahme.
- Action-Bridge-Cancellation nach bereits gestarteter Frontend-Action bleibt ein Untersuchungsgegenstand. Mangels verifiziertem konkretem Ablauf hier kein zusätzliches bestätigtes Finding.

## Empfohlene Umsetzungsreihenfolge

1. **Daten-/Autorisierungsgrenzen:** R01–R04 zuerst, jeweils mit deterministischen Unterbrechungs-/Konkurrenztests. Diese Änderungen getrennt halten; ein großer pauschaler Sync-Refactor erschwert die Prüfung.
2. **Chat-Instanzgrenzen:** R25 als rote Regression für R05/R07 aufbauen, anschließend R05–R08 und R16 korrigieren. Die gemeinsame Zustandsverantwortung vor weiteren Chat-Features festlegen.
3. **Fehler und Lebenszyklen:** R09/R10/R12/R15 sowie R13/R14. Bei R15 auch die tatsächlichen Backend-Fehler aus R09 sichtbar weiterreichen.
4. **Modelldateien:** R11 vor R28; R04 separat absichern. R18 kann in kleinen domänenspezifischen Schritten erfolgen.
5. **Gezielte Vereinheitlichung:** R17/R19/R20/R21 nach den zugehörigen Verhaltenstests. Bestehende Domain-Module erweitern, keine neue allgemeine Infrastruktur bauen.
6. **Regeln und CI:** R22/R23/R26/R27/R29 sind überschaubare eigenständige Änderungen. R24 nach fachlicher Verantwortlichkeit und nicht als reine Zeilenzahlübung behandeln.

Für Quelländerungen bleiben die vorgesehenen Spec-Kit-Stufen und PR-Review-Gates maßgeblich. Dieser Bericht priorisiert Befunde und Lösungsvorschläge; er ersetzt weder normative Specs noch die Abnahme einer konkreten Implementierung.
