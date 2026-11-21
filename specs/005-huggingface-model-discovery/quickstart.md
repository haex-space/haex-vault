# Quickstart: Freie Hugging-Face-Modellsuche und Installation

## Voraussetzungen

- HaexVault mit geöffneter Vault starten.
- Mindestens ein bestehendes Katalog- oder Importmodell für Regressionstests
  installiert haben.
- Für automatisierte Tests: `cargo test` und `pnpm typecheck` ausführen.
- Für HTTP-Tests ausschließlich `wiremock`-Fixtures verwenden; keine echte
  Hugging-Face-Anmeldung und keine Tokens benötigen.

## 1. Kuratierte Qwen3-Vorschläge bleiben erhalten

1. Onboarding oder Modellverwaltung öffnen.
2. Prüfen: Qwen3 0.6B, 1.7B und 4B erscheinen entsprechend der bestehenden
   Hardware-Tier-Logik als kuratierte Vorschläge.
3. Prüfen: Kein Qwen2.5-Modell wird als neuer Standard vorgeschlagen.

## 2. Öffentliche Suche

1. Settings öffnen und „Modell von Hugging Face suchen“ auswählen.
2. `llama gguf` suchen.
3. Prüfen: Repository-ID, Name/Autor und nur Repositories mit installierbaren
   GGUF-Dateien erscheinen.
4. Ein Repository öffnen, das mehrere GGUF-Dateien enthält.
5. Prüfen: Jede Datei wird einzeln mit Größe soweit vorhanden, Quantisierung
   soweit ermittelbar, Lizenzstatus und Hardware-Fit angezeigt.
6. Eine Suche mit `x` und eine Suche mit Whitespace ausführen.
7. Prüfen: Keine Netzwerkanfrage für ungültige Queries; lokalisierter Hinweis.
8. Netzwerkfehler simulieren.
9. Prüfen: Vorherige Ergebnisliste bleibt sichtbar und Retry ist möglich.

## 3. Download eines eigenen Modells

1. Eine GGUF-Datei auswählen, deren Fit `Fits` oder `Unknown` ist.
2. Prüfen: Preview zeigt Repository, Datei, Größe, aufgelöste Commit-SHA,
   optionalen verfolgten Branch/Tag, Tokenizer-Repository und lokale Modell-ID.
3. Falls kein sicherer Tokenizer ermittelt wurde, ein gültiges Tokenizer-
   Repository eingeben.
4. Download starten.
5. Prüfen: Fortschritt erscheint; eine temporäre Datei wird während des
   Downloads nicht als installiertes Modell gelistet.
6. Nach Abschluss prüfen: Modell erscheint in der lokalen Liste, im bestehenden
   Chat-Picker und kann geladen werden.
7. App schließen und neu öffnen.
8. Prüfen: Modell kann ohne erneute Hugging-Face-Suche geladen werden.
9. Die Modellverwaltung aktualisieren und einen neuen Stand des verfolgten
   HF-Refs simulieren.
10. Prüfen: HaexVault zeigt installierte und neue Commit-SHA sowie eine
    ausdrückliche Update-Aktion; das Modell wird nicht automatisch ersetzt.
11. Update installieren und prüfen: Die neue Datei wird atomar unter derselben
    Modell-ID veröffentlicht; bei einem simulierten Fehler bleiben alte Datei
    und alte SHA nutzbar.

12. Eine installierte GGUF-Datei testweise verändern oder ersetzen und das
    Modell laden.
13. Prüfen: Vor dem Runtime-Load erscheint ein Integritätsdialog mit erwarteter
    und aktueller SHA sowie den Aktionen „trotzdem als unsicher laden“,
    „dasselbe HF-Modell erneut herunterladen“ und „anderes Modell auswählen“.
14. „Trotzdem als unsicher laden“ wählen und prüfen: Das Modell wird geladen,
    `integrityStatus` steht auf `untrusted`, aber die erwartete SHA bleibt
    unverändert.

## 4. Warnung und Fehlerfälle

1. Eine sehr große GGUF-Datei auswählen.
2. Prüfen: `TooBig` zeigt eine deutliche Warnung und blockiert den Download ohne
   explizite Bestätigung.
3. Download abbrechen oder Verbindung unterbrechen.
4. Prüfen: Die Datei erscheint nicht in `list_installed_models`; bestehende
   Modelle bleiben nutzbar.
5. Einen Nicht-GGUF-Treffer in einer Fixture prüfen.
6. Prüfen: Er wird nicht als installierbare Datei angeboten.
7. Ein ungültiges Repository oder einen Dateinamen mit `../` an den Command
   senden.
8. Prüfen: `InvalidInput`; kein Zugriff außerhalb des Modellverzeichnisses.

## 5. Modellwechsel und Entfernen nach dem Onboarding

1. Onboarding abschließen und die Modellverwaltung erneut über Settings öffnen.
2. Prüfen: Katalog, freie Suche, installierte Modelle und aktiver Modellwechsel
   sind dauerhaft erreichbar.
3. Ein anderes installiertes Modell aktiv auswählen.
4. Ein Modell entfernen und die explizite Bestätigung erteilen.
5. Prüfen: Nur die lokalen Modellbytes werden gelöscht; die bestehende
   Spec-002-Fallback-Kette bleibt konsistent.

## 6. Gemeinsame Modell- und Fallback-Semantik

1. Ein Katalogmodell und ein freies HF-Modell gleichzeitig installiert lassen.
2. Beide im Picker laden und je eine Nachricht senden.
3. Prüfen: `chat.last_active_model_id` folgt unverändert der Spec-002-Regel.
4. Freies Modell lokal entfernen.
5. Prüfen: Katalogmodell, Provider-Modelle und synchronisierte Metadaten bleiben
   erhalten; der nächste Chat-Start verwendet die bestehende Fallback-Kette.

## Automatisierte Checks

```bash
cd src-tauri
cargo test
cd ..
pnpm typecheck
git diff --check
```

Die Tests müssen mindestens Query-Validierung, GGUF-Filter, Sortierung,
Repository-/Dateiname-Validierung, SHA-Auflösung, Datei-Hash-Prüfung vor jedem
Load, Hash-Mismatch mit allen drei Nutzerentscheidungen, Update-Erkennung,
fehlenden Tokenizer, HTTP-Fehler, Timeout, Download-Abbruch, atomare
Veröffentlichung und Source-Metadaten-Roundtrip abdecken.
