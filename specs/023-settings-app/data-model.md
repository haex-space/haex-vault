# Data Model: Einstellungs-App mit Kategorien

> **Hinweis (2026-10-15)**: Maßgeblich für Kategorien und Orte ist heute
> `src/lib/settings/registry.ts`. „Modelle“ und „Agenten“ sind seit PR #454 die
> Kategorie „KI“ (`/ai`, Orte `ai.*`); die Pfade `/models/…` und `/agents/…`
> in diesem Dokument sind der Stand bei der Umsetzung. „Darstellung“ gehört
> seit Spec 042 zu „Allgemein“.

**Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Diese Spec legt keine Tabellen an. Neu sind ein Frontend-Register, eine
Präferenz, Zustand im Modell-Store und ein Lesetyp für die Geräte der Vault.

## Kategorie (`SettingsCategory`)

Reines Datum in `src/lib/settings/registry.ts` (R2).

| Feld             | Typ                                                                 | Regel                                                                      |
| ---------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `id`             | `'general' \| 'appearance' \| 'models' \| 'agents' \| 'federation'` | genau eine der fünf Kategorien; Reihenfolge des Arrays = FR-005            |
| `path`           | `string`                                                            | Ort der Kategorie: `/`, `/appearance`, `/models`, `/agents`, `/federation` |
| `icon`           | `string`                                                            | Iconify-Name (`lucide:*`)                                                  |
| `titleKey`       | `string`                                                            | `settings.categories.<id>.title`                                           |
| `descriptionKey` | `string`                                                            | `settings.categories.<id>.description`                                     |

Eine Kategorie mit Unteransichten zeigt an ihrem Ort die Übersicht; eine ohne
(Allgemein, Darstellung, Föderation) zeigt ihren Inhalt direkt (FR-003).

## Ort (`SettingsLocation`)

| Feld             | Typ            | Regel                                                                                |
| ---------------- | -------------- | ------------------------------------------------------------------------------------ |
| `id`             | `string`       | eindeutig, z. B. `models.download.repo`                                              |
| `pattern`        | `string`       | Routenmuster relativ zu `/`, z. B. `models/download/repo/:owner/:name`; `''` für `/` |
| `category`       | Kategorie-`id` | bestimmt die Hervorhebung in der Seitenleiste                                        |
| `parent`         | Ort-`id` \| —  | nur Unteransichten; Ziel des Zurück-Pfeils (R3)                                      |
| `icon`           | `string` \| —  | nur Orte, die als Zeile einer Übersicht erscheinen                                   |
| `titleKey`       | `string`       | Kopf und Tab-Titel; darf Routenparameter nutzen (`{owner}/{name}`)                   |
| `descriptionKey` | `string`       | eine Zeile unter dem Titel und in der Übersichtszeile                                |
| `overviewRow`    | `boolean`      | erscheint als Zeile in der Übersicht seiner Kategorie                                |

