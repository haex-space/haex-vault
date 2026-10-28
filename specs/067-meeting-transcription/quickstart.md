# Quickstart: Meeting-Mitschrift prüfen

**Spec**: [spec.md](./spec.md) | **Verträge**: [contracts/](./contracts/)

## Voraussetzungen

- 066 PR B–D umgesetzt (Audio-Strom, Erkennung, Wake Word); für Hintergrund auf Android 066 PR H, für
  Online-Meetings 066 PR G (Echo-Unterdrückung).
- Entsperrter Test-Vault mit Kontakt „Jonas Weber“, eingerichtetem Stimmprofil, Standard-Chatmodell.
- Testmeeting-Aufnahmen aus `src-tauri/src/meetings/fixtures/` (vier Sprecher, bekannte Zuordnung).

## Automatisch

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 \
  --manifest-path src-tauri/Cargo.toml meetings
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 \
  --manifest-path src-tauri/Cargo.toml --test meetings_session
nix develop --command node --test scripts/check-meetings-*.ts
nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e --grep meeting
```

Messungen (Release, `--ignored`): `--test meetings_diarization_eval` (SC-003, SC-004),
`--test meetings_stt_eval` (SC-002, SC-005).

## Von Hand

| #   | Schritt                                                                                        | Erwartet                                                                                            | Spec           |
| --- | ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | -------------- |
| M1  | Launcher → „Meeting aufzeichnen“, Einwilligung bestätigen, mit drei Personen 10 Minuten reden  | Live-Mitschrift < 5 s, „Ich“ und „Sprecher 1/2“, Anzeige und Tray „Meeting“                         | US1            |
| M2  | Während des Meetings „Sprecher 1“ mit Kontakt verknüpfen; danach einen Eintrag umhängen        | Name auf allen Einträgen; Eintrag wechselt; nach Ende Hinweis auf verbesserte Zuordnung             | US2, FR-011    |
| M3  | Zwei Sprecher zusammenführen, einen Eintrag teilen, Text korrigieren                           | Ein Teilnehmer; zwei Einträge; „bearbeitet“ mit Original                                            | US2            |
| M4  | Stimmprofil löschen und neu einrichten                                                         | Nächstes Meeting „Sprecher N“, danach wieder „Ich“                                                  | US3            |
| M5  | Meeting mit „Audio behalten“; Eintrag antippen; Audio löschen                                  | Stelle spielt; nach Löschen nur Mitschrift; Größe ≤ 30 MB/h                                         | US4            |
| M6  | Online-Meeting (Jitsi) mit Kopfhörern, Quelle „Mikrofon und Geräteton“; dann über Lautsprecher | Entfernte Teilnehmer in der Mitschrift; über Lautsprecher kein Satz doppelt                         | FR-028, SC-010 |
| M7  | Im Meeting „Haex, was hat Jonas eben zum Budget gesagt?“                                       | Antwort aus der Mitschrift, als Assistent-Eintrag; Aufzeichnung läuft weiter                        | US5            |
| M8  | Nach dem Meeting „Auswerten“, Aufgabe „Übernehmen“                                             | Zusammenfassung, Beschlüsse, Aufgaben mit Verweisen; Übernehmen startet eine Anweisung mit Freigabe | US6            |
| M9  | Suche nach einem Wort; Export als Markdown; Meeting löschen                                    | Treffer mit Sprung; Datei vollständig; alles auf beiden Geräten weg                                 | US7            |
| M10 | Android: Meeting starten, Bildschirm sperren, 30 Minuten                                       | Läuft weiter; Benachrichtigung; Akku ≤ 15 %/h                                                       | FR-009, SC-007 |
| M11 | Vault sperren während eines Meetings                                                           | Meeting endet, Mitschrift bis dahin da; beim Öffnen „Zuordnung verbessern“ angeboten                | FR-007         |
