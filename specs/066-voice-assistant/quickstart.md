# Quickstart: Sprachassistent prüfen

**Spec**: [spec.md](./spec.md) | **Verträge**: [contracts/](./contracts/)

## Voraussetzungen

- Entsperrter Test-Vault mit Standard-Chatmodell (lokal oder Cloud), STT-Modell, Stimme und den
  Assistent-Modellen (`assistant_readiness` meldet nichts Fehlendes).
- Freigabestufe „Manuell“ im Chat, ein Passwort-Eintrag „Gäste-WLAN“, haex-mail mit mindestens
  einer ungelesenen Mail (oder eine andere Aktion mit Änderung).
- Lautsprecher und Mikrofon; für M6 zusätzlich Kopfhörer; für M14 ein zweites Gerät mit demselben
  Vault.

## Automatisch

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 \
  --manifest-path src-tauri/Cargo.toml assistant
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 \
  --manifest-path src-tauri/Cargo.toml --test assistant_conversation --test assistant_secret_contract
nix develop --command node --test scripts/check-assistant-*.ts
nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e --grep voice-assistant
# e2e-Build mit Feature e2e-audio: Audio kommt aus einem WAV-Skript statt vom Mikrofon
```

Wake-Word-Messung (SC-003, SC-004) und Echo-Messung (SC-005) laufen nur im Release-Build:

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test --release -j 4 \
  --manifest-path src-tauri/Cargo.toml --test assistant_wakeword_eval -- --ignored --nocapture
```

## Von Hand

| #   | Schritt                                                                                                       | Erwartet                                                                                                 | Spec            |
| --- | ------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | --------------- |
| M1  | Launcher → „Sprachassistent“; haex minimieren; „Hey Haex, wie spät ist es?“                                   | Ton „Modus ein“, Anzeige „bereit“, Tray-Symbol; Ton „hört zu“ < 0,3 s; gesprochene Antwort               | US1, SC-001/002 |
| M2  | Ohne Wake Word nachfragen; während der Antwort „Okay, reicht“; „Danke, das war's“                             | Anschlussrunde mit Kontext; Ausgabe stoppt < 0,5 s; Ton „Gesprächsende“                                  | US2             |
| M3  | „Haex, schick Jonas eine Mail, dass ich später komme“ → „Ja“; zweiter Versuch → schweigen                     | Ansage mit Ziel, Ausführung nach „Ja“; bei Stille bleibt die Frage offen, nächstes Wake Word stellt sie  | US3, FR-024/040 |
| M4  | „Erledige X und gib mir Bescheid“, dann schweigen; anderer Durchlauf: „Danke, das war's“ während X läuft      | Meldung von selbst mit Ton „Aufmerksamkeit“; aktives Ende bricht X ab und sagt den Stand                 | FR-039/040      |
| M5  | „Wie ist das Passwort vom Gäste-WLAN?“ → „Ja“ → „Nochmal langsamer“                                           | Ton „Geheimnis“, buchstabiert; Mitschrift nur Vorlese-Hinweis; Modell-Eingabe im Debug-Log ohne Passwort | US6             |
| M6  | Lange Antwort über Lautsprecher, dann über Kopfhörer dazwischenreden                                          | Verhalten wie in `docs/voice-assistant-platforms.md` für diese Plattform                                 | FR-016          |
| M7  | Eigenes Wake Word „Hallo Eichhörnchen“ einsprechen, Probephase, übernehmen                                    | Fortschritt, Prüfung, danach löst nur das eigene Wort aus; keine Aufnahme im Datenordner                 | US7             |
| M8  | Andere Stimme wählen, Probe, Tempo 1,2; externen Dienst einrichten, dann Netz trennen                         | Nächste Antwort mit neuer Stimme; Anzeige kennzeichnet Dienst; ohne Netz lokale Stimme + einmal Hinweis  | US8             |
| M9  | Android: Modus an, App verlassen, Bildschirm sperren, Wake Word; Anruf annehmen                               | Benachrichtigung „haex hört zu“; Gespräch läuft; während des Anrufs „pausiert“, danach weiter            | US9             |
| M10 | iOS: Modus an, App in den Hintergrund                                                                         | Kein Zuhören; beim Öffnen einmal erklärt; im Vordergrund wieder „bereit“                                 | US9             |
| M11 | Vault sperren mitten in einer Antwort                                                                         | Ausgabe stoppt sofort, Tray-Symbol weg; nach Entsperren Modus wieder an                                  | FR-004          |
| M12 | Alle Töne in den Einstellungen anhören; 5 Testpersonen ordnen die Kerntöne zu                                 | Mindestens 4 von 5 richtig                                                                               | SC-014          |
| M13 | Je eine Woche mit Whisper und mit Parakeet sprechen (Push-to-Talk und Assistent, de und en)                   | Parakeet erkennt mindestens so gut und antwortet spürbar schneller; erst danach eigener PR ohne Whisper  | R7              |
| M14 | Eigene Stimme: 20 s einsprechen, Probe hören, Einwilligung bestätigen, speichern; zweites Gerät öffnen        | Ohne Häkchen kein Speichern; Stimme auf beiden Geräten wählbar; Löschen wirkt auf beiden                 | US8, FR-050     |
| M15 | Community-Modell (z. B. Pocket TTS Polnisch) als Ordner importieren; einen Ordner mit `.so`-Datei importieren | Erstes wählbar und spricht; zweites mit Grund abgelehnt                                                  | FR-051          |
| M16 | Auf Französisch fragen, „non“ auf eine Freigabe, „stop“ während der Antwort                                   | Antwort auf Französisch mit französischer Stimme; Freigabe abgelehnt; Ausgabe stoppt                     | FR-052          |
