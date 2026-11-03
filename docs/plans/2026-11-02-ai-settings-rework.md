# KI-Einstellungen überarbeiten – Umsetzungsplan

> **For Claude:** REQUIRED SUB-SKILL: Use executing-plans (oder subagent-driven-development) to implement this plan task-by-task.

**Goal:** Einstellungen → KI bekommen die Einträge "Anbieter" und "Lokale Modelle", das Standardmodell entfällt, und das aktive lokale Modell lebt in einem eigenen Backend-Slot, unabhängig von der Chat-Sitzung.

**Architecture:** Vier Stufen, jede ein eigener PR gegen `main`. Stufe 1 ist reine UI-Zusammenlegung (Registry, Routen, Views, i18n). Stufe 2 entfernt das Standardmodell (Frontend, Onboarding, Backend-Kette). Stufe 3 führt `ChatState.local_model` ein (Aktivieren, Entladen, Preload). Stufe 4 verdrahtet Frontend und Chat-Auswahl damit. Design: [2026-11-02-ai-settings-rework-design.md](2026-11-02-ai-settings-rework-design.md).

**Tech Stack:** Nuxt 3 / Vue 3 / Pinia, Tauri 2 (Rust), `node --test` Check-Skripte, i18n `de.json` und `en.json`.

## Vorweg

- **Stufen:** Stufe 1 und 2 sind unten bis auf Schrittebene ausgearbeitet. Stufe 3 und 4 sind als Aufgaben mit Dateien, Signaturen und Kernlogik beschrieben. Ihre Feinplanung (Tests, genaue Zeilen) kommt, wenn Stufe 2 gemergt ist, weil sie von dort aus gelesenen Code abhängt (Store, Chat-Menü).
- **Branches:** Stufe 1 läuft im Worktree `.worktrees/ai-settings-rework` (Branch `ai-settings-rework`). Jede weitere Stufe bekommt einen neuen Worktree von `origin/main`, nach dem Merge der vorigen: `git worktree add .worktrees/<name> -b <name> --no-track origin/main`.
- **Commits:** Englisch, kurz, ohne Claude-Zeile und ohne `Co-Authored-By` (globale CLAUDE.md des Users gilt).
- **Rust bauen:** Nur im Speicher-Scope und ohne die NDK-Variablen: `env -u CC -u CXX -u AR systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 cargo test --manifest-path src-tauri/Cargo.toml --lib -j 1 -- <filter>`. Exit 137 heißt Speicherdruck, nicht Testfehler. Rust-Tests liegen in `*_tests.rs` neben dem Modul, nie inline.
- **Vor jedem Push:** `pnpm check:settings && pnpm check:chat-state && pnpm check:chat-model-search && pnpm check:templates && pnpm typecheck:scripts && pnpm lint`. Nach Änderungen an Composables alle `check:*`.
- **Baseline am 2026-11-02:** `check:settings` 32, `check:chat-state` 81, `check:chat-model-search` 7 grün; 319 Templates kompilieren; `typecheck:scripts` sauber.
- **E2E** läuft auf PRs nicht. Vor dem Merge einer UI-Stufe: `gh workflow run ci.yml --ref <branch>`.

---

# Stufe 1: Anbieter und Lokale Modelle (nur UI)

Ändert keine Backend-Semantik. "Laden" in den lokalen Modellen heißt bis Stufe 4 weiter "Laden" und ruft weiter `chat.model.select`. `ai.default` bleibt bis Stufe 2.

### Task 1: Registry und Routen

**Files:**

- Modify: `src/lib/settings/registry.ts:167-206`
- Modify: `src/components/wm/appRoutes.ts:60-100`
- Test: `scripts/check-settings.ts:46-72, 150-200`

**Step 1: Test anpassen (rot).** In `LOCATION_PATHS` die Zeilen für `ai.servers`, `ai.servers.server`, `ai.installed`, `ai.download`, `ai.download.search`, `ai.download.repo` ersetzen durch:

```ts
  'ai.providers': '/ai/providers',
  'ai.providers.provider': '/ai/providers/0b9f1c2e-5d6a-4e7b-8c9d-0a1b2c3d4e5f',
  'ai.local': '/ai/local',
  'ai.local.search': '/ai/local/search',
  'ai.local.repo': '/ai/local/repo/Qwen/Qwen2.5-GGUF',
```

(die alte Zeile `'ai.providers': '/ai/providers'` darf nur einmal stehen). Im Test "overview rows of general (spec 042) and ai" die Liste ersetzen durch:

```ts
assert.deepEqual(rows('ai'), [
  'ai.default',
  'ai.providers',
  'ai.local',
  'ai.speech',
  'ai.speech.voice',
  'ai.speech.tones',
  'ai.autonomy',
  'ai.denyRules',
  'ai.files',
  'ai.agents',
  'ai.browser',
])
```

Im Test "the ai overview groups its rows by task" die ersten beiden Gruppen:

```ts
      ['chat', ['ai.default', 'ai.providers']],
      ['local', ['ai.local']],
```

**Step 2:** `pnpm check:settings` → FAIL (die Registry kennt `ai.local` nicht).

