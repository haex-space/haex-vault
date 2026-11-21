# Phase 1 Data Model: Onboarding-Härtung und Modellwahl-Persistenz

## Neue Entitäten

### `preferences` (sync-tracked)

Namespaced Key-Value-Speicher für nutzergesetzte Einstellungen. Composite-PK erlaubt sowohl device-scoped als auch vault-weite Einträge in derselben Tabelle. Sync-tracked — jedes Gerät sieht alle Rows aller anderen Geräte, filtert für lokale Anzeige per `WHERE vault_device_uuid IN (my_uuid, VAULT_SCOPE_UUID)`.

**Migration 0011**:

```sql
CREATE TABLE preferences (
  vault_device_uuid TEXT NOT NULL
    REFERENCES known_devices(vault_device_uuid) ON DELETE CASCADE,
  key TEXT NOT NULL,
  value TEXT,
  PRIMARY KEY (vault_device_uuid, key)
);

CREATE INDEX idx_preferences_key
  ON preferences (key, vault_device_uuid);
```

**Spalten**:

| Name                       | Typ  | Nullable   | Beschreibung                                                                                                                                                                                                                |
| -------------------------- | ---- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `vault_device_uuid`        | TEXT | NO         | UUID eines Geräts aus `known_devices.vault_device_uuid`, ODER `'00000000-0000-0000-0000-000000000000'` (Sentinel für vault-weit). Hard-FK auf `known_devices(vault_device_uuid) ON DELETE CASCADE`.                         |
| `key`                      | TEXT | NO         | Dotted-namespaced Schlüsselname, z. B. `chat.default_model_id`, `chat.last_active_model_id`. Kein Format-Enforcement in SQL — Konvention lebt im Rust-Wrapper.                                                              |
| `value`                    | TEXT | YES        | Freier Textwert. `NULL` wird semantisch wie "absent" behandelt (Wrapper API surfaced sie nicht separat). Für Modell-IDs enthält der Wert die Composite-ID (`<provider_uuid>:<remote>` für api_key, Katalog-Slug für lokal). |
| `haex_hlc_no_sync`         | TEXT | (implicit) | Vom `CrdtTransformer` zur Migration-Time hinzugefügt. Storage-Wrapper setzt bei jedem Insert/Update `current_hlc()`.                                                                                                        |
| `haex_column_hlcs_no_sync` | TEXT | (implicit) | Column-HLC-Map, vom Transformer verwaltet.                                                                                                                                                                                  |
| `haex_column_sigs_no_sync` | TEXT | (implicit) | Column-Signature-Map, vom Transformer verwaltet.                                                                                                                                                                            |

**Validierungsregeln** (Rust-Wrapper):

- `key` ist nicht-leer und enthält mindestens einen `.` (Namespace-Erzwingung).
- `vault_device_uuid` ist ein gültiges UUID-Format (parseable via `Uuid::parse_str`). Für `PrefScope::Device(uuid)` wird zusätzlich `Uuid::nil()` abgewiesen; die nil-UUID ist ausschließlich `PrefScope::Vault` und dessen Sentinel-Row vorbehalten. Andere parsebare UUIDs sind für den Device-Scope zulässig.
- `set_pref(scope, key, value)` erzeugt Row wenn nicht vorhanden, sonst UPDATE.
- `clear_pref(scope, key)` = DELETE der Row (nicht Update-auf-NULL).

**Bekannte Keys** (dieses Feature nutzt zwei):

| Key                         | Scope             | Semantik                                                                                                                                                                    |
| --------------------------- | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `chat.default_model_id`     | device oder vault | Expliziter Standardmodell-Wunsch. Wird ausschließlich via Settings-Screen-Trigger gesetzt (FR-011). Kein Auto-Overwrite.                                                    |
| `chat.last_active_model_id` | nur device        | Zuletzt erfolgreich genutztes Modell. Auto-Write ausschließlich nach erfolgreichem `send_message` (FR-009/010); ein manueller Picker-Wechsel ohne Nachricht schreibt nicht. |

