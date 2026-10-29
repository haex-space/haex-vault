# Marketplace-Protokoll v1 (normativ)

Dieser Vertrag beschreibt, was ein **Marketplace** liefern muss, damit HaexVault ihn nutzen kann. Er gilt für
jeden Betreiber gleich: den öffentlichen Marketplace des Projekts, firmeneigene und selbst gehostete.
HaexVault setzt über einen Marketplace nichts voraus außer diesem Vertrag (FR-034).

Herkunft: `@haex-space/marketplace-sdk` 0.1.6 und der Dienst `marketplace.haex.space`, am 2026-10-28 mit
lesenden Aufrufen bestätigt; ab 069 ist **dieser Text** maßgeblich, nicht der Dienst. Der Host liest tolerant:
unbekannte Felder werden ignoriert (FR-035), ein Eintrag ohne Pflichtfeld wird ausgelassen (nicht die ganze
Antwort).

## Anforderungen an einen Betreiber

- **Adresse**: `https://` (empfohlen) oder `http://` (nur im lokalen Netz sinnvoll), mit beliebigem
  Pfadanteil. Alle Pfade dieses Vertrags werden **hinter** den Pfadanteil gehängt
  (`https://example.org/market` → `https://example.org/market/extensions`).
- **Antworten** sind JSON (`Content-Type: application/json`) in `camelCase`; Zeichensatz UTF-8.
- **Anmeldung** (optional): Bearer-Token oder Basic im `Authorization`-Kopf; HaexVault sendet sie nur über
  `https://` und nur an diesen Host (FR-027). Ein Marketplace, der sie verlangt, antwortet ohne gültige
  Anmeldung mit `401` oder `403`.
- **Download-Adresse** (`downloadUrl`) darf auf einem anderen Host liegen (Objektspeicher, signierte
  Adresse); sie wird ohne Anmeldung geladen und MUSS mindestens so lange gelten, wie der Aufruf dauert (der
  öffentliche Dienst: 300 s).
- **Prüfsumme und Größe** jeder Fassung sind Pflicht (`bundleHash`, `bundleSize`), sonst ist sie nicht
  installierbar (FR-013).
- **Keine Anforderung** an Antwortzeit über eine Frist von 8 s je Aufruf hinaus, an CORS (der Host ruft ab,
  nicht ein Browser) oder an eine feste Adresse.

## Anfragen

Alle Anfragen sind `GET`, `Accept: application/json`, mit `User-Agent: HaexVault/<Fassung>`,
`X-Platform: linux|windows|macos|android|ios` und `X-App-Version: <Fassung>`. Sonst **keine** Kopfzeile, die
Nutzer, Vault oder Gerät erkennbar macht (FR-007). Bei `auth_type` ≠ `none` zusätzlich
`Authorization: Bearer <token>` oder `Basic <base64>`, nur über `https://` und nur an den Host der
Marketplace-Adresse (FR-027).

| Aufruf                                        | Zweck                          | Antwort                                            |
| --------------------------------------------- | ------------------------------ | -------------------------------------------------- |
| `/health`                                     | Erreichbarkeit (Formular-Test) | `{ "status": "ok" }`                               |
| `/categories`                                 | Kategorien                     | `{ categories: [CategoryWithCount] }`              |
| `/extensions?page&limit&category&search&sort` | Katalogseite                   | `{ extensions: [ListItem], pagination }`           |
| `/extensions/{slug}`                          | Details                        | `ExtensionDetail`                                  |
| `/extensions/{slug}/download[?version]`       | Download-Adresse einer Fassung | `{ downloadUrl, version, bundleSize, bundleHash }` |

`sort` ∈ `downloads` (Vorgabe von HaexVault), `rating`, `newest`, `updated`. `limit` = 20.

Die Antwortformen und Feldtypen sind verbindlich:

```ts
type HealthResponse = { status: 'ok' }
type CategoryWithCount = { name: string; slug: string; count: number }
type CategoriesResponse = { categories: CategoryWithCount[] }
type Pagination = {
  page: number
  limit: number
  total: number
  totalPages: number
}
type ListResponse = { extensions: ListItem[]; pagination: Pagination }
type DownloadResponse = {
  downloadUrl: string
  version: string
  bundleSize: number
  bundleHash: string
}
```