**Step 3: Registry.** In `registry.ts` den Block von `subView('ai.providers', …)` bis einschließlich `subView('ai.download.repo', …)` ersetzen durch:

```ts
  // "Anbieter": Claude, Codex and OpenAI-compatible endpoints (spec 055 US5) on one page; an
  // endpoint's page is reached from its row, never searched for. An agent opens it with `wm.app.open`.
  subView('ai.providers', 'ai/providers', 'ai', {
    icon: 'lucide:plug',
    group: 'chat',
    overviewRow: true,
  }),
  subView('ai.providers.provider', 'ai/providers/:providerId', 'ai.providers'),
  // "Lokale Modelle": the models on this device and the way to get more.
  subView('ai.local', 'ai/local', 'ai', {
    icon: 'lucide:hard-drive',
    group: 'local',
    overviewRow: true,
  }),
  subView('ai.local.search', 'ai/local/search', 'ai.local', {
    icon: 'lucide:search',
    row: true,
  }),
  subView('ai.local.repo', 'ai/local/repo/:owner/:name', 'ai.local.search'),
```

Den Kommentar `// Reached from "Standard-Modell" and the chat; …` über dem alten `ai.providers` entfernen; `ai.default` bleibt unverändert davor.

**Step 4: Routen.** In `appRoutes.ts` die Einträge `'ai.installed'`, `'ai.download'`, `'ai.download.search'`, `'ai.download.repo'`, `'ai.providers'`, `'ai.servers'`, `'ai.servers.server'` ersetzen durch:

```ts
  'ai.local': defineAsyncComponent(
    () => import('~/components/settings/LocalModelsView.vue'),
  ),
  'ai.local.search': defineAsyncComponent(
    () => import('~/components/models/HuggingFaceSearch.vue'),
  ),
  'ai.local.repo': defineAsyncComponent(
    () => import('~/components/models/HuggingFaceFilePicker.vue'),
  ),
  'ai.providers': defineAsyncComponent(
    () => import('~/components/settings/ProvidersView.vue'),
  ),
  'ai.providers.provider': defineAsyncComponent(
    () => import('~/components/settings/ModelServerView.vue'),
  ),
```

**Step 5:** Tests laufen erst nach Task 2 bis 5 grün (fehlende Komponenten). Hier nur committen, wenn `pnpm check:settings` grün ist; sonst Commit am Ende von Task 5.

### Task 2: ProvidersView

**Files:**

- Create: `src/components/settings/ProvidersView.vue`
- Modify: `src/components/settings/ModelServersView.vue:81, 118`
- Modify: `src/components/settings/ModelServerView.vue:125`

**Step 1: Pfade umziehen.** `` `/ai/servers/${added.providerId}` `` → `` `/ai/providers/${added.providerId}` `` (ModelServersView Zeile 81), ``:to="`/ai/servers/${server.id}`"`` → ``:to="`/ai/providers/${server.id}`"`` (Zeile 118), `router.push('/ai/servers')` → `router.push('/ai/providers')` (ModelServerView Zeile 125).

**Step 2: View anlegen.**

```vue
<script setup lang="ts">
/**
 * "Anbieter": the subscriptions (Claude, Codex) and the OpenAI-compatible endpoints (OpenRouter,
 * a GPU machine, a school server) as chat backends, on one page. Both parts keep their own
 * component and their own data; this view only stacks them under a heading each.
 */
const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col gap-6" data-testid="settings-providers">
    <section class="flex flex-col gap-3">
      <h2 class="px-1 text-sm font-medium text-muted-foreground">
        {{ t('settings.providers.subscriptions') }}
      </h2>
      <SettingsConnectDelegateProvider />
    </section>
    <section class="flex flex-col gap-3">
      <h2 class="px-1 text-sm font-medium text-muted-foreground">
        {{ t('settings.providers.endpoints') }}
      </h2>
      <SettingsModelServersView />
    </section>
  </div>
</template>
```

Der Komponentenname folgt Nuxts Auto-Import (`components/settings/X.vue` → `SettingsX`). Prüfen: `grep -rn "SettingsGroup\b" src/components/settings/Group.vue src/components/settings/OverviewView.vue | head -3` zeigt, ob der Präfix `Settings` gilt (`SettingsGroup` aus `settings/Group.vue` bestätigt es).

**Step 3:** `pnpm check:templates` → `319 … compile` (mit neuer Datei 320).

### Task 3: LocalModelsView und die beiden Listen

**Files:**

- Create: `src/components/settings/LocalModelsView.vue`
- Modify: `src/components/settings/InstalledModels.vue:195-203` (leerer Zustand) und Kopfkommentar
- Modify: `src/components/settings/DownloadModels.vue:42-44, 82, 116-160`
- Modify: `src/components/models/HuggingFaceSearch.vue:3, 200`
- Modify: `src/components/models/HuggingFaceFilePicker.vue:3, 133, 174`

**Step 1: View anlegen.**

