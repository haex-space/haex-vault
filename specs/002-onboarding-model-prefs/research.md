# Phase 0 Research: Onboarding-Härtung und Modellwahl-Persistenz

Alle offenen Klärfragen aus dem Spec sind in der Clarify-Session vom 2026-09-10 aufgelöst. Diese Phase konsolidiert die Recherche zu Technologie-Wahlen und Integrationsmustern, damit Phase 1 (data-model + contracts) darauf aufsetzen kann.

## Entscheidung 1: OS-Hostname-Detection

**Decision**: `sysinfo::System::host_name()` verwenden.

**Rationale**: `sysinfo 0.32` ist bereits im Backend als Cargo-Dependency für die VRAM/RAM-Erkennung eingebunden (`src-tauri/src/hardware/mod.rs`). `System::host_name()` liefert `Option<String>` — perfekt für den Placeholder-Fallback ("wenn Hostname ermittelbar, sonst 'Neues Gerät'" aus FR-003).

**Alternatives considered**:

- `hostname` crate: dedizierte Single-Purpose-Crate, ~10 KB, aber redundant zu bestehender `sysinfo`-Dep.
- `std::env::var("HOSTNAME")` bzw. `COMPUTERNAME` (Windows): unzuverlässig, plattform-abhängig, nicht immer gesetzt.
- Tauri `AppHandle::config().tauri.bundle.identifier`: nicht der Gerätehostname, sondern eine App-ID.

## Entscheidung 2: haex-crdt-Kompatibilität der neuen Migrationen

**Decision**: Migrations 0011 und 0012 werden über den bestehenden `HaexMigrationSource`-Mechanismus geliefert. `CREATE TABLE preferences (..., PRIMARY KEY (vault_device_uuid, key), FOREIGN KEY (vault_device_uuid) REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE)` als 0011; `DROP TABLE device_downloaded_models_no_sync` als 0012.

**Rationale**:

- haex-crdt's `CrdtTransformer` verarbeitet `ALTER TABLE` (bereits bewiesen in Migration 0009 `models.tokenizer_repo`) und `DROP TABLE` (unterstützt in `src/db/core/extract.rs:226` und `migrations/compat.rs:184`).
- `CREATE TABLE` mit `FOREIGN KEY`-Klausel geht durch: haex-crdt's Transformer installiert seine CRDT-Metadaten-Spalten (`haex_hlc_no_sync`, `haex_column_hlcs_no_sync`, `haex_column_sigs_no_sync`) additiv, ohne die FK-Deklaration zu berühren.
- `PRAGMA foreign_keys` ist beim App-Betrieb an (siehe `haex-crdt/src/db/core/init.rs:61`); wird während CRDT-Sync-Apply per RAII-Guard temporär off — dadurch scheitert ein Payload mit potenzieller Referential-Race-Condition nicht mitten im Apply. `apply_remote_changes` staged den Payload, wendet `known_devices`-Elternzeilen unabhängig von der Eingangsreihenfolge vor `preferences`-Kindzeilen an und führt vor dem Commit `PRAGMA foreign_key_check` aus. Der Integrationstest deckt sowohl child-before-parent als auch parent-before-child ab; ein nach dem Staging fehlendes Elternobjekt wird verworfen statt als Orphan committed.

**Alternatives considered**:

- Migration ohne FK-Deklaration, nur Namenskonvention: verwirft die Hard-FK-Safety aus ADR-0001, die genau wegen bestehender Bug-Vorlage (`device_downloaded_models_no_sync` ohne device_id) gefordert wurde.
- `WITHOUT ROWID`-Tabelle für strikte NOT NULL auf PK: unklare Kompatibilität mit haex-crdt's row_pks-Serialisierung; Composite-PK mit NOT NULL-Spalten reicht.

## Entscheidung 3: Bootstrap-Sentinel-Row-Insertion

**Decision**: In `HaexBootstrap::bootstrap` VOR dem `installation_uuid`-Lookup ein `INSERT OR IGNORE INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) VALUES (nil_uuid, nil_uuid, NULL, 0)`. Läuft bei JEDEM Open (Genesis + Adoption + normaler Resume). Idempotent per `INSERT OR IGNORE`.

**Rationale**:

- Der bestehende `HaexBootstrap`-Impl (`src-tauri/src/identity/bootstrap.rs`) läuft in einer eigenen Transaktion vor HLC-Init und ist der einzige atomare Kontext, in dem wir garantieren können, dass die Sentinel-Zeile vor dem ersten Write in `preferences` existiert.
- CRDT-Sync-Kollision: wenn zwei Geräte parallel den Sentinel schreiben, sind die Werte identisch (nil, nil, NULL, 0) — LWW resolviert conflict-free.
- HLC-basiert wird der zweite Insert (auf einem Gerät, das den ersten sync-empfangen hat) durch `INSERT OR IGNORE` supprimiert.

