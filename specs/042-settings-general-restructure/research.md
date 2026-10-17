# Research: Allgemein mit Grundeinstellung und Erscheinungsbild

Alle Pfadangaben beziehen sich auf den Stand von `main` @ `3fd5aa53`.

## R1 — Struktur der Einstellungen

**Decision**: Die Kategorie `appearance` fällt aus `SettingsCategoryId` und `SETTINGS_CATEGORIES`
(`src/lib/settings/registry.ts:8-15, :63`). `general` (Pfad `/`) wird zur Übersicht (`OverviewView`) mit
den `subView`s `general.basic` (`/general/basic`) und `general.appearance` (`/general/appearance`), beide
`overviewRow: true`. Die Passwort-Ansicht ist `general.basic.password` (`/general/basic/password`,
`row: true`), dargestellt in der Grundeinstellung als `SettingsRow` mit `to` wie `federation.link` in
`FederationView.vue:82-90`. `subView()` bekommt die Option `settingKeys`, damit die Suche einzelne
Einstellungen weiterhin auf ihren Unterpunkt führt (FR-005).

**Rationale**: Die Übersichts-Mechanik existiert (Modelle, Agenten); nichts Neues. Die Kategorie wird aus
dem ersten Segment der Location-ID abgeleitet (`registry.ts:86-108`), `general.basic.password` landet also
korrekt unter `general`.

**Alternatives considered**: Eigene Kategorie-Komponente mit Tabs — verworfen, doppelt zur vorhandenen
Overview. Alias-Pfad `/appearance` — verworfen, keine Nutzer (Operator 2026-10-07).

## R2 — Sprache vor dem Entsperren

**Decision**: `defaultLocale: 'en'` und `detectBrowserLanguage: { useCookie: false, fallbackLocale: 'en' }`
in `nuxt.config.ts:54-63` (`@nuxtjs/i18n` 10.6.0). Ob das im SPA mit `strategy: 'no_prefix'` die
Systemsprache bei jedem Start ohne Persistenz übernimmt, prüft T001 per Dev-Server-Probe. Fällt die Probe
negativ aus: reine Funktion `systemLocale(languages: readonly string[]): 'de' | 'en'` in
`src/lib/settings/language.ts` plus ein Client-Plugin, das sie mit `navigator.languages` aufruft.

**Rationale**: Laziness-Ladder — erst das Feature der vorhandenen Abhängigkeit, dann minimaler Code.
Die Semantik von `detectBrowserLanguage` ohne Cookie und ohne Präfix ist nicht belegt `[Guessing]`, daher
die Probe vor dem Festlegen (Memory „verify framework behavior empirically“).

**Result T001 (2026-10-07)**: Probe positiv. `pnpm dev`, Chromium mit gestubbter Tauri-API: Locale `de-DE`
→ „Willkommen bei Holzi“, `fr-FR` und `en-US` → „Welcome to Holzi“; weder Cookie noch localStorage-Eintrag.
Kein Plugin, keine `systemLocale`-Funktion nötig. Nicht geprüft: dass WebKitGTK/WebView2 `navigator.language`
aus der Systemsprache (`LANG`) ableiten — quickstart Schritt 2 deckt das in der App ab.

**Alternatives considered**: Sprache gerätelokal merken — vom Operator abgelehnt.

## R3 — Sprache in der Vault

**Decision**: Vault-Präferenz `general.language` (`'de' | 'en'`), gelesen und geschrieben über
`usePreferences` mit `{ kind: 'vault' }`. Neues Composable `useLanguage` nach dem Muster von
`useColorScheme` (`src/composables/useColorScheme.ts:42-80`):

- `loadAsync()`: gespeichert → `setLocale`; fehlt oder ungültig → aktive Sprache schreiben (FR-009).
- `refreshAsync()`: nach Sync-Änderung erneut lesen und anwenden (FR-010).
- `setAsync(lang)`: schreiben, dann `setLocale`.

Aufgerufen wird `loadAsync` in `pages/index.vue` nach `onCreated`/`onUnlocked` (`:65-68`, `:78-81`) —
damit gilt die Sprache schon im Alias-Wizard `pages/onboarding/[instance].vue` — und in
`pages/workspace/[instance].vue` `onMounted` (`:85-95`) für einen direkten Reload. `refreshAsync` hängt an
`onVaultTablesChanged(['preferences'], …)` (`:35-40`).

