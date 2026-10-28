# Vertrag: Tauri-Commands und Typen

Alle Commands stehen nur im Handler des Hauptfensters (R13). Typen kommen per `ts-rs` nach
`src/types/bindings/` (`camelCase`). Fehler sind `HolziError` mit stabilen Arten (siehe unten), die
Oberfläche übersetzt sie.

## Liste und Verwaltung (Einstellungen, FR-020 bis FR-029)

| Command                              | Argumente                                                               | Ergebnis                            | Hinweis                                                                     |
| ------------------------------------ | ----------------------------------------------------------------------- | ----------------------------------- | --------------------------------------------------------------------------- |
| `marketplace_list`                   | –                                                                       | `MarketplaceView[]`                 | Standard + Zeilen, nach Rang                                                |
| `marketplace_save`                   | `MarketplaceInput { id?, name, baseUrl, authType, credentialsItemId? }` | `MarketplaceView`                   | legt an oder ändert; prüft Adresse, `https` bei Anmeldung (FR-024, FR-027)  |
| `marketplace_set_enabled`            | `id`, `enabled`                                                         | –                                   | gilt sofort, auch für `default` (legt die Überschreibung an)                |
| `marketplace_reorder`                | `ids: string[]` (vollständige Reihenfolge)                              | –                                   | schreibt `sort_order` neu, auch `default`                                   |
| `marketplace_delete`                 | `id`                                                                    | –                                   | `default` → Fehler `marketplace_default_protected`; eigene Zeile weg        |
| `marketplace_test`                   | `MarketplaceInput`                                                      | `TestResult { ok, count?, error? }` | `GET /health` und erste Seite; sendet Zugangsdaten nur bei `https` (FR-021) |
| `marketplace_credentials_candidates` | `authType`                                                              | `CredentialsCandidate[]`            | passende Einträge, getaggte zuerst (R11)                                    |

## Katalog (FR-001 bis FR-005, FR-017, FR-018, FR-031)

| Command                     | Argumente                                                                  | Ergebnis                                                                                       |
| --------------------------- | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `marketplace_catalog`       | `CatalogQuery { page, search?, category? }` (`limit` 20, `sort` downloads) | `CatalogPage`                                                                                  |
| `marketplace_categories`    | –                                                                          | `{ categories, sourceErrors }`                                                                 |
| `marketplace_details`       | `sourceId`, `slug`                                                         | `CatalogDetail` (Eintrag + Beschreibung, Schlüssel-Fingerabdruck, `publisherMatchesInstalled`) |
| `marketplace_icon`          | `sourceId`, `url`                                                          | `string` (`data:`-URL) oder Fehler `marketplace_icon_unavailable`                              |
| `marketplace_known_updates` | –                                                                          | `KnownUpdate[]` (nur was der Katalog zuletzt sah, FR-017)                                      |

Jeder Katalogabruf ersetzt die bekannten Updates der abgefragten Quellen. Ein neuer Aufruf von
`marketplace_catalog` bricht einen noch laufenden ab.

## Download und Installation (FR-010 bis FR-016)

| Command                        | Argumente                                                              | Ergebnis                                                       |
| ------------------------------ | ---------------------------------------------------------------------- | -------------------------------------------------------------- |
| `marketplace_download`         | `sourceId`, `slug`, `version`, `onProgress: Channel<DownloadProgress>` | `PendingBundleRef { id }`                                      |
| `marketplace_download_cancel`  | `id`                                                                   | –                                                              |
| `marketplace_download_discard` | `id`                                                                   | – (Schließen der Vorschau)                                     |
| `extension_install_preview`    | `source: BundleSource`                                                 | `InstallPreview` (+ `origin?: { sourceName }` bei Marketplace) |
| `extension_install`            | `ExtensionInstallArgs { source: BundleSource, accepted, confirmed }`   | `ExtensionSummary`                                             |

```ts
type BundleSource =
  | { kind: 'file'; file: PickedFile } // wie bisher
  | { kind: 'marketplace'; id: string } // PendingBundleRef.id
```

`extension_install` mit `marketplace` verbraucht den Speicherplatz bei Erfolg. Das Entfernen
(`extension_remove`, Spec 017) bleibt unverändert und wird vom Katalog aufgerufen (FR-016).

## Ereignisse

- `vault-data-changed` (bestehend): die Liste der Marketplaces und die Kachelzustände abonnieren
  `haex_marketplaces` bzw. die Erweiterungen (`onVaultTablesChanged`, Spec 028-Live-Daten).
- Kein neues Ereignis.

## Fehlerarten (`HolziError::Marketplace { kind }`)

`marketplace_default_protected`, `marketplace_invalid_url`, `marketplace_credentials_required`,
`marketplace_insecure_credentials`, `marketplace_not_found`, `marketplace_no_hash`,
`marketplace_hash_mismatch`, `marketplace_size_mismatch`, `marketplace_too_large`,
`marketplace_incompatible_version`, `marketplace_download_cancelled`, `marketplace_source_*`
(`unreachable`, `timeout`, `auth_rejected`, `bad_response`), `marketplace_icon_unavailable`,
`marketplace_pending_missing` (Speicherplatz leer oder abgelaufen).

## Nicht vorhanden (FR-030)

Keine Aktion in `*ActionHandlers`, kein MCP-Werkzeug, keine Methode der Erweiterungs-Prüfstelle. Ein Test
in `marketplace/commands_tests.rs` prüft das namentlich.