**Alternatives considered**:

- Sentinel nur bei Genesis inserten: müsste unterscheiden zwischen "first ever bootstrap on any device" vs "adoption". Bootstrap kennt diese Unterscheidung nicht atomar, weil sync-empfangene Rows nicht klar von genesis-erstellten unterscheidbar sind. `INSERT OR IGNORE` bei jedem Open ist einfacher und robuster.
- Sentinel als separate Tabelle (`vault_scope_marker`): schafft eine zusätzliche Struktur nur um eine einzige nil-UUID zu tragen. Simpler direkt in `known_devices` mit dokumentierter Konvention (siehe ADR-0001).

## Entscheidung 4: Filesystem-Scan für `list_installed_models`

**Decision**: `tokio::fs::read_dir` unter `<AppLocalData>/models/`. Für jeden Sub-Dir wird der Slug als Modell-ID interpretiert, aber nur wenn eine vollständige reguläre `.gguf`-Datei vorhanden ist. Download und Import schreiben temporär und veröffentlichen die Datei per atomarem Rename; bei mehreren vollständigen Dateien wird die lexikografisch kleinste UTF-8-Datei nach Dateiname als kanonische Datei gewählt. Modell-Metadaten kommen aus einem Lookup in der `models`-Tabelle (per `models_store::get_model(&id)`). Sub-Dirs ohne passende `models`-Row oder ohne verfügbare Modelldatei werden übersprungen; ein fehlendes Root-Verzeichnis ergibt eine leere Liste.

**Rationale**:

- Async-Konsistenz mit anderen Tauri-Command-Pfaden.
- Kleine Katalog-Größe (~5-20 Modelle), Read-Dir ist im Millisekunden-Bereich.
- Der `models`-Row-Lookup filtert Ghost-Dirs: wenn jemand manuell ein Verzeichnis anlegt, das nicht via `download_model_from_hf` oder `import_model_from_file` registriert wurde, wird es nicht als Installed gezählt (Sicherheits-/Konsistenz-Schutz).
- Atomarer Rename verhindert, dass ein laufender Download als installiert erscheint; die lexikografische Auswahl verhindert, dass wechselnde `read_dir`-Reihenfolgen einen anderen `relativePath` liefern. Die gemeinsame Selector-Routine wird auch vom lokalen Loader verwendet, damit Discovery und Laden niemals unterschiedliche Dateien wählen.
- Verlust von `sha256`/`verified_at`: kein UI zeigt diese Werte aktiv; die praktische Integritätsprüfung ist der `LocalModel::load`-Aufruf selbst (mistralrs schlägt bei korrupter GGUF fehl).

**Alternatives considered**:

- Sync-Scan mit `std::fs::read_dir`: verringert Async-Symmetrie mit dem Rest der Command-Pfade; kaum Performance-Unterschied bei dieser Größenordnung.
- `.sha256`-Sidecar-Datei pro GGUF: erhält den Integritätsprüfungspfad; kostet extra Datei pro Modell und wird nicht durch ein UI-Feature validiert. Zurückgestellt bis konkreter Bedarf.
- Modell-DB-Registry beibehalten und Sync-Truth mit File-Scan reconcilen: doppelter Buchführungsaufwand, genau das Problem das die Streichung von `device_downloaded_models_no_sync` löst.

## Entscheidung 5: FAB-Komponente und Workspace-Landing-Layout

**Decision**: FAB als eigene Komponente `src/components/workspace/ChatFab.vue`; nutzt `UiButton` aus dem existierenden `@haex/ui`-Nuxt-Layer als Basis, mit `class="fixed bottom-6 right-6 rounded-full shadow-lg h-14 w-14"` (Tailwind). Workspace-Landing (`src/pages/workspace/[instance].vue`) ist ein minimales Layout: Instanzname als Überschrift, Header mit Settings-Link (Zahnrad-Icon), Chat-FAB rechts unten.

**Rationale**:

- `@haex/ui`-Layer wird via GitHub-extends geladen (pinned rev `634d621`, siehe `nuxt.config.ts`). Aktueller Snapshot enthält keine dedizierte FAB-Komponente (nur `UiButton`, `UiDrawerModal`, `UiInputPassword` u.a. die wir bisher nutzten).
- Eigene FAB-Komponente in haex-vault zu haben statt Upstream-Beitrag in `@haex/ui` ist für den ersten Wurf schneller und blockt kein externes Repo. Wenn andere haex-Apps auch FAB brauchen, kann später ein Upstream-PR die Komponente in die Shared-Layer heben.
- FAB-Verhalten in diesem Feature: Klick öffnet den Chat als eigene Route (`/chat/[instance]`) — Overlay/Modal ist explizit in FR-018b als "Plan-Detail, nicht Spec-Detail" offen gelassen; Route ist die einfachste Variante ohne Modal-State-Handling in der Landing.