```vue
<script setup lang="ts">
/**
 * "Lokale Modelle": the models on this device (load, update, delete) above the way to get more
 * (HuggingFace search, running downloads, the recommended models not installed yet). A finished
 * download moves from the lower list into the upper one.
 */
const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col gap-6" data-testid="settings-local-models">
    <section class="flex flex-col gap-3">
      <h2 class="px-1 text-sm font-medium text-muted-foreground">
        {{ t('settings.localModels.onDevice') }}
      </h2>
      <SettingsInstalledModels />
    </section>
    <section class="flex flex-col gap-3">
      <h2 class="px-1 text-sm font-medium text-muted-foreground">
        {{ t('settings.localModels.download') }}
      </h2>
      <SettingsDownloadModels />
    </section>
  </div>
</template>
```

**Step 2: Leerer Zustand der oberen Liste.** In `InstalledModels.vue` den `SettingsGroup`-Block mit `v-else-if="installed.length === 0"` (Zeilen 195–204) ersetzen durch einen Absatz; die Zeile zur Download-Seite entfällt, weil der Download darunter auf derselben Seite steht:

```vue
<p
  v-else-if="installed.length === 0"
  class="px-1 text-sm text-muted-foreground"
  data-testid="settings-installed-empty"
>
      {{ t('models.installed.empty') }}
    </p>
```

**Step 3: Katalogliste zeigt nur nicht installierte Modelle.** In `DownloadModels.vue`: `isInstalled` bleibt, dazu ein Computed, und die Schleife und der Button ändern sich:

```ts
const notInstalled = computed(() =>
  catalogEntries.value.filter((entry) => !isInstalled(entry)),
)
```

Im Template `v-if="catalogEntries.length > 0"` → `v-if="notInstalled.length > 0"`, `v-for="entry in catalogEntries"` → `v-for="entry in notInstalled"`, am Button `:disabled="isInstalled(entry)"` entfernen und den Text auf `{{ t('models.filePicker.install') }}` verkürzen. Hinweis: Während eines Downloads bleibt der Eintrag in der Liste (noch nicht installiert), mit Fortschrittsbalken.

**Step 4: Pfade.** `to="/ai/download/search"` → `to="/ai/local/search"` (DownloadModels), `` `/ai/download/repo/${…}` `` → `` `/ai/local/repo/${…}` `` (HuggingFaceSearch:200), `router.push('/ai/installed')` → `router.push('/ai/local')` und `to="/ai/download/search"` → `to="/ai/local/search"` (HuggingFaceFilePicker:133, 174). Die Kopfkommentare der vier Dateien, die `/ai/installed`, `/ai/download` nennen, auf `/ai/local` umstellen.

**Step 5:** `grep -rn "ai/download\\|ai/installed\\|ai/servers" src --include=*.vue --include=*.ts` should have only the expected old link in `DefaultModelSetting.vue:169` at this stage. Task 4, Step 3 updates that link; rerun the check then and expect no hits except `ai/default` references.

### Task 4: i18n

**Files:**

- Modify: `src/i18n/locales/de.json` (um Zeile 645–700 und `settings.modelServers` bei 1442)
- Modify: `src/i18n/locales/en.json` (gleiche Struktur)

**Step 1: de.json.** Unter `settings.locations.ai` die Blöcke `providers`, `servers`, `installed`, `download` ersetzen durch:

```json
        "providers": {
          "title": "Anbieter",
          "description": "Claude, Codex und OpenAI-kompatible Dienste als Chat-Backend verbinden.",
          "keywords": "Anbieter Abo verbinden Claude Code Codex CLI OpenAI kompatibel OpenRouter Abacus Ollama vLLM llama.cpp Server Endpunkt Schulserver Adresse Schlüssel API Bilder Aufwand",
          "provider": {
            "title": "Endpunkt"
          }
        },
        "local": {
          "title": "Lokale Modelle",
          "description": "Heruntergeladene Modelle verwalten und neue herunterladen.",
          "keywords": "lokal installiert laden aktualisieren Update löschen entfernen herunterladen Download empfohlen HuggingFace GGUF",
          "search": {
            "title": "Auf HuggingFace suchen",
            "description": "GGUF-Modelle auf HuggingFace finden.",
            "keywords": "HuggingFace suchen GGUF Quantisierung"
          },
          "repo": {
            "title": "{owner}/{name}"
          }
        },
```

Die bisherigen Werte von `download.title`/`download.description` entfallen; `download.search`/`download.repo` sind nach `local` gewandert. Direkt neben `settings.modelServers` (Zeile ~1442) ergänzen:

```json
    "providers": {
      "subscriptions": "Abos",
      "endpoints": "Endpunkte"
    },
    "localModels": {
      "onDevice": "Auf diesem Gerät",
      "download": "Herunterladen"
    },
```

Falls `settings.providers` schon existiert: Schlüssel dort ergänzen, nicht doppeln (`grep -n '"providers"' src/i18n/locales/de.json`). Im Adressfeld-Hinweis `settings.modelServers.addressHint` Beispiele ergänzen: `Beispiele: https://openrouter.ai/api/v1 oder http://192.168.1.20:11434/v1`.

**Step 2: en.json.** Gleiche Struktur, englische Texte: "Providers", "Connect Claude, Codex and OpenAI-compatible services as chat backends.", Keywords entsprechend inkl. OpenRouter/Abacus; "Local models", "Manage downloaded models and download new ones."; "Search on HuggingFace" / "Find GGUF models on HuggingFace."; `subscriptions` "Subscriptions", `endpoints` "Endpoints", `onDevice` "On this device", `download` "Download".