`page` ist 1-basiert. `total` zählt alle Treffer nach Filtern; `totalPages` ist `ceil(total / limit)` und
bei null Treffern `0`. `bundleSize` ist eine nichtnegative ganze Bytezahl; `bundleHash` ist ein
64-stelliger SHA-256-Hexwert. Ein nicht vorhandenes optionales Feld wird ausgelassen oder als `null`
geliefert. Unbekannte Felder sind zulässig.

## Felder, die HaexVault liest

**ListItem**: `id: string`, `extensionId: string` (`<Herausgeber-Slug>/<Name>`), `name: string`,
`slug: string`, `shortDescription: string`, `iconUrl?: string`, `verified: boolean`,
`totalDownloads: number`, `averageRating?: number`, `reviewCount: number`, `tags?: string[]`,
`publishedAt?: string`, `publisher?: { displayName: string; slug: string; verified: boolean }`,
`category?: { name: string; slug: string }`, `versions: Version[]`.

**Version**: `version: string` (semver), `changelog?: string`, `bundleSize?: number` (Byte),
`bundleHash?: string` (SHA-256, 64 Hex-Zeichen), `permissions?: string[]` (nur Anzeige:
**maßgeblich sind die erklärten Berechtigungen im Bundle**, Spec 017), `minAppVersion?: string`,
`maxAppVersion?: string`, `publishedAt?: string`.

**ExtensionDetail**: `ListItem` plus `publicKey: string` (Herausgeberschlüssel), `description: string`,
`author?: string`, `latestVersion?: string`.

**Pflichtfelder je Eintrag**: `extensionId`, `name`, `slug`, `shortDescription`, `versions` (darf leer sein;
dann ist nichts installierbar). Für die Installation einer Fassung müssen `bundleHash` und `bundleSize`
vorliegen; fehlt eines, bleibt die Fassung sichtbar, aber nicht installierbar.

## Download

1. `GET /extensions/{slug}/download?version=<v>` liefert `downloadUrl` (signiert, kurzlebig, **auf einem
   anderen Host möglich**), `bundleSize`, `bundleHash`.
2. HaexVault lädt `downloadUrl` **sofort** und **ohne** `Authorization`, gestreamt, bis höchstens 64 MiB
   und höchstens `bundleSize` Bytes; mehr Bytes oder weniger Bytes sind ein Fehler.
3. SHA-256 der Bytes MUSS `bundleHash` entsprechen (Groß-/Kleinschreibung egal); ohne `bundleHash`
   wird nicht installiert (FR-013).
4. Die Bytes gehen in die Installations-Vorschau (Spec 017); deren Signaturprüfung ist unabhängig.

## Fehler

Jeder Status ≥ 400 ist ein Fehler der Quelle: `401/403` → `auth_rejected`; Zeitüberschreitung →
`timeout`; Verbindungsfehler, TLS, DNS → `unreachable`; kein gültiges JSON oder fehlende Pflichtfelder der
Antwort → `bad_response`. Der Client verwirft den Antworttext des Dienstes vollständig: er kann ein
Echo von Zugangsdaten enthalten. Fehlerdetails enthalten nur lokal erzeugte, bereinigte Angaben wie
Statuscode oder Fehlerart; niemals Antworttext, vollständige URL, Anfragekopfzeilen oder Zugangsdaten.

## Bilder

`iconUrl` lädt der Host (R12): ohne Zugangsdaten, nur `http` oder `https`, `image/png|jpeg|webp`, ≤
256 KiB (Symbole) bzw. ≤ 1 MiB (Screenshots), **kein SVG**. Jedes Ziel muss öffentlich routbar sein,
auch wenn es auf dem konfigurierten Marketplace-Origin liegt. Private, reservierte, Loopback-,
Link-Local-, Multicast- und nicht spezifizierte IP-Adressen sind gesperrt. Eine URL auf einem
anderen Origin muss `https` sein. Der öffentliche Speicher `api.marketplace.haex.space` bleibt
zulässig. Weiterleitungen werden einzeln nach denselben Regeln geprüft und dürfen nicht von `https`
auf `http` wechseln. DNS-Adressen werden für die Verbindung validiert und festgehalten, damit die
Prüfung nicht durch DNS-Rebinding umgangen wird.
