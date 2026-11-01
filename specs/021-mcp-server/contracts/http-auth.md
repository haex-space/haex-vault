# Contract: HTTP-Weg mit OAuth 2.1

**Feature**: [../spec.md](../spec.md) | **Research**: R1, R2, R14

Für Android, iOS und Clients, die nur HTTP sprechen. holzi ist Ressourcen- und
Autorisierungsserver nach der MCP-Autorisierung.

## Endpunkte (nur Loopback, Port `mcp.http_port`)

| Pfad                                          | Zweck                                                                                                                                                            |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST/GET/DELETE /mcp`                        | Streamable HTTP; ohne gültiges Access-Token `401` mit `WWW-Authenticate` und `resource_metadata`                                                                 |
| `GET /.well-known/oauth-protected-resource`   | Metadaten der Ressource                                                                                                                                          |
| `GET /.well-known/oauth-authorization-server` | Metadaten des Autorisierungsservers                                                                                                                              |
| `POST /oauth/register`                        | Dynamische Client-Registrierung; legt noch keinen Agenten an                                                                                                     |
| `GET /oauth/authorize`                        | Löst den Registrierungsdialog im holzi-Fenster aus; die Seite zeigt nur „In holzi bestätigen“ und wartet                                                         |
| `GET /oauth/authorize/wait?id=…`              | Dieselbe Seite lädt sich ohne Skript jede Sekunde hierüber neu; nach der Entscheidung `302` zur `redirect_uri` mit `code` oder `error=access_denied` und `state` |
| `POST /oauth/token`                           | `authorization_code` mit PKCE (S256 Pflicht) und `refresh_token`                                                                                                 |

- Access-Token: 1 h, nur im Speicher. Refresh-Token: Zugangsnachweis des Agenten (`token_hash`),
  rotiert bei jeder Verwendung.
- `redirect_uri`: nur `http://127.0.0.1:*`, `http://localhost:*` oder eigene Schemata
  nativer Clients; sonst Ablehnung. Bei `/oauth/authorize` MUSS sie zu den für `client_id`
  registrierten URIs passen; bei Loopback-URIs darf nur der Port abweichen. Der akzeptierte Wert
  wird am `authorization_code` gebunden und `/oauth/token` lehnt einen anderen `redirect_uri` ab.
- Ablauf 90 Tage (R9) und Widerruf machen den Refresh-Token ungültig. Ein Access-Token gilt nur,
  solange sein Agent registriert ist (Prüfung je Anfrage an `/mcp`).
- Der Refresh-Token ist an die `client_id` gebunden, für die autorisiert wurde
  (`oauth_client_id`); jede Verwendung gibt einen neuen aus, der alte gilt nicht mehr.
- Der Slug eines HTTP-Agenten kommt aus `client_name` der Client-Registrierung
  (`[a-z0-9-]{1,32}`, sonst `http-agent`). Autorisiert sich ein Client mit demselben Slug erneut,
  beginnt der Dialog mit den Rechten des bisherigen Agenten und ersetzt dessen Nachweis.
- Registrierte Clients liegen nur im Speicher (höchstens 64, der älteste geht zuerst); nach einem
  Neustart registriert sich ein Client neu.
- Eine MCP-Sitzung über HTTP nutzt nur Protokollversionen mit `initialize` (bis 2025-11-25): Sie
  braucht den Handschlag, um Rechte-Änderungen und `tools/list_changed` zu bekommen.

## Abwehr

- `Host` muss `127.0.0.1:<port>`, `[::1]:<port>` oder `localhost:<port>` sein, sonst `421`.
- Anfragen mit `Origin`-Header an `/mcp` und `/oauth/token`: `403` (Browser-Skripte von Webseiten).
  Die Autorisierungsseite selbst wird im Browser geöffnet und braucht keinen `Origin`.
- Höchstens 16 Sitzungen, 1 MiB je Anfrage, 10 offene Autorisierungen.
