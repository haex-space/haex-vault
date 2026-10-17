# Contract: Lokaler Medienserver (044)

Vorlage: haex-vault 2.x (Legacy) `src-tauri/src/media_server/mod.rs` (Revision
`fc4e84b61a050576ba42e0dc832d04064a8605a3`). Läuft im Prozess der Vault-Sitzung und endet mit
`gate.token()`.

## Adresse

`http://127.0.0.1:<port>/<token>` mit zufälligem Port (Bind auf `127.0.0.1:0`) und Token als UUIDv4.
Jede andere Adresse, auch `/`, antwortet `404`. Der Server lauscht nie auf anderen Schnittstellen.

## Anfragen

| Methode   | Kopfzeilen der Anfrage         | Antwort                                    |
| --------- | ------------------------------ | ------------------------------------------ |
| `GET`     | ohne `Range`                   | `200`, ganzer Inhalt gestreamt             |
| `GET`     | `Range: bytes=N-M`, `N-`, `-N` | `206` mit `Content-Range: bytes N-M/total` |
| `GET`     | ungültiger Bereich             | `416` mit `Content-Range: bytes */total`   |
| `GET`     | mehrere Bereiche               | `416` (nicht unterstützt)                  |
| `HEAD`    | –                              | wie `GET` ohne Körper                      |
| `OPTIONS` | –                              | `204` mit den CORS-Kopfzeilen              |

Immer gesetzt: `Accept-Ranges: bytes`, `Content-Type` (aus der Endung), `Content-Length`,
`Cache-Control: no-store`, `Access-Control-Allow-Origin: *`,
`Access-Control-Allow-Headers: Range`,
`Access-Control-Expose-Headers: Accept-Ranges, Content-Range, Content-Length`. Kein
`Content-Encoding` (sonst schaltet pdf.js Range ab).

## Speicher

Gelesen wird in Stücken von höchstens 256 KiB; eine Antwort hält nie mehr als ein Stück im Speicher
(FR-013). Quellen: lokale Datei (`seek` + `read`) und S3 (`get_range` je Anfrage).

## Lebenszyklus der Tokens

| Ereignis                     | Wirkung                                        |
| ---------------------------- | ---------------------------------------------- |
| `files_open`                 | neues Token für genau eine Datei und einen Tab |
| `files_release(url)`         | Token entfernt                                 |
| `files_release_tab(tabId)`   | alle Tokens des Tabs entfernt                  |
| Vault gesperrt / Prozessende | Server und alle Tokens weg (ADR 0003)          |

## CSP und Plattform

- `tauri.conf.json`: `media-src 'self' http://127.0.0.1:*`; `http://127.0.0.1:*` zusätzlich in `img-src`
  und `connect-src`.
- Android: `network_security_config.xml` erlaubt Cleartext nur für `127.0.0.1`.
- Probe vor allem anderen: Wiedergabe mit Springen unter Windows (WebView2) und Android (LNA-Risiko,
  research R4).
