# Contract: Sitzung wiederherstellen

## 1. Tauri-Befehle

Alle Befehle laufen im Vault-Gate wie die übrigen (`gate.wrap` in
`src-tauri/src/lib.rs`), ermitteln die Gerätekennung selbst
(`current_device_uuid`) und liefern `HaexError` im Fehlerfall. Wire-Typen
werden per ts-rs nach `src/types/bindings/` exportiert. Sie ersetzen die sechs
Befehle aus Spec 015 (`wm_load_layout`, `wm_create_workspace`,
`wm_delete_workspace`, `wm_set_active_workspace`, `wm_save_windows`,
`wm_close_windows`), die entfallen.

### `wm_session_restore_get() -> SessionRestoreState`

Liest Gerätewert und Vault-Wert von `wm.session_restore` und den geltenden Wert
(data-model.md). Schreibt nichts.

### `wm_session_restore_set(args: { scope: 'device' | 'vault', enabled: boolean | null }) -> SessionRestoreState`

- `enabled: true | false` setzt den Wert im gewählten Scope, `null` löscht ihn
  (Zurücksetzen, US3 AS5).
- In derselben Transaktion: gilt danach auf diesem Gerät nichts mehr, wird die
  eigene Zeile in `wm_sessions_no_sync` gelöscht (FR-007).
- Gibt den neuen Zustand zurück. Das Frontend speichert sofort, wenn
  `effective` von falsch auf wahr gewechselt ist (FR-005).

### `wm_session_load() -> { restore: SessionRestoreState, session: unknown | null }`

- Gilt die Einstellung: gibt die eigene Sitzung als geparstes JSON zurück, oder
  `null`, wenn keine gespeichert ist.
- Gilt sie nicht: löscht eine vorhandene eigene Zeile und gibt `session: null`
  zurück (FR-008, FR-012 letzter Satz).
- Schreibt sonst nichts. Anders als `wm_load_layout` legt es keinen
  Standard-Arbeitsbereich in der Datenbank an; den leeren Arbeitsbereich erzeugt
  das Frontend.

### `wm_session_save(args: { session: unknown }) -> { saved: boolean }`

- Prüft: gültiges JSON-Objekt (sonst `HaexError::InvalidInput`), serialisiert
  höchstens 4 MiB (sonst `HaexError::SessionTooLarge { bytes }`; das Frontend
  versucht es dann einmal ohne Historien, data-model.md „Größe“).
- Gilt die Einstellung nicht, schreibt es nichts und gibt `saved: false` zurück
  (ein verspätetes Speichern nach dem Ausschalten legt nichts an).
- Sonst überschreibt es die eigene Zeile (`INSERT … ON CONFLICT DO UPDATE`,
  erlaubt, weil keine CRDT-Trigger beteiligt sind) und gibt `saved: true` zurück.

## 2. Start und Wartung (Rust, nach `Database::open`)

In `src-tauri/src/instances/open.rs` (und `create.rs` für neue Vaults), nach
erfolgreichem Öffnen, vor der ersten Anfrage des Frontends:

1. `PRAGMA secure_delete = ON` (research R6).
2. `DELETE FROM preferences WHERE key = 'shell.active_workspace_id'` (alle
   Geräte, idempotent, research R5).
3. Steht `vacuum_after_legacy_wm_drop` in `haex_maintenance_no_sync`:
   `VACUUM`, `PRAGMA wal_checkpoint(TRUNCATE)`, dann die Zeile löschen.

Jeder Schritt protokolliert Fehler und lässt das Öffnen weiterlaufen (FR-012).
Ein Fehler in Schritt 3 lässt die Zeile stehen, der nächste Start versucht es
erneut.

## 3. Aktionen (Katalog aus Spec 020)

In `src/lib/actions/settingsActions.ts`, Handler in
`src/stores/settingsActionHandlers.ts`.

| Kennung                         | Eingabe                                            | Ergebnis              | Bereich           | Wirkung | Agent |
| ------------------------------- | -------------------------------------------------- | --------------------- | ----------------- | ------- | ----- |
| `settings.sessionRestore.set`   | `{ scope: 'device' \| 'vault', enabled: boolean }` | `SessionRestoreState` | `settings.device` | write   | ja    |
| `settings.sessionRestore.clear` | `{ scope: 'device' \| 'vault' }`                   | `SessionRestoreState` | `settings.device` | write   | ja    |

Beide rufen `useWindowManagerStore().setSessionRestore(scope, enabled)`. Das
schickt `wm_session_restore_set` über dieselbe Warteschlange wie das Speichern
(kein früheres Speichern kann danach ankommen) und übernimmt den neuen Zustand.
`settings.get` ergänzt `sessionRestore` über `getSessionRestore()`.

Das Ergebnis der Aktionen ist `SessionRestoreState` ohne die nicht gesetzten
Werte (`toRestoreResult`): `{ effective, device?, vault? }`. Die Schemas des
Katalogs kennen kein `null` (Spec 020, R8), und sie werden in Spec 021 zur
Tool-Beschreibung für Agenten. Die Einstellungsansicht setzt die fehlenden
Werte mit `fromRestoreResult` wieder auf `null`.

## 4. Store (`src/stores/windowManager.ts`)

Die Logik liegt in `src/lib/wm/sessionSync.ts` (rein, unter Node testbar); der
Store gibt ihr seinen Zustand, die Historien-Map und `useWmSession()`.

| Mitglied                            | Verhalten                                                                                                                 |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| geltender Wert (intern)             | `false`, bis `restoreSessionAsync` ihn gesetzt hat                                                                        |
| `restoreSessionAsync()`             | ersetzt `hydrateFromBackendAsync`: `wm_session_load`, dann `hydrate` und Historien der Tabs übernehmen, oder leerer Start |
| `setSessionRestore(scope, enabled)` | setzt oder löscht (`null`) einen Wert und übernimmt den Zustand; wird er wahr, sofort speichern                           |
| `getSessionRestore()`               | liest Gerätewert, Vault-Wert und geltenden Wert                                                                           |
| `flushAsync()`                      | wie bisher, wartet die Speicher-Warteschlange ab (vor dem Sperren)                                                        |

`createWorkspace` erzeugt die Kennung selbst (`crypto.randomUUID()`) und ist
synchron. `switchWorkspace`, `deleteWorkspace` und alle Fenster- und
Tab-Änderungen lösen sofortiges bzw. entprelltes Speichern aus, Navigation im
Tab (`navigate`, `goTab`, `skipCurrent`) entprelltes; ohne geltende Einstellung
tut beides nichts.

## 5. Einstellungsansicht

`src/components/settings/SessionRestoreSetting.vue` in `SettingsApp.vue` nach
`SettingsAliasSetting`:

- Überschrift „Sitzung wiederherstellen“, eine Zeile Erklärung (was gespeichert
  wird, dass es nur dieses Gerät betrifft).
- Eine Auswahl „Aus / Nur auf diesem Gerät / Auf allen Geräten dieser Vault“
  (FR-004), gespeichert beim Wählen, ohne Knopf zum Übernehmen.
  `restoreChoice(state)` bestimmt die gewählte Option,
  `restoreChoiceSteps(state, choice)` in `src/lib/wm/sessionSync.ts` die
  Aktionsaufrufe in einer Reihenfolge, in der eine eingeschaltete
  Wiederherstellung zwischendurch nie aus ist.
- Alle Änderungen laufen über `useActionOrThrow` (Spec 020 FR-024,
  `check:templates`).
- i18n-Schlüssel unter `settings.sessionRestore.*` (de, en).