**State-Transitions**:

Keine — Preference-Rows sind zustandslos. Ihre Existenz ist der Zustand. Übergänge: `absent → set(x) → set(y) → clear`.

**Beziehungen**:

- N:1 zu `known_devices` via `vault_device_uuid` (mit Sentinel-Sonderrow für vault-weit).
- Impliziter Bezug zu `models` via `value` (bei den zwei Modell-Keys): Werte enthalten Modell-IDs, aber KEIN Hard-FK auf `models(id)`, weil die Modell-Zeile über Sync verspätet ankommen kann oder gelöscht sein könnte, ohne den Preference-Eintrag ungültig machen zu wollen (Selbst-Heilung via Resolver-Fallback).

---

## Geänderte Entitäten

### `known_devices` (sync-tracked, bestehend)

**Semantik-Erweiterung** (kein Schema-Change):

- Eine reservierte Sentinel-Zeile mit `installation_uuid = '00000000-0000-0000-0000-000000000000'`, `vault_device_uuid = '00000000-0000-0000-0000-000000000000'`, `alias = NULL`, `first_seen = 0` repräsentiert "vault-scope" als FK-Ziel für `preferences.vault_device_uuid`.
- Wird von `HaexBootstrap::bootstrap` per `INSERT OR IGNORE` bei jedem Open geschrieben, VOR dem `installation_uuid`-Lookup für das aktuelle Device.
- Storage-Wrapper `list_known_devices()` filtert die Sentinel-Zeile per `WHERE vault_device_uuid != '00...'` heraus, damit sie nicht in UI-Anzeigen von "Geräte im Vault" auftaucht.

**Existierende Spalten** (unverändert):

| Name                | Typ     | Nullable    | Beschreibung                                                                                                                                                                                                                        |
| ------------------- | ------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `installation_uuid` | TEXT    | NO (PK)     | Local-only Lookup-Schlüssel; für Sentinel: nil-UUID.                                                                                                                                                                                |
| `vault_device_uuid` | TEXT    | NO (UNIQUE) | CRDT-`device_id`; für Sentinel: nil-UUID; FK-Ziel für `preferences`.                                                                                                                                                                |
| `alias`             | TEXT    | YES         | Menschenlesbarer Gerätename; Nullness triggert das Onboarding. Der Wizard hält den Wert zunächst nur lokal und persistiert ihn erst beim erfolgreichen Abschluss des zweiten Schritts; danach ist er im Settings-Screen editierbar. |
| `first_seen`        | INTEGER | NO          | Epoch-ms des ersten Bootstrap; für Sentinel: 0.                                                                                                                                                                                     |

**Neuer Trigger für Alias**:

- Onboarding-Wizard MUSS `alias` beim Abschluss setzen (FR-004) — nicht-leer. Das Speichern wird bis nach der Modellwahl oder der expliziten Auswahl "später via Anbieter" zurückgestellt, damit ein Abbruch zwischen den Schritten `alias = NULL` lässt und das Onboarding erneut öffnet.
- Settings-Screen erlaubt Rename via bestehendem `storage::known_devices::update_alias`.

---

## Gestrichene Entitäten

### `device_downloaded_models_no_sync` (bestehend, gelöscht)

**Migration 0012**:

```sql
DROP TABLE device_downloaded_models_no_sync;
```

Ersatz: `list_installed_models` scannt `<AppLocalData>/models/` per readdir. Ein Sub-Verzeichnis gilt nur als installiert, wenn eine vollständige reguläre `.gguf`-Datei vorhanden ist; Downloads und Importe veröffentlichen die finale Datei erst nach atomarem Rename, sodass temporäre `.part`-/`.tmp`-Dateien nicht zählen. Der gemeinsame Selector `models::paths::canonical_model_file` wählt bei mehreren vollständigen `.gguf`-Dateien die lexikografisch kleinste UTF-8-Datei nach Dateiname; `relativePath`, `size_bytes` und `LocalModel::load` beziehen sich immer auf diese Datei. Der Verzeichnisname muss als `models.id` existieren. Fehlt das Root-Verzeichnis, ist die Liste leer; `sha256` und `verified_at` fallen ersatzlos weg (LocalModel-Load selbst ist Integritätscheck).