**Step 3:** `grep -rn "locations.ai.\(installed\|download\|servers\)" src | grep -v "locations.ai.download.search"` → alle Treffer auf die neuen Schlüssel umstellen (`locations.ai.local…`, `locations.ai.providers…`). `DefaultModelSetting.vue` zeigt mit `settings.locations.ai.download.*` auf `…ai.local.*` und `to="/ai/download"` auf `to="/ai/local"`.

**Step 4:** `pnpm check:settings && pnpm check:templates && pnpm typecheck` → grün.

### Task 5: E2E-Szenarien nachziehen und committen

**Files:**

- Modify: `scripts/e2e/scenarios/models-download-confirm.test.ts:51`
- Modify: `scripts/e2e/scenarios/settings-header-back.test.ts:11`
- Modify: `scripts/e2e/scenarios/settings-categories.test.ts`, `settings-deep-links.test.ts`, `settings-search.test.ts`

**Step 1:** `grep -n "ai/download\|ai/installed\|ai/servers\|ai\.download\|ai\.installed\|ai\.servers\|Installierte Modelle\|Modelle herunterladen\|Abo verbinden\|Modell-Server" scripts/e2e -r` und jeden Treffer auf die neuen Pfade und Titel setzen: `/ai/local`, `/ai/local/repo/…`, `/ai/providers`, "Lokale Modelle", "Anbieter". Der Test in `settings-header-back` erwartet das Zurück von der Repo-Seite; dort stand "Installierte Modelle → Modelle herunterladen". Neu: `/ai/local` → `/ai/local/search` → `/ai/local/repo/…`.

**Step 2:** `pnpm check:e2e-lib && pnpm typecheck:scripts` grün.

**Step 3: Commit und PR.**

```bash
git add -A
git commit -m "feat(settings): merge providers and local models views"
```

Dann `gh workflow run ci.yml --ref ai-settings-rework`, PR mit `gh pr create` (bei GraphQL-Fehler: `gh api -X POST …/pulls`; Account `haex-space` für PRs gegen `haex-space/*`, danach zurück).

---

# Stufe 2: Standardmodell entfernen

Neuer Worktree `.worktrees/ai-settings-no-default` von `origin/main` nach dem Merge von Stufe 1.

### Task 6: Backend-Kette verkürzen (Tests zuerst)

**Files:**

- Modify: `src-tauri/src/chat/default_model_tests.rs`
- Modify: `src-tauri/src/chat/default_model.rs:31-190, 218-260`
- Modify: `src-tauri/src/model_jobs/preflight.rs:25, 53-54, 139-148, 227-250`
- Modify: `src-tauri/src/extensions/bridge/model_jobs_run_tests.rs:14, 165`
- Modify: `src-tauri/src/storage/maintenance.rs:17, 71`
- Test: `src-tauri/src/chat/default_model_tests.rs`

**Step 1: Tests umschreiben (rot).** `choose_model` bekommt `[Option<String>; 2]` (Session, zuletzt gewählt). Ersetzen:

```rust
fn a_continued_session_keeps_its_model() {
    let chosen = choose_model(
        [some("claude:sonnet"), some("qwen3-4b")],
        ids(&["qwen3-4b"]),
        ids(&["claude:sonnet"]),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("claude:sonnet"));
    assert_eq!(chosen.source, ResolveSource::Session);
}

#[test]
fn a_session_whose_model_is_gone_falls_back_to_the_last_pick() {
    let chosen = choose_model(
        [some("deleted-model"), some("qwen3-4b")],
        ids(&["qwen3-4b"]),
        Vec::new(),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("qwen3-4b"));
    assert_eq!(chosen.source, ResolveSource::LastActive);
}

#[test]
fn a_new_session_starts_with_the_last_pick() {
    let chosen = choose_model(
        [None, some("codex:gpt")],
        ids(&["qwen3-4b"]),
        ids(&["codex:gpt"]),
    );
    assert_eq!(chosen.model_id.as_deref(), Some("codex:gpt"));
    assert_eq!(chosen.source, ResolveSource::LastActive);
}

#[test]
fn without_a_loadable_pick_there_is_no_model() {
    // Nothing is chosen on the user's behalf: a loadable model that nobody picked stays unused.
    let chosen = choose_model([None, None], ids(&["qwen3-4b"]), ids(&["codex:gpt"]));
    assert_eq!(chosen.model_id, None);
    assert_eq!(chosen.source, ResolveSource::None);

    let gone = choose_model([None, some("deleted-model")], ids(&["qwen3-4b"]), Vec::new());
    assert_eq!(gone.model_id, None);
    assert_eq!(gone.source, ResolveSource::None);
}
```

Die bisherigen Tests für `DefaultDevice`, `DefaultVault`, `FirstAvailable` und `None` (Zeilen 48–83) werden gelöscht, den Kopfkommentar anpassen ("…then nothing: the person chooses").

**Step 2:** `env -u CC -u CXX -u AR systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 cargo test --manifest-path src-tauri/Cargo.toml --lib -j 1 -- default_model` → FAIL (Kompilierfehler: Arraylänge, `ResolveSource`-Varianten).

