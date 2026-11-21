# Quickstart: Onboarding-Härtung und Modellwahl-Persistenz manuell verifizieren

Dieses Dokument beschreibt die manuellen Verifikations-Schritte für Reviewer und Operator vor dem Merge. Automatisierte Tests laufen in `cargo test` und decken den Backend-Anteil ab; Frontend-Verifikation ist manuell (kein Playwright).

## Vorbereitung

```bash
# 1. Aus main den Feature-Branch checken
git switch 002-onboarding-model-prefs

# 2. Dependencies aktuell
pnpm install

# 3. Backend-Tests laufen lassen — MUSS grün sein
cd src-tauri
cargo test --lib
cargo test --test bootstrap
cargo test --test provider_models
cargo test --test preferences_roundtrip  # neu in diesem Feature
cd ..

# 4. Frontend-Typecheck
pnpm typecheck
```

Erwartetes Ergebnis: alle Test-Suites grün, `pnpm typecheck` exit 0.

## Szenario 1: Genesis — neue Vault, erstes Öffnen

**Aufsetzen**: existierende Test-Instances-Ordner löschen (oder eine noch nicht verwendete Instanz-Namen benutzen).

**Schritte**:

1. `pnpm tauri:dev:cuda` starten (oder `pnpm tauri:dev` für CPU-only).
2. In der Landing "Anlegen" klicken, Instanznamen "genesis-test" eingeben, Passphrase setzen, öffnen.
3. **Erwartung**: Nutzer landet direkt auf `/onboarding/genesis-test`, NICHT auf `/chat/genesis-test`.
4. Im Wizard: Alias-Feld ist mit dem OS-Hostname vorbefüllt (z.B. "pop-os" oder ähnlich). Modell-Vorschläge zeigen drei Chips: "Easy" (Qwen 0.5B), "Sweet" (Qwen 1.5B oder Llama 3.2 3B je nach HW), "Max" (Qwen 7B wenn genug VRAM, sonst gleiches Modell wie Sweet).
5. Ohne Alias-Änderung, ohne Modell-Wahl: klick "später via Anbieter".
6. **Erwartung**: Nutzer landet auf `/workspace/genesis-test` mit dem Instanznamen als Überschrift und einem FAB unten rechts.
7. `preferences`-Tabelle prüfen (via sqlite3-CLI oder DB-Tool nach Öffnen): sollte KEINE Zeile für `chat.default_model_id` haben.
8. `known_devices.alias` prüfen: sollte den vorbefüllten OS-Hostname enthalten.

## Szenario 2: Genesis mit Modellwahl

**Schritte**:

1. Wie Szenario 1 bis Schritt 4.
2. Alias auf "Laptop" ändern, Sweet-Modell wählen (klick auf Chip → Download startet).
3. Download-Fortschritt sichtbar.
4. Nach Download-Abschluss: automatischer Übergang zu `/workspace/genesis-test`.
5. **Erwartung**:
   - `known_devices.alias === 'Laptop'`
   - `preferences[('<my_uuid>', 'chat.default_model_id')]` enthält die Katalog-ID des Sweet-Modells (z.B. `qwen3-1.7b-instruct-q4_k_m`)
   - `preferences[..., 'chat.last_active_model_id']` ist NICHT gesetzt (Onboarding-Wahl ist explizit ein "Default", kein "aktives Modell").
6. FAB klicken → Chat öffnet sich, das Sweet-Modell lädt automatisch mit sichtbarem Ladepanel ("Optimiere GPU für erste Nutzung von …" wenn CUDA-Erst-Load).

## Szenario 3: Adoption — Vault-Datei auf zweites Gerät kopieren

**Aufsetzen**: Vault-Datei aus Szenario 2 (`<AppLocalData>/instances/genesis-test.db`) auf ein anderes System kopieren, oder — für lokale Simulation — den `<AppLocalData>/installation-id`-File temporär löschen (simuliert ein neues Gerät auf demselben Host).

**Schritte** (auf "zweitem Gerät"):

1. `pnpm tauri:dev:cuda` starten.
2. Instanz-Landing zeigt "genesis-test" — anklicken, Passphrase eingeben.
3. **Erwartung**: Redirect auf `/onboarding/genesis-test` (Adoption erkannt: neue `known_devices`-Zeile mit `alias === NULL` → Middleware routet).
4. Im Wizard: Alias-Feld ist wieder mit OS-Hostname vorbefüllt (kann anders sein als Szenario 1 wenn "anderes Gerät" simuliert).
5. **Kritisch**: Die 3 Modell-Vorschläge zeigen weiterhin die Katalog-Optionen — aber der Sidebar-Hint sollte klar sein "auf diesem Gerät noch keine Modelle installiert; auf einem anderen Gerät läuft bereits Sweet". (Optional; wenn nicht umgesetzt, wenigstens die Modell-Vorschläge zeigen.)
6. "Später via Anbieter" wählen — der Sync sollte die Anbietermodelle aus Szenario 2 mitbringen (falls dort Anbieter konfiguriert waren; sonst leer).
7. **Erwartung**:
   - `known_devices` hat jetzt DREI Zeilen für "genesis-test" (Sentinel-Zeile + Gerät-1-Zeile + Gerät-2-Zeile).
   - Auf Gerät 2 ist noch kein `chat.default_model_id` (`preferences` hat nur die alte Row vom Gerät 1 — sichtbar via `SELECT * FROM preferences`).
