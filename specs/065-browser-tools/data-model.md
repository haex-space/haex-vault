# Data Model: Browser-Werkzeuge für Agenten

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

## Rechtegruppe `browser` (in `haex_mcp_agent_grants`, Spec 021)

Keine neue Tabelle und keine neue Spalte: die Zeilen der Gruppe stehen neben `operate` (R8).

| `grant_group` | `grant_key`                         | `level`             | `scope_tags`                      | Bedeutung                         |
| ------------- | ----------------------------------- | ------------------- | --------------------------------- | --------------------------------- |
| `browser`     | `` (leer)                           | `on`                | –                                 | Rechtegruppe eingeschaltet        |
| `browser`     | `user-browser`                      | `on`                | –                                 | Browser des Nutzers erlaubt (US6) |
| `browser`     | Website (`https://shop.example.de`) | `read` \| `operate` | `["fill"]` wenn Anmeldung gemerkt | erlaubte Website                  |

Regeln:

- Fehlt die Zeile mit leerem Schlüssel, ist die Gruppe aus (021 FR-008); andere Zeilen der Gruppe
  wirken dann nicht.
- Website ist ein Ursprung: Schema `https` oder `http`, Host in Kleinbuchstaben (IDN als Punycode),
  Port nur wenn nicht Standard. `http` nur mit `read` (R9).
- `scope_tags = ["fill"]` nur bei `operate`; „Für diese Website merken“ setzt es, Zurücknehmen in
  den Rechten löscht es.
- In Rust: `Rights.browser: Option<BrowserRights>` mit `user_browser: bool` und
  `sites: BTreeMap<Site, SiteGrant { level, fill_remembered }>`. `grants::validate` weist fremde
  Schemata, Zugangsdaten in der Adresse und `http` mit `operate` ab.

## Einstellung des eingebauten Agenten

Geräteeinstellung `agent_browser.builtin` (JSON, Form wie `BrowserRights`), ab Werk nicht
gesetzt = aus. Leitplanke: nur über die Aktion `settings.agentBrowser.builtin.set` änderbar, die
kein Agent aufrufen kann.

## `haex_agent_browser_no_sync` (neu, geräteeigen, ohne Sync)

| Spalte              | Typ     | Regel                             |
| ------------------- | ------- | --------------------------------- |
| `vault_device_uuid` | TEXT PK | Gerät                             |
| `browser_path`      | TEXT    | gewählter Browser (leer = suchen) |
| `created_at`        | INTEGER | Unix-ms                           |

Migration `0047_agent_browser`. Kein Schlüssel in der Tabelle: das Profil liegt in einem
verschlüsselten Ordner aus Spec 048 (R3).

## Profil zwischen zwei Läufen

`<AppLocalData>/agent-browser/<Instanz>/` ist ein gewöhnlicher Geräteordner; darin liegt der
verschlüsselte Ordner „Profil“ (048 US7, Schlüssel aus dem Inhaltsschlüssel der Vault) mit der
Datei `profile.zip`: ZIP (Deflate) der bleibenden Profilteile (R3). Jedes Ende schreibt eine neue
Version der Datei; die vorige entfällt wie beim Überschreiben im Dateibrowser. Lässt sich die
Datei nicht lesen (kaputt, Vault ohne passenden Schlüssel), startet der Browser mit leerem Profil
und holzi meldet es in „KI → Browser“.

## Im Speicher

- **Agenten-Browser**: Prozess, Pipe, Profil-Arbeitsverzeichnis, letzte Sitzung (für die 15
  Minuten aus FR-003).
- **Browser-Sitzung**: `id`, Agent (`builtinAgent` oder `externalAgent { id }`), Browser
  (`agent` | `user`), Agenten-Tabs (≤ 5), Zustand `running` | `paused { by: user | request }` |
  `ended { reason }`, letzte Handlung (Werkzeug, Website, Beschriftung, Zeit).
- **Agenten-Tab**: CDP-Ziel bzw. Tab-Kennung der Erweiterung, Website, Elementnummern →
  `backendNodeId` (bis zur nächsten Änderung des Dokuments), gefüllte Felder (R7).
- **Agenten-Sitzung**: `screenshotsBlocked` wird nach dem ersten Credential-Fill bis zum Ende der
  Sitzung gesetzt und kann nicht durch eine Dokument-Navigation zurückgesetzt werden.
- **Einmal erlaubte Websites**: je Sitzung, verfallen mit ihr (FR-007).
- **Browser-Rückfrage**: Art (`site`, `action` (Manuell vor einer Navigation), `submit`, `fill`, `takeover`, `download`, `newTab`),
  Agent, Website, Inhalt je Art (data-model der Ereignisse in
  [contracts/tauri-commands.md](./contracts/tauri-commands.md)), Frist 5 Minuten.

## Protokoll

Einträge im Agenten-Protokoll aus 021 (`haex_mcp_agent_log_no_sync`): `tool` = Werkzeugname,
`target` = Website und Beschriftung des Ziels („shop.example.de · Jetzt kaufen“), `input` =
Eingaben ohne Geheimnisse und ohne eingetragene Anmeldedaten. Für den eingebauten Agenten gilt das
Protokoll des Chats (Spec 003). Neue Ergebnisse: `denied_site` (Website abgelehnt), `paused`
(Nutzer bedient). Seiteninhalte und Bildschirmfotos nie.