**Step 3: Implementieren.** In `default_model.rs`:

```rust
pub enum ResolveSource {
    /// The model the thread used last: a continued session keeps its model.
    Session,
    LastActive,
    None,
}
```

`PREF_DEFAULT_MODEL` und seine Verwendung in `resolve_model` und `resolve_default_local_model` entfernen. In `resolve_model` die DB-Lesung auf `session` und `last_active` kürzen, `installed_local_ids`/`api_key_ids` bleiben (Ladbarkeitsprüfung). `choose_model`:

```rust
pub(crate) fn choose_model(
    preferred: [Option<String>; 2],
    loadable_local: Vec<String>,
    loadable_api: Vec<String>,
) -> ResolveDefaultModelResult {
    const SOURCES: [ResolveSource; 2] = [ResolveSource::Session, ResolveSource::LastActive];
    let is_loadable =
        |id: &str| loadable_api.iter().any(|s| s == id) || loadable_local.iter().any(|s| s == id);
    for (candidate, source) in preferred.into_iter().zip(SOURCES) {
        if let Some(id) = candidate.filter(|id| is_loadable(id)) {
            return ResolveDefaultModelResult {
                model_id: Some(id),
                source,
            };
        }
    }
    ResolveDefaultModelResult {
        model_id: None,
        source: ResolveSource::None,
    }
}
```

`resolve_default_local_model` (Preload): nur noch das gemerkte lokale `last_active`, sonst `Ok(None)` (kein "erstes ladbares"). Den Rumpf nach dem Lesen ersetzen durch:

```rust
    let Some(last) = last_active.filter(|id| !id.is_empty() && !id.contains(':')) else {
        return Ok(None);
    };
    let loadable = paths::canonical_model_file(app, &last)
        .ok()
        .flatten()
        .is_some();
    Ok(loadable.then_some(last))
```

mit einer DB-Lesung, die nur `last_active` liefert. Docstrings von `resolve_default_model` (FR-015-Absatz "FirstAvailable") anpassen. In `preflight.rs`: das Feld `default_model` aus `Facts` entfernen, in `facts_of` die Berechnung (Zeilen 139–142) streichen, in `ask_for_model` `let target = facts.default_model…unwrap_or(first)` durch `let target = first;` ersetzen und den Docstring "the default model when it fits" streichen; Import `PREF_DEFAULT_MODEL` raus. In `model_jobs_run_tests.rs` den Test, der `PREF_DEFAULT_MODEL` setzt (Zeile 165), anpassen: erwartet wird jetzt das erste passende Modell statt des Standardmodells. In `maintenance.rs`: `"chat.default_model_id"` aus `DEVICE_PREFERENCE_KEYS` entfernen und den Kommentar (Zeile 17) kürzen. Hinweis: `maintenance_tests.rs` und `preferences_tests.rs` nennen den Schlüssel; dort nur dann ändern, wenn sie wegen des entfernten Eintrags brechen, und dann auf `chat.notifications` als Beispielschlüssel umstellen.

**Step 4:** `cargo test … -- default_model` → PASS; danach `cargo test … -- model_jobs` und `-- maintenance` und `-- preferences`; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -j 2` (im Scope) ohne neue Findings.

**Step 5: Commit** `refactor(chat): drop the default model from the model resolver`.

### Task 7: Frontend – Standardmodell entfernen

**Files:**

- Delete: `src/components/settings/DefaultModelSetting.vue`
- Modify: `src/lib/settings/registry.ts` (Eintrag `ai.default`)
- Modify: `src/components/wm/appRoutes.ts` (Route `ai.default`)
- Modify: `src/lib/actions/settingsActions.ts:346-366`, `src/stores/settingsActionHandlers.ts:276-287` (+ Konstante `DEFAULT_MODEL_KEY`, falls danach ungenutzt)
- Modify: `src/i18n/locales/de.json`, `en.json` (`settings.locations.ai.default`, `settings.default`)
- Modify: `scripts/check-settings.ts` (`ai.default` aus `LOCATION_PATHS`, Zeilenlisten und Gruppen)
- Modify: `src/composables/usePreferences.ts`, `src/stores/models.ts` (Kommentare)

**Step 1: Test (rot).** In `check-settings.ts` `ai.default` aus `LOCATION_PATHS`, aus `rows('ai')` und aus der Gruppe `chat` streichen (`['chat', ['ai.providers']]`). `pnpm check:settings` → FAIL.

**Step 2:** Registry-Eintrag samt Kommentar `ai.default`, Route, die zwei Actions samt Handlern, die Datei `DefaultModelSetting.vue` und die i18n-Blöcke entfernen. `grep -rn "DEFAULT_MODEL_KEY\|default_model_id\|settings.default\b" src scripts` bis auf `src/pages/onboarding/[instance].vue` leer bekommen.

**Step 3:** `pnpm check:settings && pnpm check:agent-actions && pnpm check:mcp-actions && pnpm check:templates && pnpm typecheck` → grün. (Die Action-Checks prüfen den Katalog gegen Handler und Beschreibungen.)

**Step 4: Commit** `refactor(settings): remove the default model setting`.

### Task 8: Onboarding

**Files:**

- Modify: `src/pages/onboarding/[instance].vue:91-115`

**Step 1:** Den Block ab `// Set device-scoped default model` ersetzen: der Wizard merkt das Modell als zuletzt gewähltes, weil es der Chat so übernimmt:

