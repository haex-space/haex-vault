# Quickstart: Chat-Historie verwalten

## Voraussetzungen

- HaexVault mit einer geöffneten Vault starten.
- Mindestens vier persistierte Threads mit bekannten unterschiedlichen
  Eröffnungszeitpunkten bereitstellen: unter einer Minute, mindestens eine
  Minute, mindestens zwei Stunden und mindestens fünf Tage.
- Die Chat-Historie in Deutsch und anschließend in Englisch öffnen.

## 1. Duration-Anzeige

1. Den Verlauf öffnen.
2. Prüfen: Jeder Eintrag zeigt die vergangene Dauer dauerhaft am rechten Rand.
3. Prüfen: Die Beispiele erscheinen als `0min`, `1min`, `2h` und `5d` bzw. mit
   den exakt gleichen kompakten Einheiten.
4. Einen Eintrag geöffnet lassen, bis die nächste Einheiten-Grenze erreicht ist.
5. Prüfen: Die Dauer aktualisiert sich ohne manuellen Reload.
6. Einen langen Titel bzw. eine schmale Fensterbreite verwenden.
7. Prüfen: Der Titel nutzt den verfügbaren Raum, wird bei Bedarf gekürzt und
   die Dauer bleibt rechtsbündig sichtbar.
8. Einen fehlenden oder unbrauchbaren Eröffnungszeitpunkt simulieren.
9. Prüfen: Der Eintrag zeigt weiterhin `0min` und keinen technischen Rohfehler.
10. Tastaturfokus bzw. zugängliche Zusatzinformation der Dauer prüfen.

## 2. Titel ändern

1. Einen Verlaufseintrag fokussieren oder mit dem Mauszeiger darüber fahren.
2. Prüfen: Bearbeiten/Pencil erscheint direkt links neben der Dauer, ist
   erreichbar und verständlich beschriftet. Im ausgeblendeten Zustand bleibt
   mehr Platz für den Titel verfügbar.
3. Einen neuen gültigen Titel eingeben und mit `Enter` speichern.
4. Prüfen: Der neue Titel erscheint sofort nach erfolgreicher Persistenz und
   bleibt nach einem Reload erhalten.
5. Prüfen: Dauer, Nachrichten und aktiver Gesprächskontext sind unverändert.
6. Erneut bearbeiten, `Escape` drücken und prüfen: Der Entwurf wird verworfen.
7. Einen leeren, aus Leerzeichen bestehenden und überlangen Titel testen.
8. Prüfen: Jeder ungültige Titel bleibt ungespeichert und zeigt eine
   verständliche Fehlermeldung.

## 3. Thread löschen

1. Einen nicht benötigten Thread fokussieren oder mit dem Mauszeiger darüber
   fahren.
2. Prüfen: Löschen erscheint direkt links neben der Dauer, ist erreichbar und
   verständlich beschriftet.
3. Löschen aktivieren und die Bestätigung abbrechen.
4. Prüfen: Thread, Nachrichten und Verlauf bleiben unverändert.
5. Löschen erneut aktivieren und bestätigen.
6. Prüfen: Thread und Nachrichten verschwinden aus Verlauf und Unterhaltung;
   ein Reload stellt sie nicht wieder her.
7. Den aktiven Thread auswählen, während eines laufenden Turns löschen und
   bestätigen.
8. Prüfen: Der laufende Turn wird zuerst abgebrochen und erst nach seinem
   terminalen Zustand gelöscht; bei fehlgeschlagenem Abbruch bleibt der Thread
   erhalten.
9. Prüfen: Nach erfolgreicher Löschung erscheint eine neue leere Session; kein
   anderer Thread wird automatisch geöffnet.

Für reproduzierbare Dauerfälle verwendet `scripts/check-chat-state.ts` feste
Unix-Millisekunden-Zeitstempel und keine Wartezeiten. Ein manueller Durchlauf
kann dieselben Fälle über entsprechend alte Test-Threads beziehungsweise
einen unbrauchbaren `created_at`-Wert nachstellen.

## 4. Accessibility und Sprache

1. Alle Bearbeiten- und Löschen-Aktionen ausschließlich per Tastatur bedienen.
2. Prüfen: Fokuszustände, zugängliche Namen, Eingabefehler und Bestätigung
   sind verständlich.
3. Die Szenarien in Deutsch und Englisch wiederholen.
4. Prüfen: Keine neu eingeführte sichtbare oder zugängliche Meldung fehlt in
   einer Sprache.

## 5. Automatisierte Checks

```bash
cargo test --manifest-path src-tauri/Cargo.toml
pnpm typecheck
pnpm lint
node scripts/check-chat-state.ts
```