**Alternatives considered**:

- Upstream-PR gegen `@haex/ui` für FAB: verzögert dieses Feature auf einen externen Review-Zyklus; wenn später gebraucht, ist die Extraktion aus haex-vault trivial.
- Chat als Modal/Overlay statt Route: erfordert State-Handling für "Chat sichtbar-ja/nein" in Workspace, macht Layouts komplexer (welcher Container hält Chat-State?); Route hat freies Back-Button-Verhalten und bekannte Nuxt-Route-Guard-Semantik.

## Entscheidung 6: Route-Guarding für Onboarding

**Decision**: Nuxt-Middleware `src/middleware/onboarded.ts` (auto-import), Named-Middleware auf den drei neuen Routen (`workspace`, `settings`, `chat`) plus der bestehenden `chat`-Route. Middleware ruft `current_device_info` und routet auf `/onboarding/[instance]`, wenn `alias === null`. Die `/onboarding/[instance]`-Route selbst nutzt diese Middleware NICHT (sonst Endlos-Redirect).

**Rationale**:

- Nuxt-4-Standard-Pattern; funktioniert client-side im SPA-Modus (`ssr: false`) — passt zum bestehenden Nuxt-Setup.
- Middleware pro Route (Named) hält den Redirect-Kontrollfluss lokal und ohne globalen Side-Effect.
- Alias-Nullness ist der einzige Trigger (nicht "gibt es Modelle" — der Wizard ist Pflicht bei jedem Erst-Open, unabhängig von Modell-Verfügbarkeit).

**Alternatives considered**:

- Global-Middleware mit Ausnahme-Liste: fragile Konfiguration, schwer zu erweitern.
- Route-Guard im Composable innerhalb jeder Page mit `onMounted → navigateTo`: sichtbares Flackern zwischen Ziel-Route und Redirect; Middleware verhindert das Rendering.

## Entscheidung 7: Frontend-Store für Route-Ziel nach Vault-Open

**Decision**: Bestehende `pages/index.vue` bleibt Landing für nicht-authentifizierte Sitzungen; nach erfolgreichem `open_instance` navigiert der bestehende Unlock-Flow (bisher zu `/chat/[name]`) neu zu `/workspace/[name]`. Die Onboarded-Middleware fängt dann bei nicht-gesetztem Alias ab und leitet auf `/onboarding/[name]` um.

**Rationale**:

- Minimaler Eingriff: der Unlock-Flow wird nur um sein Redirect-Ziel geändert, nicht um Business-Logik erweitert.
- Onboarded-Middleware ist der einzig autoritative Ort für die "muss zur Onboarding-Seite"-Entscheidung — kein Zweit-Check in Unlock, kein Zweit-Check in Landing.

**Alternatives considered**:

- Unlock-Flow selbst prüft `current_device_info` und entscheidet Redirect: dupliziert die Middleware-Logik.
- Direkt zu `/onboarding` weiterleiten wenn wir bei Vault-Open wissen, dass Alias null ist: braucht Backend-Round-Trip vor dem Redirect, verzögert Landing-Anzeige; Middleware macht das nach der ersten Landing-Route (Nutzer sieht Landing kurz und wird dann sofort umgeleitet — visuell unmerklich, weil Landing nur ein Stub ist).

## Entscheidung 8: Loading-UX pro Modell-Load-Kategorie

**Decision**: Ein neuer Tauri-Event `model-load-progress` mit **strukturiertem Payload** `{ modelId, modelName, phase: 'connecting' | 'loading' | 'cuda-jit-warmup' | 'ready', providerName?: string }` wird vom Backend während des Session-Resolver-Loads emittiert. Frontend zeigt eine sichtbare Ladepanel-Komponente und übersetzt die Beschriftung via `@nuxtjs/i18n`-Keys `chat.loading.<phase>` mit Interpolation der Payload-Parameter. **Backend liefert keine lokalisierten Strings.**

**Rationale**:

- Etappe-0-Findung #4 verlangt eigene UX für den CUDA-JIT-Kalt-Load.
- Backend kann die "Kalt vs Warm"-Unterscheidung machen: prüft, ob `~/.nv/ComputeCache/` einen Eintrag für die aktuelle Modell-Signatur enthält. Alternative: erste Load-Latenz messen und über Threshold entscheiden — weniger genau, aber einfacher.
- API-Key-Modelle: haben keine "Load"-Phase, nur eine "connecting"-Phase mit sofortigem Ready.
- **i18n-Boundary im Backend**: die App nutzt `@nuxtjs/i18n` v10 mit `de` + `en` (FR-020). Lokalisierung ist Frontend-Verantwortung; das Backend spricht mit Enum-Werten und Parametern, damit Sprach-Switch keinen Backend-Rebuild braucht und beide Sprachen konsistent aus derselben Locale-Datei gepflegt werden.
- Auf non-CUDA-Builds (CPU/Metal) wird `cuda-jit-warmup` NICHT emittiert; stattdessen fällt der Backend-Klassifizierer auf `loading` zurück. Frontend zeigt entsprechend den warmen Ladehinweis.