```ts
// The wizard's explicit choice is the model a new chat starts with: the last picked one
// (spec 002 FR-009 as amended 2026-11-02). There is no default model.
// action-exempt: onboarding runs before the window manager (spec 002), outside the app catalog.
await setPrefAsync(
  { kind: 'device', uuid: info.vaultDeviceUuid },
  'ai.active_local_model_id',
  installed.id,
)
await setPrefAsync(
  { kind: 'device', uuid: info.vaultDeviceUuid },
  'chat.last_active_model_id',
  installed.id,
)
```

**Step 2:** `pnpm typecheck && pnpm check:templates`; Spec-002-Text unten nachziehen (Task 9).

### Task 9: Chat ohne Modell prüfen, Spec nachziehen

**Files:**

- Verify: `src/components/chat/Composer.vue:68-70`, `src/stores/models.ts:350-390` (Kommentar zu `autoLoadFirstAvailableModel`)
- Verify: `src-tauri/src/chat/send_admission.rs`, `src-tauri/src/assistant/turn.rs:331-340`
- Modify: `specs/002-onboarding-model-prefs/spec.md` (FR-007, FR-009, FR-014)

**Step 1: Prüfen, nicht raten.** App starten (`run`-Skill oder Dev-Server), Vault ohne `chat.last_active_model_id` öffnen. Erwartet: Auswahl "Modell wählen", Senden nicht möglich. Fehlt die Sperre, in `Composer.vue` das Senden mit `!displayModelId` sperren, mit einem Test in `scripts/check-chat-state.ts`. `assistant/turn.rs` ruft `use_session_model` und bekommt bei `None` ein `ResolveDefaultModelResult { model_id: None }`: prüfen, dass der Sprachassistent daraus eine Meldung macht und nicht abstürzt; sonst einen Fehler `no model chosen` zurückgeben.

**Step 2:** Den Kommentar zu `autoLoadFirstAvailableModel` in `stores/models.ts` ("then its default, then the vault default, then simply the first available one") auf die neue Kette bringen und die Funktion umbenennen in `restoreLastModel` (nur Name und Aufrufstellen).

**Step 3:** Spec 002 anpassen: FR-007 (Wizard setzt das zuletzt gewählte Modell), FR-009 (kein Standardmodell), FR-014 (Kette Session → zuletzt gewählt → keins). Je ein Satz "Geändert 2026-11-02: …" nach dem Muster der vorhandenen Amendments.

**Step 4:** Alle `check:*` und `pnpm lint`. Commit `fix(chat): no model is chosen on the user's behalf`. Push, CI-E2E anstoßen, PR.

---

# Stufe 3: Eigener Slot für das aktive lokale Modell (Backend)

> **Stand 2026-11-03:** umgesetzt, mit zwei Abweichungen vom Plan unten. (1) Stufe 3 enthält das minimale Frontend (Aktivieren, Entladen und das Badge "Aktiv" in der bestehenden Liste, die gefilterte Chat-Auswahl, der Wizard), weil der Chat ein lokales Modell sonst nicht mehr laden könnte. (2) `load_model_inner` bekommt `LoadOptions { mode, integrity_override }` (`LoadMode::Chat` übernimmt den Motor aus dem Slot, `LoadMode::Activate` lädt in den Slot); ein Chat-Laden bricht den Preload nicht mehr ab, eine Aktivierung wird nur von einem geschlossenen Vault überholt. Der Befehl `load_model_with_integrity_override` heißt jetzt `activate_local_model_with_integrity_override`, `unload_local_model` ist `deactivate_local_model`. Die Modellaufträge (`model_jobs/chat_slot.rs`) leihen sich das aktive lokale Modell statt der Chat-Sitzung.

Neuer Worktree. **Vor dem Start diese Stufe feinplanen** (Tests und Zeilen), nach `executing-plans`-Muster. Die Aufgaben und ihre Reihenfolge stehen fest:

### Task 10: Slot in ChatState

- `src-tauri/src/chat/session.rs`: Feld `local_model: Arc<Mutex<Option<ActiveSession>>>` neben `session` (Konstruktor, `Clone`, `reset_for_close`). Zugriffe nur über Methoden `local_model()` (Klon) und `set_local_model(Option<ActiveSession>)`. `session` bleibt unverändert, damit die rund 240 Zugriffe nicht angefasst werden.
- Test (`session_tests.rs`, falls vorhanden, sonst neues `*_tests.rs`): Slot nach `reset_for_close` leer; Slot und `session` getrennt.

### Task 11: Befehle aktivieren und entladen