**Rationale**: `VAULT_PREFERENCE_KEYS` (`maintenance.rs:54`) ist nicht nötig — er steuert nur die einmalige
Umfaltung alter gerätelokaler Zeilen. Präferenzen sind eine Key/Value-Tabelle, keine Migration, keine
ts-rs-Bindings.

## R4 — Passwort ändern

**Decision**: Tauri-Command `change_vault_passphrase(args: { current, new })` in
`src-tauri/src/instances/passphrase_change.rs`:

1. `chat.acquire_operation()` (`chat/session.rs:448-454`) — kein Turn, Modell-Load oder Open läuft parallel.
2. `Passphrase::validate_new(&self)` — neue Methode in `passphrase.rs`; `create.rs:67-71` und
   `link_vault.rs:49-52` nutzen sie statt ihrer duplizierten Prüfung (`MIN_PASSPHRASE_LEN = 8` zieht mit
   um). Zusätzlich `new != current`, sonst `WeakPassphrase`.
3. Pfad aus `state.active_name()` (`state.rs:135`) + `get_instance_path` (`paths.rs:60`).
4. Im `gate.spawn_blocking` (`vault_gate/mod.rs:291`), damit ein Schließen auf das Ende wartet:
   - Prüfung: zweite Connection `open_with_flags(path, READ_ONLY)`, `pragma_update("key", current)`,
     `SELECT COUNT(*) FROM sqlite_master`; `NotADatabase` → `WrongPassphrase` (wie `open.rs:188-200`).
     Connection vor dem Rekey schließen, sonst kann der Journal-Modus WAL nicht verlassen.
   - Unter `db.with_connection` (eine einzige Connection hinter einem Mutex, haex-crdt
     `database/mod.rs:91`): `wal_checkpoint(TRUNCATE)` als `query_row` mit Prüfung `busy == 0` (Muster
     `storage/maintenance.rs:174`) → `journal_mode=DELETE` → `pragma_update("rekey", new)` →
     `journal_mode=WAL`.

**Rationale**: Ablauf aus haex-vault 2.x (Legacy) `src-tauri/src/database/maintenance.rs:134` übernommen (dort belegt:
Rekey funktioniert im WAL-Modus nicht). holzi hält das Passwort nicht im Speicher (`SqlCipherKey::as_str`
ist `pub(crate)` in haex-crdt), daher Prüfung über die Datei. Gleiche Cipher-Einstellungen wie beim Öffnen:
haex-crdt setzt nur `PRAGMA key`, keine KDF-/Kompatibilitäts-Pragmas. Im DELETE-Modus ist der Rekey durch
das Rollback-Journal atomar: Ein Absturz hinterlässt die Datei mit genau einem der beiden Passwörter
`[Likely]`; der Integrationstest prüft mindestens Erfolg und Ablehnung.

Das Passwort spielt im Sync keine Rolle (nur `instances/` und Link-Join), daher entfällt haex-vault-2.x-Vaults
Schritt „Vault-Key auf Sync-Servern neu verschlüsseln“.

ponytail: `pragma_update` rendert Schlüssel in SQL-Text, der nicht genullt wird — dieselbe Grenze wie
`passwords/import/legacy_haex_vault/open.rs:170`; Upgrade-Pfad `sqlite3_rekey_v2` über `rusqlite::ffi`.
Quoting ist sicher (rusqlite verdoppelt `'`).

**Alternatives considered**: Neue Datei schreiben (`sqlcipher_export`) und tauschen — mehr Code, Datei
während offener Session ersetzen; verworfen.

## R5 — Agenten und Passwort

**Decision**: `change_vault_passphrase` steht nicht im Action-Katalog (`src/lib/actions`), ist damit kein
Agent-Tool (FR-014). Agenten öffnen die Ansicht mit der vorhandenen Aktion
`wm.app.open { appId: 'system.settings', at: '/general/basic/password' }` (`wmActions.ts:150`).

**Rationale**: Agenten erreichen nur `ActionTool` (Katalog), `FindActionsTool`, `McpTool`, `CliTool`; keine
Durchreichung beliebiger Commands. Restlücke: `CliTool` kann mit Nutzerfreigabe Shell-Befehle ausführen;
ohne das aktuelle Passwort (nirgends gespeichert) kann ein Agent die Datei aber nicht umschlüsseln.

## R6 — Hintergrundbild speichern und synchronisieren