**Alternatives considered**:

- Backend liefert deutschen Text direkt: brechen mit i18n-Regel; jeder Sprach-Support-Ausbau würde Backend-Änderungen erfordern; deutsche/englische Konsistenz driftet auseinander.
- Nur einen einzigen "loading"-Zustand ohne Kontext-Kategorie: verletzt Etappe-0-Findung #4.
- Kalt-vs-Warm nicht unterscheiden, immer "GPU-Optimierung"-Text zeigen: nervt bei warmen Loads (~4s), wo der Text irreführend wäre.

## Entscheidung 9: Preference-Value-Persistence-Details

**Decision**: `set_pref(scope, key, value)` und `clear_pref(scope, key)` als separate Commands. Composable exposes `usePreferences().setPref({ scope, key, value })` und `.clearPref({ scope, key })`. Werte werden als reine TEXT ohne JSON-Encoding gespeichert (Spec-Entscheidung 4a in Grill). Der `chat.default_model_id`-Wert ist die Composite-ID (bei api_key `<provider_uuid>:<remote>`, bei local Catalog-ID) — dieselbe Semantik wie im Chat-Command.

**Rationale**:

- `clear_pref` ist expliziter als "set with empty string" oder "set with NULL" — Nutzer-Intent "vergessen" ist eine eigene Aktion, kein Sonderfall von "setzen".
- Kein JSON-Encoding: Grill-Ergebnis 4a; einfacher, schneller, weniger Fehlerquelle.
- Composite-ID direkt speichern: keine Übersetzungsschicht nötig; der bestehende `load_model`-Pfad interpretiert schon Composite vs Katalog.

**Alternatives considered**:

- Ein einzelner `upsert_pref`-Command mit `value: Option<String>` und `None` = clear: verwischt Set/Clear-Semantik.
- `default_model_id` als Kombination aus separaten Feldern `provider_id` + `model_remote_id`: verletzt "TEXT-only" und dupliziert die Composite-Konvention.

## Entscheidung 10: Test-Strategie

**Decision**:

- **Backend-Unit-Tests**: `src-tauri/src/storage/preferences_tests.rs` für Storage-Roundtrip + FK-Cascade + Sentinel-Idempotency + Resolver-Chain-Priorisierung.
- **Backend-Integration-Test**: `src-tauri/tests/preferences_roundtrip.rs` für einen End-to-End-Bootstrap-mit-Sentinel-Test plus Cross-Session-Preferences-Persistenz.
- **Sync-FK-Apply-Order-Test**: `apply_remote_changes` mit Child-before-Parent und Parent-before-Child Payloads; beide Reihenfolgen müssen vor dem Commit einen gültigen FK-Zustand herstellen.
- **Filesystem-Scan-Test**: eigener Test in `src-tauri/src/models/commands.rs` oder als Modul-Test mit tmp-Verzeichnis.
- **Frontend**: keine automatisierten Tests (Playwright ist nicht eingerichtet, siehe Etappe-1-Follow-Up-Note); manuelle Verifikation via `pnpm tauri dev` mit dokumentierten Steps in [quickstart.md](quickstart.md).

**Rationale**:

- Bestehendes Test-Muster in haex-vault ist rein Rust (Unit + Integration, keine Frontend-Tests). Kein neuer Test-Stack in diesem Feature.
- Der spannendste Testfall ist die FK-Cascade auf ON DELETE — Delete einer `known_devices`-Zeile muss alle `preferences`-Zeilen mit diesem `vault_device_uuid` mit-entfernen. Dieser Test beweist auch, dass haex-crdt's Sync-Apply die FK-Deklaration respektiert wenn er sie temporär abschaltet.

**Alternatives considered**:

- Playwright einführen: großer Scope-Add, nicht gerechtfertigt für ein Feature dieser Größe. Bleibt in der Etappe-1-Follow-Up-Liste (siehe [haex-status]-Memory).
- E2E-Test via cargo mit gestartetem Tauri-Prozess: Overkill, keine bestehende Infrastruktur.

---

**Ergebnis**: Alle NEEDS-CLARIFICATION-Marker sind aufgelöst (es gab keine im Spec, aber die Recherche-Entscheidungen sind hier explizit gemacht). Bereit für Phase 1.