Die vollständige Liste steht in R1 und im [Vertrag](./contracts/settings-app.md#1-orte).

Abgeleitet:

- `settingsRoutePatterns()`: `RoutePattern[]` für `routeMatch.ts`, Wurzel `/`
  mit einem Kind je Ort.
- `locationFor(path)`: der Ort eines Pfads über `matchRoute`, `undefined` für
  Unbekanntes.
- `overviewRows(categoryId)`: Orte mit `overviewRow` in Registerreihenfolge.
- `headerBack(history)`: `{ kind: 'back', path }` (vorige Station in derselben
  Kategorie), `{ kind: 'push', path }` (übergeordneter Ort) oder `undefined`
  für eine Kategorie (R3).

## Farbschema (Präferenz)

| Eigenschaft        | Wert                                                                             |
| ------------------ | -------------------------------------------------------------------------------- |
| Schlüssel          | `appearance.color_scheme`                                                        |
| Werte              | `light`, `dark`, `system`; jeder andere gespeicherte Wert gilt als nicht gesetzt |
| Scope              | `vault` (FR-024)                                                                 |
| Standard           | `system`, wenn nicht gesetzt                                                     |
| Sync               | wie alle Präferenzen (Tabelle `preferences`)                                     |
| Vor dem Entsperren | `system` (keine Vault offen)                                                     |

Dunkel ist die Ansicht, wenn der Wert `dark` ist oder `system` und das
Betriebssystem dunkel meldet.

## Scopes der Einstellungen (FR-024, research R14)

| Schlüssel                    | Scope                   | Übernahme beim Öffnen                         |
| ---------------------------- | ----------------------- | --------------------------------------------- |
| `appearance.color_scheme`    | `vault`                 | Gerätewert → Vault, wenn die Vault keinen hat |
| `wm.session_restore`         | `vault`                 | Gerätewert → Vault, wenn die Vault keinen hat |
| `chat.autonomy_mode`         | `vault`                 | Gerätewert → Vault, wenn die Vault keinen hat |
| `chat.permission_mode`       | `vault`                 | Gerätewert → Vault, wenn die Vault keinen hat |
| `cli_delegate.deny_rules`    | `vault`                 | Gerätewert → Vault, wenn die Vault keinen hat |
| `chat.reasoning_option.<id>` | `vault`                 | Gerätewert → Vault, wenn die Vault keinen hat |
| `chat.default_model_id`      | `device`                | Vault-Wert → dieses Gerät, wenn es keinen hat |
| `voice.stt_model_id`         | `device`                | —                                             |
| Gerätename                   | Gerät (`known_devices`) | —                                             |

Nach der Übernahme ist der alte Wert dieses Geräts (bzw. beim Standard-Modell
der alte Vault-Wert) gelöscht; Werte anderer Geräte übernimmt jedes Gerät beim
eigenen Öffnen.

## Download-Fortschritt (Download-Store)

In `stores/modelDownloads.ts` (R6), dazu `clearDownload(id)` und
`downloadPercent(id)`:

| Feld               | Typ                                     | Regel                                                                                                      |
| ------------------ | --------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `downloads`        | `Record<string, DownloadProgressEvent>` | Schlüssel Modellkennung; gesetzt bei Fortschritt, 100 % bei Abschluss, entfernt von der startenden Ansicht |
| `watchDownloads()` | `() => Promise<void>`                   | abonniert einmal je Vault-Session; weitere Aufrufe tun nichts                                              |

Die vorhandenen Felder `downloadingId`, `downloadProgressBytes`,
`downloadTotalBytes` (Download aus dem Katalog im Chat) bleiben.

## Gerät der Vault (`VaultDevicePayload`)

Lesetyp des Befehls `list_vault_devices` (R12), aus der vorhandenen Tabelle
`known_devices`.

| Feld              | Typ              | Regel                                                             |
| ----------------- | ---------------- | ----------------------------------------------------------------- |
| `vaultDeviceUuid` | `string` (UUID)  | eindeutig je Gerät und Vault                                      |
| `alias`           | `string \| null` | `null`, solange das Gerät die Einrichtung nicht abgeschlossen hat |
| `isCurrent`       | `boolean`        | genau ein Eintrag ist `true`                                      |

Nicht enthalten: die interne Vault-Bereichszeile (`VAULT_SCOPE_UUID`) und die
`installation_uuid` anderer Geräte. Reihenfolge: dieses Gerät, dann nach Namen,
Geräte ohne Namen zuletzt. „Zuletzt online“ kommt mit der Sync-Spec.

## App-Definition: Tab-Titel

`AppDefinition` in `lib/wm/apps.ts` bekommt `tabTitle?: 'location' | 'app'`
(fehlt = `location`). `system.settings` setzt `'app'`: Der Tab zeigt
`wm.apps.settings`, die Verlaufsliste weiter die Titel der Orte.

## Alias entfallener Apps

In `lib/wm/apps.ts` (R7):

```text
LEGACY_APP_ALIASES: Record<string, { appId: string; at: string }>
  'system.federation' → { appId: 'system.settings', at: '/federation' }
resolveAppAlias(appId) → { appId, at: string | null }
```

## Entfällt

- App-Definition `system.federation`, `components/apps/FederationApp.vue`,
  i18n `wm.apps.federation`.
- Lokaler Zustand in `HuggingFaceModelManagement.vue` (`activeTab`,
  `selectedRepo`, `downloadStates` und Abonnements); die Komponente selbst wird
  aufgeteilt (R10).
- i18n `settings.header.forDevice` (der Kopf zeigt die Kategorie).
