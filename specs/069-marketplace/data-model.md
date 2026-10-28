# Datenmodell: Marketplace für Erweiterungen

## Tabelle `haex_marketplaces` (Vault-Daten, CRDT-getrackt, synchronisiert)

Migration `0047_marketplaces` (nächste freie Nummer beim Bauen), `HOLZI_TRIGGER_VERSION` + 1. Kein UNIQUE
und kein Fremdschlüssel auf Zugangsdaten (siehe R11, R14).

| Spalte                | Typ                          | Bedeutung                                                                                                |
| --------------------- | ---------------------------- | -------------------------------------------------------------------------------------------------------- |
| `id`                  | TEXT PK                      | zufällige UUID; die feste Kennung `default` für die Überschreibung des Standards (R10)                   |
| `name`                | TEXT                         | Anzeigename; bei `default` leer (der Name kommt aus dem Code)                                            |
| `base_url`            | TEXT                         | Adresse ohne abschließenden `/`; bei `default` leer                                                      |
| `enabled`             | INTEGER NOT NULL DEFAULT 1   | Ein/Aus                                                                                                  |
| `sort_order`          | INTEGER NOT NULL             | Rang, kleiner = früher; bei Gleichstand `(sort_order, name, id)`                                         |
| `auth_type`           | TEXT NOT NULL DEFAULT `none` | `none`, `bearer` oder `basic`                                                                            |
| `credentials_item_id` | TEXT                         | Passwort-Eintrag (Spec 034); Pflicht bei `bearer`/`basic`, sonst NULL; **kein** Geheimnis in der Tabelle |
| `created_at`          | INTEGER NOT NULL             | Unix-Millisekunden                                                                                       |
| `updated_at`          | INTEGER NOT NULL             | Unix-Millisekunden                                                                                       |

**Regeln**

- `base_url` MUSS mit `http://` oder `https://` beginnen und einen Host haben (FR-024); sie wird
  normalisiert (ohne abschließenden `/`, ohne Fragment und Query).
- `auth_type` ≠ `none` verlangt `credentials_item_id` und eine `https://`-Adresse (FR-027); die Prüfung
  sitzt im Host-Command beim Speichern **und** im Client beim Senden.
- Die Zeile `default` darf nur `enabled` und `sort_order` ändern; `name`, `base_url`, `auth_type` bleiben
  leer/`none`. Löschen der Zeile `default` setzt den Standard zurück (eingeschaltet, Rang 1); das Löschen
  des Standards selbst ist nicht möglich (FR-022).
- Eine Zeile ohne gültige Adresse (aus einer künftigen Fassung) wird beim Lesen ausgelassen und
  protokolliert, nicht abgelehnt.

## Abgeleitete Ansicht `MarketplaceView` (keine Tabelle)

Aus Konstante `default` plus Zeilen, sortiert nach `(sort_order, name, id)`: `id`, `name`, `baseUrl`,
`enabled`, `isDefault`, `rank`, `authType`, `credentials: none | present | missing | syncing` (R11).

## Flüchtige Typen (nicht gespeichert)

- **CatalogEntry**: `source: { id, name }`, `extensionId`, `name`, `slug`, `shortDescription`, `iconUrl`,
  `verified`, `totalDownloads`, `averageRating`, `reviewCount`, `tags`, `publisher`, `category`,
  `versions[]` (je `version`, `bundleSize`, `compatible`, `incompatibleReason?`), dazu
  `installed: { version, enabled } | null`, `updateTo: string | null`, `latestCompatible: string | null`.
- **CatalogPage**: `entries[]`, `page`, `totalPages`, `total`, `sourceErrors[]`.
- **SourceError**: `{ sourceId, name, kind, detail? }` mit `kind` ∈ `unreachable`, `timeout`,
  `auth_rejected`, `credentials_missing`, `credentials_syncing`, `bad_response`, `insecure_credentials`.
- **PendingBundle** (nur Arbeitsspeicher im Host): `id`, `bytes`, `sourceId`, `slug`, `version`,
  `fetchedAt`, `lastUsedAt`. Höchstens einer; Frei werden siehe R3.
- **DownloadProgress** (Channel): `{ loaded, total }`.
- **KnownUpdate** (Arbeitsspeicher): `name → { installed, latestCompatible, sourceId }`, gefüllt von jedem
  Katalogabruf und von „Nach Updates suchen“, nie von allein (FR-017).

## Zustandsübergänge des Downloads

```text
idle ──download──▶ fetching-url ──▶ streaming ──▶ verifying ──▶ ready(id)
  ▲                    │              │             │             │
  └──── cancel / error / vault closed / idle 10 min ◀─────────────┘
ready(id) ──preview──▶ ready(id) ──install ok──▶ idle      ready(id) ──discard──▶ idle
```

`verifying` prüft Größe (genau `bundleSize`, höchstens 64 MiB) und SHA-256; schlägt es fehl, geht der
Zustand zu `idle` und der Speicherplatz ist leer. Ein neuer Download ersetzt `ready` und verwirft den alten.
