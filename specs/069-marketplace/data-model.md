# Datenmodell: Marketplace für Erweiterungen

## Tabelle `haex_marketplaces` (Vault-Daten, CRDT-getrackt, synchronisiert)

Migration `0047_marketplaces` (nächste freie Nummer beim Bauen), `HOLZI_TRIGGER_VERSION` + 1. Kein UNIQUE
und kein Fremdschlüssel auf Zugangsdaten (siehe R11, R14).

| Spalte                | Typ                          | Bedeutung                                                                                                |
| --------------------- | ---------------------------- | -------------------------------------------------------------------------------------------------------- |
| `id`                  | TEXT PK                      | zufällige UUID; nur die Voreinstellung trägt die feste Kennung `preset-public` (R10)                     |
| `name`                | TEXT NOT NULL                | Anzeigename                                                                                              |
| `base_url`            | TEXT NOT NULL                | Adresse ohne abschließenden `/`, mit beliebigem Pfadanteil                                               |
| `enabled`             | INTEGER NOT NULL DEFAULT 1   | Ein/Aus                                                                                                  |
| `sort_order`          | INTEGER NOT NULL             | Rang, kleiner = früher; bei Gleichstand `(sort_order, name, id)`                                         |
| `auth_type`           | TEXT NOT NULL DEFAULT `none` | `none`, `bearer` oder `basic`                                                                            |
| `credentials_item_id` | TEXT                         | Passwort-Eintrag (Spec 034); Pflicht bei `bearer`/`basic`, sonst NULL; **kein** Geheimnis in der Tabelle |
| `created_at`          | INTEGER NOT NULL             | Unix-Millisekunden                                                                                       |
| `updated_at`          | INTEGER NOT NULL             | Unix-Millisekunden                                                                                       |

**Regeln**

- `base_url` MUSS mit `http://` oder `https://` beginnen und einen Host haben; ein Pfadanteil ist erlaubt
  (`https://example.org/market`, FR-024). Sie wird normalisiert (ohne abschließenden `/`, ohne Fragment
  und Query); Pfade des Protokolls werden **hinter** den Pfadanteil gehängt.
- `auth_type` ≠ `none` verlangt `credentials_item_id` und eine `https://`-Adresse (FR-027); die Prüfung
  sitzt im Host-Command beim Speichern **und** im Client beim Senden.
- Eine neue Zeile bekommt `sort_order` = größter vorhandener Wert + 1 (ans Ende); der Nutzer schiebt sie
  mit Auf/Ab, die Oberfläche schreibt dabei die volle Reihenfolge neu (`marketplace_reorder`).
- Alle Zeilen sind gleichrangig: jede lässt sich ändern, ausschalten und löschen. Es gibt kein
  Standard-Kennzeichen und keine Zeile, die nicht gelöscht werden dürfte.
- Eine Zeile ohne gültige Adresse (aus einer künftigen Fassung) wird beim Lesen ausgelassen und
  protokolliert, nicht abgelehnt.

## Abgeleitete Ansicht `MarketplaceView` (keine Tabelle)

Aus den Zeilen, sortiert nach `(sort_order, name, id)`: `id`, `name`, `baseUrl`,
`enabled`, `rank`, `authType`, `credentials: none | present | missing | syncing` (R11).

## Flüchtige Typen (nicht gespeichert)

- **CatalogEntry** (bestimmt durch `(source.id, slug)`, nie quellenübergreifend zusammengelegt): `source: { id, name }`, `extensionId`, `name`, `slug`, `shortDescription`, `iconUrl`,
  `verified`, `totalDownloads`, `averageRating`, `reviewCount`, `tags`, `publisher`, `category`,
  `versions[]` (je `version`, `bundleSize`, `compatible`, `incompatibleReason?`), dazu
  `installed: { version, enabled } | null`, `updateTo: string | null`, `latestCompatible: string | null`.
- **CatalogPage**: `entries[]`, `page`, `totalPages`, `total`, `sourceErrors[]`.
- **SourceError**: `{ sourceId, name, kind, detail? }` mit `kind` ∈ `unreachable`, `timeout`,
  `auth_rejected`, `credentials_missing`, `credentials_syncing`, `bad_response`, `insecure_credentials`.
- **PendingBundle** (nur Arbeitsspeicher im Host): `id`, `bytes`, `sourceId`, `slug`, `version`,
  `fetchedAt`, `lastUsedAt`. Höchstens einer; Frei werden siehe R3.
- **DownloadProgress** (Channel): `{ loaded, total }`.
- **KnownUpdate** (Arbeitsspeicher): Kandidaten sind nach `(installedName, sourceId, slug)` getrennt und
  enthalten `{ installed, latestCompatible, sourceId, slug }`. `marketplace_check_updates` und
  Katalogabrufe aktualisieren nur die Kandidaten der antwortenden Quelle für die enthaltenen Einträge;
  eine ausgefallene Quelle behält ihre bisherigen Kandidaten. Die Installationsliste wählt aus den
  bekannten Kandidaten je Erweiterungsname die höchste kompatible Fassung, bei Gleichstand die Quelle
  mit dem besseren Rang. Es gibt keine Abfrage von allein (FR-017).

## Zustandsübergänge des Downloads

```text
idle ──download──▶ fetching-url ──▶ streaming ──▶ verifying ──▶ ready(id)
  ▲                    │              │             │             │
  └──── cancel / error / vault closed / idle 10 min ◀─────────────┘
ready(id) ──preview──▶ ready(id) ──install ok──▶ idle      ready(id) ──discard──▶ idle
```

`verifying` prüft Größe (genau `bundleSize`, höchstens 64 MiB) und SHA-256; schlägt es fehl, geht der
Zustand zu `idle` und der Speicherplatz ist leer. Ein neuer Download ersetzt `ready` und verwirft den alten.
