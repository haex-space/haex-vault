# Vertrag: Tauri-Commands und Typen

Alle Commands stehen nur im Handler des Hauptfensters (R13). Typen kommen per `ts-rs` nach
`src/types/bindings/` (`camelCase`). Fehler sind `HolziError` mit stabilen Arten (siehe unten), die
Oberfläche übersetzt sie.

## Liste und Verwaltung (Einstellungen, FR-020 bis FR-029)

| Command                              | Argumente                                                               | Ergebnis                                       | Hinweis                                                                                                           |
| ------------------------------------ | ----------------------------------------------------------------------- | ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `marketplace_list`                   | –                                                                       | `MarketplaceView[]`                            | alle Zeilen, nach Rang (gleichrangig, keine Sonderzeile)                                                          |
| `marketplace_save`                   | `MarketplaceInput { id?, name, baseUrl, authType, credentialsItemId? }` | `MarketplaceView`                              | legt an oder ändert; prüft Adresse, `https` bei Anmeldung (FR-024, FR-027)                                        |
| `marketplace_set_enabled`            | `id`, `enabled`                                                         | –                                              | gilt sofort                                                                                                       |
| `marketplace_reorder`                | `ids: string[]` (vollständige Reihenfolge)                              | –                                              | schreibt `sort_order` neu                                                                                         |
| `marketplace_delete`                 | `id`                                                                    | –                                              | jede Zeile ist löschbar, auch die Voreinstellung; nichts legt sie wieder an                                       |
| `marketplace_add_preset`             | –                                                                       | `MarketplaceView`                              | legt die Voreinstellung (`preset-public`) an, falls sie fehlt; sonst schaltet sie nichts um (Leerzustand, FR-022) |
| `marketplace_test`                   | `MarketplaceInput`                                                      | `MarketplaceTestResult { ok, count?, error? }` | `GET /health` und erste Seite; sendet Zugangsdaten nur bei `https` (FR-021)                                       |
| `marketplace_credentials_candidates` | `authType`                                                              | `CredentialsCandidate[]`                       | passende Einträge, getaggte zuerst (R11)                                                                          |

## Katalog (FR-001 bis FR-005, FR-017, FR-018, FR-031)

| Command                     | Argumente                                                                             | Ergebnis                                                                                                                                                                                                                                         |
| --------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `marketplace_catalog`       | `CatalogQuery { page, search?, category?, sourceId? }` (`limit` 20, `sort` downloads) | `CatalogPage`                                                                                                                                                                                                                                    |
| `marketplace_categories`    | `sourceId?`                                                                           | `{ categories, sourceErrors }`                                                                                                                                                                                                                   |
| `marketplace_details`       | `sourceId`, `slug`                                                                    | `CatalogDetail` (Eintrag + Beschreibung, Schlüssel-Fingerabdruck, `publisherMatchesInstalled`)                                                                                                                                                   |
| `marketplace_icon`          | `sourceId`, `url`                                                                     | `string` (`data:`-URL) oder Fehler `marketplace_icon_unavailable`; Ziel und jede Weiterleitung müssen öffentlich routbar sein (R12)                                                                                                              |
| `marketplace_known_updates` | –                                                                                     | `{ candidates: KnownUpdate[], selected: KnownUpdate[] }` (nur zuletzt bekannte Kandidaten, FR-017)                                                                                                                                               |
| `marketplace_check_updates` | –                                                                                     | `{ updates: KnownUpdate[], sourceErrors }` (fragt für **jede installierte Erweiterung** jede eingeschaltete Quelle mit `search=<Name>`, nur genaue Namenstreffer; läuft beim Öffnen der App und bei „Aktualisieren“, nie im Hintergrund, FR-017) |

Ohne `sourceId` werden alle eingeschalteten Marketplaces gleichzeitig befragt und ihre Einträge hintereinander
in Rangfolge geliefert, jeder mit seiner Quelle, **ohne Zusammenlegung** (FR-012). Jede Quelle liefert die
angeforderte Seitennummer mit bis zu 20 Einträgen; `totalPages` ist das Maximum der Seitenzahlen, `total`
die Summe der Treffer. Eine gemeinsame Seite kann daher mehr als 20 Einträge enthalten. Innerhalb jeder
Quelle gilt die Sortierung nach Downloads absteigend (FR-003). Bekannte Updates werden je
`(installedName, sourceId, slug)` geführt: eine Antwort ändert nur die Kandidaten ihrer Quelle und
Einträge, über die sie geantwortet hat; eine ausgefallene Quelle löscht nichts. In der Installationsliste
wird je Name die höchste bekannte kompatible Fassung gezeigt, bei Gleichstand entscheidet die
Marketplace-Rangfolge. Ein neuer Aufruf von
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

`marketplace_invalid_url`, `marketplace_credentials_required`,
`marketplace_insecure_credentials`, `marketplace_not_found`, `marketplace_no_hash`,
`marketplace_hash_mismatch`, `marketplace_size_mismatch`, `marketplace_too_large`,
`marketplace_incompatible_version`, `marketplace_download_cancelled`, `marketplace_source_*`
(`unreachable`, `timeout`, `auth_rejected`, `bad_response`), `marketplace_icon_unavailable`,
`marketplace_pending_missing` (Speicherplatz leer oder abgelaufen).

## Nicht vorhanden (FR-030)

Keine Aktion in `*ActionHandlers`, kein MCP-Werkzeug, keine Methode der Erweiterungs-Prüfstelle. Ein Test
in `marketplace/commands_tests.rs` prüft das namentlich.