---

## Neue Rust-Konstanten

```rust
// src-tauri/src/identity/mod.rs
pub const VAULT_SCOPE_UUID: uuid::Uuid = uuid::Uuid::nil();
```

## Neue Rust-Typen

```rust
// src-tauri/src/storage/preferences.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefScope {
    Vault,           // → VAULT_SCOPE_UUID
    Device(Uuid),    // → konkrete vault_device_uuid
}

pub struct PrefRow {
    pub scope: PrefScope,
    pub key: String,
    pub value: Option<String>,
}
```

```rust
// src-tauri/src/device/commands.rs

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfoPayload {
    pub installation_uuid: Uuid,
    pub vault_device_uuid: Uuid,
    pub alias: Option<String>,
    /// Roher OS-Hostname wenn ermittelbar, sonst None. Frontend wendet
    /// i18n-Fallback `onboarding.alias.defaultPlaceholder` als echten
    /// initialen Eingabewert an — kein
    /// lokalisierter String im Backend (FR-020).
    pub hostname: Option<String>,
}
```

```rust
// src-tauri/src/catalog/mod.rs (Erweiterung)

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierRecommendation {
    pub tier: Tier,               // Easy | Sweet | Max
    pub entry: CatalogEntry,      // bereits existierender Typ
    pub fit: Fit,                 // bereits existierender Typ aus hardware::fit
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Easy,   // kleinster mit Fit::Fits
    Sweet,  // größter mit Fit::Fits (Median-Fallback wenn zu wenig Fits-Kandidaten)
    Max,    // größter mit Fit::Fits ODER Fit::Tight
}
```

## Frontend-Typen (Composables)

```typescript
// src/composables/usePreferences.ts

export type PrefScope = { kind: 'vault' } | { kind: 'device'; uuid: string }

export interface PrefRow {
  scope: PrefScope
  key: string
  value: string | null
}
```

```typescript
// src/composables/useDevice.ts

export interface DeviceInfo {
  installationUuid: string
  vaultDeviceUuid: string
  alias: string | null
  hostname: string | null
}
```

```typescript
// src/composables/useCatalog.ts (Erweiterung)

export interface TierRecommendation {
  tier: 'easy' | 'sweet' | 'max'
  entry: CatalogEntryWithFit // bereits existierend
  fit: CatalogEntryWithFit['fit']
}
```

---

## Migrations-Reihenfolge und -Constraints

- **0011 preferences** kommt VOR **0012 DROP device_downloaded_models_no_sync** (sequentiell nach `MigrationName`-Ordering).
- **0011** kann erst laufen wenn `known_devices` existiert — das ist Migration `0002`, also unproblematisch.
- **0012** hat keinen Vorbedingung außer "device_downloaded_models_no_sync existiert" — was Migration `0007` sicherstellt.
- Der `list_installed_models`-Code muss bereits gegen die neue Filesystem-Scan-Semantik gebaut sein, BEVOR 0012 in Prod läuft — sonst würde ein bestehender Nutzer nach dem App-Update seine Modell-Sicht verlieren, weil die Tabelle weg ist.

**Nicht-Regression für bestehende Nutzer**: der Filesystem-Scan liefert dieselben installierten Modelle wie vor der Migration, weil er dieselben Slug-Dir-Namen wie die DB-Rows nutzt (`AppLocalData/models/<slug>/<file>`). Kein Datenverlust; nur die 4 Extra-Metadaten (`size_bytes`, `sha256`, `verified_at`, `relative_path`) fallen weg. `size_bytes` wird bei Anzeige neu berechnet; die anderen drei sind nicht mehr abrufbar.