8. FAB klicken → Chat öffnet. Session-Resolver: `last_active` (leer) → `default_device` für Gerät 2 (leer) → `default_vault` (leer) → `first_available` → lädt ein api_key-Modell wenn vorhanden, sonst zeigt "Kein Modell verfügbar" mit Hint "Anbieter hinzufügen".

## Szenario 4: Session-Persistenz

**Voraussetzung**: Szenario 2 ist durchlaufen, Sweet-Modell ist geladen und mindestens eine Nachricht gesendet.

**Schritte**:

1. App komplett schließen.
2. App neu starten, Vault öffnen.
3. **Erwartung**:
   - Redirect zu `/workspace/genesis-test` (kein Onboarding, weil Alias gesetzt).
   - FAB klicken → Chat lädt AUTOMATISCH das Sweet-Modell (last_active greift, weil send_message es geschrieben hat).
   - Ladepanel zeigt "Lade Qwen3 1.7B…" (Warm-Load, weil bereits einmal geladen — CUDA-Cache warm; Erstlade-Text nur beim allerersten Mal).

## Szenario 5: Ausprobier-Wechsel vergisst sich

**Voraussetzung**: Szenario 4 durchgelaufen, Chat läuft mit Sweet.

**Schritte**:

1. Im Sidebar-Picker das Easy-Modell wählen (manueller Wechsel, ohne Nachricht zu senden).
2. **Erwartung**: `preferences.chat.last_active_model_id` bleibt auf der zuletzt erfolgreich verwendeten Sweet-Katalog-ID.
3. App schließen ohne Nachricht zu senden.
4. App neu starten, Vault öffnen, FAB → Chat.
5. **Erwartung**: Sweet lädt (der Picker-Wechsel ohne `send_message` hatte keinen Persistenz-Effekt).

Damit verifiziert das Szenario FR-009: Ein Modellwechsel ohne erfolgreiches `send_message` gilt als Ausprobieren und ändert `chat.last_active_model_id` nicht.

## Szenario 6: Explizit "Als Standard setzen" (device vs vault)

**Voraussetzung**: Szenario 2, mehrere Modelle verfügbar (installiertes Sweet plus einen konfigurierten Anbieter mit Modellen).

**Schritte**:

1. Aus dem Workspace zum Settings-Screen navigieren (Zahnrad-Icon im Header).
2. **Erwartung**: Screen-Titel enthält Gerätename ("Einstellungen für: Laptop").
3. "Standard-Modell"-Bereich zeigt aktuellen Wert und einen Modell-Selector plus Scope-Radio ("Dieses Gerät" / "Vault-weit").
4. Modell "claude-opus-5" wählen, Scope "Dieses Gerät" → Speichern.
5. **Erwartung**: `preferences[('<my_uuid>', 'chat.default_model_id')] = '<provider_uuid>:claude-opus-5'`.
6. Zurück zur Workspace, App schließen.
7. Neu öffnen, FAB → Chat.
8. **Erwartung**: Claude Opus 5 lädt (default_device gewinnt, weil last_active leer oder für lokales Modell das nicht ladbar/nicht bevorzugt).

Wait — hier ist ein Feinheit: wenn `last_active` gesetzt IST und ladbar, gewinnt es über `default_device`. Das ist FR-014-Reihenfolge. Der Nutzer setzt hier bewusst einen expliziten Default UND ändert nicht sein `last_active`. Der Test müsste also VOR Schritt 4 sicherstellen dass `last_active` entweder leer ist oder dass der Nutzer nach Setzen des neuen Defaults auch bewusst ein Modell manuell wechselt (oder die App vor Chat-Nutzung schließt).

Sauberer Test: nach Schritt 4 auch NICHT den Chat öffnen (der Load würde last_active setzen). Direkt schließen, neu starten — dann greift default_device weil last_active nie geschrieben wurde für dieses Gerät.

## Szenario 7: Sentinel-Idempotenz und FK-Cascade (Backend-Test)

Automatisiert in `src-tauri/tests/preferences_roundtrip.rs`. Manuell verifizieren dass der Test wie folgt aufsetzt:

- Bootstrap läuft zweimal auf derselben DB → nur EIN Sentinel-Row in `known_devices` (dank `INSERT OR IGNORE`).
- Insert `preferences (vault_device_uuid, key, value)` für ein bestehendes Device → OK.
- Insert `preferences` für eine nicht-existierende `vault_device_uuid` → FK-Verletzung.
- Delete `known_devices` für ein Device mit ≥1 Preferences-Zeile → alle diese Preferences-Zeilen verschwinden ebenfalls (ON DELETE CASCADE).

## Non-Regression-Prüfungen

Diese existierenden Flows müssen nach dem Feature unverändert funktionieren:

- Chat mit lokalem Modell (mistralrs): Streaming, Abbruch, Nachrichten-Persistenz.
- Chat mit Anthropic-Adapter: SSE-Streaming, ungültige Credentials als Toast, Persistenz.
- Modell-Download aus Katalog: Fortschritt sichtbar, atomisches Rename am Ende.
- Vault-Anlegen und -Öffnen: `HaexBootstrap` läuft unverändert für Genesis und Resume.
- Anbieter-Hinzufügen: auto-refresh der Modell-Liste.

Wenn einer dieser Flows kaputt geht: Feature-PR ist nicht mergefähig.