**Decision**: Vault-Präferenz `appearance.background` als `data:image/webp;base64,…` (fehlt = kein Bild).
`validate_value` in `src-tauri/src/storage/preferences_commands.rs:52` prüft für diesen Schlüssel Präfix
und Höchstgröße 4 MiB (Vertrauensgrenze: die Präferenz kommt auch per Sync und über `set_pref`).

**Rationale**: Ein Wert von 200–600 KB passt in eine Sync-Seite (`PAGE_BUDGET = 4 MiB − 64 KiB`,
`sync/change.rs:21`; größere Werte würden ohnehin geteilt, `Change::split` `:110-132`). T037 prüft
den erwarteten ~500-KB-Fall über zwei Geräte. Nostr trägt keine Zeilendaten. Der CSP erlaubt `data:`
in `img-src` (`tauri.conf.json:28, :40`).

ponytail: jede Änderung an `preferences` liest im Workspace den ganzen Wert erneut über IPC
(`refreshAsync`); bei ≤ 600 KB unkritisch. Upgrade-Pfad: eigene Zeile in `haex_passwords_binaries`-artiger
Tabelle mit Hash-Referenz (`passwords/binaries.rs`).

**Alternatives considered**: eigene CRDT-Tabelle mit BLOB — nur nötig, wenn Präferenzen nicht reichen;
verworfen.

## R7 — Bild auswählen und verkleinern

**Decision**: `<input type="file" accept="image/*">` liefert eine `File`; `createImageBitmap` mit
`resizeWidth/Height` auf höchstens 2560 px der langen Kante, Canvas `toBlob('image/webp', 0.8)`, dann
`FileReader.readAsDataURL`. Dekodierfehler → Fehlermeldung, Hintergrund unverändert.

Bestehender Kandidat: `scaledUrl(bytes, mime)` in `src/composables/usePasswordsThumbnails.ts:27-55`
(gleicher Ablauf, Kante 320, liefert Object-URL). Vorschlag gemäß graphify-first-Regel: den gemeinsamen
Kern als `downscaleToWebp(bytes: ArrayBuffer, mime: string, maxEdge: number): Promise<Blob>` nach
`src/lib/images/downscale.ts`
ziehen und von beiden Stellen nutzen (~15 Zeilen gespart, ein Aufrufer umgeschrieben). Die gemeinsame
Signatur ist `downscaleToWebp(bytes: ArrayBuffer, mime: string, maxEdge: number)`; die File-Auswahl
liest ihre `File` vor dem Aufruf als `ArrayBuffer`. Freigabe durch den
Operator mit diesem Plan.

**Rationale**: Der native Input braucht weder `fs:allow-read-file` (heute nur `read-text-file`,
`capabilities/default.json`) noch den Dialog-Scope.

## R8 — Agenten und Hintergrund

**Decision**: Abweichend vom Entwurf keine Aktion `setBackground(pfad)`. Agenten öffnen Erscheinungsbild
mit `wm.app.open { at: '/general/appearance' }`; neu ist nur `settings.appearance.removeBackground`
(`effect: 'write'`). `settings.general.setLanguage` (`effect: 'write'`) wie geplant.

**Rationale**: Der native File-Input liefert der Ansicht bereits die vom Nutzer ausgewählte `File`;
ein Agent übergibt keinen Dateipfad. Dadurch braucht die Hintergrundauswahl keinen breiten
Tauri-Dateiscope, und FR-019 kann die Auswahl bei der Nutzeransicht belassen.

## R9 — Tests

- `scripts/check-settings.ts` (`pnpm check:settings`, `node --test`): `LOCATION_PATHS` (`:41-64`),
  Kategorien-Test (`:98-116`), Übersicht `general`, Suche (`:301` → `/general/appearance`), i18n-Schlüssel
  de/en.
- Rust: `src-tauri/tests/vault_passphrase_change.rs` nach `tests/vault_single_session.rs` (`create`/`open`,
  `mock_app()`, `XDG_DATA_HOME`-Tempdir): ändern → schließen → altes Passwort `WrongPassphrase`, neues
  öffnet, Daten vorhanden; falsches aktuelles Passwort ändert nichts; zu kurz / gleich → `WeakPassphrase`.
  `passphrase_tests.rs`: `validate_new`.
- E2E (`scripts/e2e/scenarios/`): `settings-categories`, `settings-color-scheme`, `appearance-basic`,
  `settings-narrow-window`, `settings-search`, `settings-deep-links` auf die neuen Pfade. Laufen nur auf
  Arch (Memory „host bridge Arch-only“); auf diesem Rechner `nuxt build` + `check:settings`.