- `src-tauri/src/chat/model_loading.rs`: `activate_local_model(model_id)`, `deactivate_local_model()`, `active_local_model_info()` als `#[tauri::command]`, registriert in `lib.rs` (Block bei `use_session_model`, Zeile ~808).
- Aktivieren: `chat.cancel_preload_and_wait()`, `chat.acquire_operation()?` (schlägt fehl, solange eine Antwort läuft; das ist die Ablehnung "Modell wird gerade benutzt"), `resolve_local_model_metadata` + `load_local_model_from_metadata` (Integritätsprüfung bleibt), Slot setzen, Pref `ai.active_local_model_id` (Gerät, `preferences::insert_or_update`) schreiben, dieselben Load-Phasen-Ereignisse wie bisher (`publish_load_phase`, `set_model_ready`).
- Das bisher aktive Modell wird erst nach erfolgreichem Laden ersetzt (wie heute bei `session`).
- Entladen: Operation holen, Slot leeren, Pref löschen, falls `session` dasselbe lokale Modell hält: `session` ebenfalls leeren.
- Der alte Befehl `unload_local_model` (leert `session`) und `useChat.unloadLocalModel` (`src/composables/useChat.ts:438`) werden durch `deactivate_local_model` ersetzt; es gibt keinen Aufrufer außer dem Composable (geprüft per grep).
- Tests (`model_loading_tests.rs` oder neuer `local_slot_tests.rs`, mit dem vorhandenen Fake-`ProviderAdapter` der Session-Tests; `grep -rn "impl ProviderAdapter for" src-tauri/src --include=*_tests.rs`): Aktivieren ersetzt; Entladen leert und löscht das Pref; Aktivieren und Entladen werden abgelehnt, solange `operation_busy()` gilt.

### Task 12: Chat nutzt den Slot, lädt aber nichts

- `load_model_inner`, lokaler Zweig (`model_loading.rs`, `else`-Zweig ab "#[cfg(feature = "llm-cpu")]"): statt `resolve_local_model_metadata`/`load_local_model_from_metadata` den Slot lesen. Ist `slot.model_id == model_id`: `session = slot.clone()` (der `Arc<dyn ProviderAdapter>` wird geteilt, kein zweiter Motor). Sonst `HolziError::InvalidInput { reason: "local model is not active" }`.
- Die Tool-Prüfung im Hintergrund (`spawn_tool_use_check`) läuft weiter nur beim Laden in den Slot, nicht bei jedem Chat-Wechsel auf das lokale Modell.
- `load_local_session` (Modellaufträge, spec 055 FR-033) bleibt unverändert. Falls das Job-Modell dem aktiven entspricht, könnte es den Slot teilen; das ist bewusst nicht Teil dieser Stufe.
- Test: Chat wählt das aktive lokale Modell → `session.adapter` ist derselbe `Arc` wie im Slot (`Arc::ptr_eq`); wählt ein anderes lokales Modell → Fehler; wählt ein API-Modell → Slot unverändert.

### Task 13: Resolver und Preload

- `resolve_model`: `loadable_local` ist `[slot.model_id]` oder leer (nicht mehr "Datei existiert"). Ein gemerktes lokales Modell, das nicht aktiv ist, zählt damit als "keins".
- `start_default_model_preload` → `start_active_model_preload` (Aufrufstellen: `instances/open.rs:196`, `import.rs:237`, `create.rs:170`): liest `ai.active_local_model_id`, lädt in den Slot (`activate`-Logik ohne Operation-Mutex wie bisher, abbrechbar über das Token), setzt keine `session`. `resolve_default_local_model` entfällt.
- Datei `default_model.rs` in `session_model.rs` umbenennen, wenn die Umbenennung klein bleibt; sonst Name lassen (Kommentar anpassen).

### Task 14: Löschen

- `models/commands.rs::delete_installed_model` (Zeile ~658): ist die ID das aktive Modell → zuerst entladen (dieselbe Logik wie `deactivate_local_model`, ohne die Operation doppelt zu holen). `ChatState` ist dafür als `State` zusätzlich nötig; Signatur und `lib.rs`-Registrierung bleiben gleich.
- Test: Löschen des aktiven Modells leert Slot und Pref.

### Task 15: Verifikation Stufe 3

- `cargo test --lib -j 1 -- chat::`, `-- model_loading`, `-- models::`, `cargo clippy --all-targets -j 2` (Scope), Android-Clippy laut Memory `project-host-cargo-env-ndk-vars` (wegen cfg-Zweigen, `--no-default-features`).
- `pnpm gen`-Äquivalent für ts-rs-Bindings: Whitespace am Zeilenende nach `cargo test` bereinigen (`sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`).

---

# Stufe 4: Dropdown statt Liste

> **Geändert 2026-11-03:** Die Aufgaben 16 bis 19 unten sind mit Stufe 3 erledigt. Stufe 4 ist jetzt die Oberfläche "Auf diesem Gerät" als **Dropdown**: Es listet die heruntergeladenen Modelle mit Badges ("Aktiv", "Update verfügbar") und wählt das Modell, mit dem man arbeitet; die Knöpfe rechts daneben wirken auf das gewählte Modell: Aktivieren (beim aktiven Modell Entladen), Aktualisieren (nur bei einem Update, läuft es: Abbrechen) und Löschen. Unter dem Dropdown stehen die Details des gewählten Modells (Quelle, Integrität, Größe, Update-Status). Beim Öffnen ist das aktive Modell vorgewählt. Offen: Badges im geschlossenen Dropdown brauchen einen Erweiterungspunkt in haex-ui (`SettingsSelect` zeigt nur ein `label`).

(Ursprüngliche Planung der Frontend-Anbindung, erledigt mit Stufe 3:)

Neuer Worktree nach dem Merge von Stufe 3; **vor dem Start feinplanen**.

### Task 16: Composables und Actions

- `src/composables/useChat.ts`: `activateLocalModelAsync(id)`, `deactivateLocalModelAsync()`, `activeLocalModelInfoAsync()` neben `activeModelInfoAsync` (Zeile ~443); `unloadLocalModel` ersetzen.
- `src/lib/actions/settingsActions.ts` und `src/stores/settingsActionHandlers.ts`: Actions `settings.models.activate` (`modelId`) und `settings.models.deactivate`, Scope `settings.models`, Effekt `write`, Beschreibungen englisch wie die Nachbarn. Handler rufen den Store; `check:agent-actions` und `check:mcp-actions` laufen lassen.

### Task 17: Store

- `src/stores/models.ts`: `activeLocalModelId` (Ref) und `refreshActiveLocalModel()`; in `initialize()` nach `refreshActiveModel()` laden und bei `ready`/`idle`-Statusereignissen aktualisieren. `activeModel` bleibt das Modell der Sitzung.
- `modelGroups` (Chat-Auswahl, Quelle: `useModels`-Zusammenstellung, destrukturiert in `stores/models.ts:64`): die Gruppe der lokalen Modelle enthält nur das aktive. Das ist eine Filterung an der Stelle, die die Gruppen baut, mit einem Fall in `scripts/check-chat-model-search.ts` oder einem neuen `check-chat-model-groups.ts`.
- Node-Sandbox-Regel beachten: keine DOM-Globals im Composable; danach alle `check:*`.

### Task 18: Lokale Modelle – Aktivieren, Entladen, Aktiv

- `src/components/settings/InstalledModels.vue`: das Badge `models.installed.active` hängt an `activeLocalModelId === model.id` (nicht mehr an `activeModelId` der Sitzung). Der Button "Laden" wird "Aktivieren" (`settings.models.activate`); an der aktiven Zeile steht stattdessen "Entladen" (`settings.models.deactivate`). Fehler beim Aktivieren (Hardware reicht nicht) erscheinen in der Zeile, wie `loadErrorMessage` heute. Der Integritätsdialog bleibt; seine Entscheidungen laufen weiter über `chat.modelIntegrity.decide`, das im Store jetzt `activate` statt `selectModel` nutzt (`useModelIntegrity`, `onIntegrityLoadUntrusted`).
- Löschen des aktiven Modells: `models.installed.deleteConfirm` ergänzen um den Satz "Das Modell wird dabei entladen."
- i18n de/en: `models.installed.activate` "Aktivieren" / "Activate", `models.installed.unload` "Entladen" / "Unload"; `models.installed.load` entfällt. Beschreibung der Seite um einen Satz erweitern: "Aktiv ist das Modell, das im Speicher geladen ist." und der Hinweis aus dem Design: ein API-Modell im Chat ändert daran nichts.

### Task 19: Chat-Seite

- `chat.model.select` für ein lokales Modell, das nicht aktiv ist, kann nicht mehr vorkommen (Liste gefiltert). Für ein verwaistes Auswahl-Pref (`chat.last_active_model_id` auf ein nicht aktives lokales Modell) greift "kein Modell" aus Stufe 2.
- E2E-Szenarien unter `scripts/e2e/scenarios/`: neues `local-model-activate.test.ts`: Modell aktivieren → in der Chat-Auswahl wählbar → entladen → verschwindet aus der Auswahl; und `chat-no-model.test.ts`: ohne Modell ist das Eingabefeld gesperrt. Muster: bestehende `models-download-confirm.test.ts`. Vor dem Merge `gh workflow run ci.yml --ref <branch>`.

### Task 20: Specs

- `specs/005-huggingface-model-discovery/spec.md`, `specs/012-unified-model-capabilities/spec.md`, `specs/023-settings-app/spec.md`, `specs/055-extension-model-access/spec.md` (US5): neue Namen und Pfade ("Anbieter", "Lokale Modelle", `/ai/providers`, `/ai/local`) und die Semantik "aktiv = geladen" mit je einem Satz "Geändert 2026-11-02".
- Alle `check:*`, `pnpm lint`, `pnpm typecheck`; PR.

---

## Risiken

- **Stufe 3 trifft das Laden, den Preload und das Schließen der Vault.** Die Tests dort sind die Absicherung; ein Slot, der beim Schließen nicht frei wird, hielte das Modell über Vault-Wechsel im Speicher (`reset_for_close` deckt das ab, Test in Task 10).
- **Zwei Listen laden beim Mounten beide `refreshInstalledAndCatalog()`** (Stufe 1). Doppelte Abfrage, kein Fehler; bei Bedarf später in `LocalModelsView` bündeln.
- **`model_jobs`** nutzt das Standardmodell nur als Vorschlag in der Rückfrage; nach Task 6 schlägt es das erste passende vor. Verhaltensänderung, im PR benennen.
