# Data Model: Browser-Werkzeuge für Agenten

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

## Rechtegruppe `browser` (in `haex_mcp_agent_grants`, Spec 021)

Keine neue Tabelle und keine neue Spalte: die Zeilen der Gruppe stehen neben `operate` (R8).

| `grant_group` | `grant_key`                         | `level`                          | `scope_tags`                      | Bedeutung                             |
| ------------- | ----------------------------------- | -------------------------------- | --------------------------------- | ------------------------------------- |
| `browser`     | `` (leer)                           | `on`                             | –                                 | Rechtegruppe eingeschaltet            |
| `browser`     | `user-browser`                      | `on`                             | –                                 | Browser des Nutzers erlaubt (US6)     |
| `browser`     | Website (`https://shop.example.de`) | `read` \| `operate` \| `blocked` | `["fill"]` wenn Anmeldung gemerkt | erlaubte oder gesperrte Website       |
| `browser`     | `*`                                 | `read` \| `operate`              | –                                 | level for all other websites (FR-043) |
| `browser`     | `request:<Website>`                 | `allow` \| `deny`                | –                                 | rule for page requests (FR-044)       |

Regeln:

- Fehlt die Zeile mit leerem Schlüssel, ist die Gruppe aus (021 FR-008); andere Zeilen der Gruppe
  wirken dann nicht.
- Website ist ein Ursprung: Schema `https` oder `http`, Host in Kleinbuchstaben (IDN als Punycode),
  Port nur wenn nicht Standard. `http` nur mit `read` (R9).
- `scope_tags = ["fill"]` nur bei `operate`; „Für diese Website merken“ setzt es, Zurücknehmen in
  den Rechten löscht es.
- Revision 2026-11-03: no row `*` means no level for all (`none`); `Site::parse` reads neither
  `*` nor `request:…`, so an older build skips both rows and only grants less (021 FR-008). The
  key `request:<Website>` holds the website the request goes to, not the tab's. `http` under `*`
  is only `read`. "Für diese Website merken" of a fill on a website operated only through `*`
  lists that website at `operate` with `["fill"]`, so the fill stays when `*` is lowered later.
- Revision 2026-11-05 (FR-047): a website row can have the level `blocked`: the website never
  loads (`check_document` answers `site_blocked`, also with `all` and a once-allowed website),
  and the held requests of a page to it (fetch, XHR, beacons) fail with `site_blocked` before any
  request rule is read.
  The row `*` is never `blocked`: `validate` refuses it, a stored one reads as none. A build before
  this revision cannot parse `blocked`, skips the row and so does not block the website; the
  other rows stay as they are.
- In Rust: `Rights.browser: Option<BrowserRights>` mit `user_browser: bool` und
  `sites: BTreeMap<Site, SiteGrant { level, fill_remembered }>`, `all: Option<SiteLevel>` und
  `requests: BTreeMap<Site, RequestRule>` (`allow` | `deny`). `grants::validate` weist fremde
  Schemata, Zugangsdaten in der Adresse und `http` mit `operate` ab.

## Einstellung des eingebauten Agenten

Geräteeinstellung `agent_browser.builtin` (JSON, Form wie `BrowserRights`), ab Werk nicht
gesetzt = aus. Leitplanke: nur über die Aktion `settings.agentBrowser.builtin.set` änderbar, die
kein Agent aufrufen kann.

Geräteeinstellung `agent_browser.builtin_passwords` (JSON, Form wie `PasswordRights`: `level`,
`tags`, `defaultTag`), ab Werk nicht gesetzt = aus: die Einträge, aus denen holzi für den
eingebauten Agenten eintragen darf (2026-11-02). Leitplanke: nur über
`settings.agentBrowser.builtinPasswords.set`.

## `PasswordRights.defaultTag` (021, erweitert)

Das Tag, das ein neuer Eintrag eines Agenten bekommt (`browser.credentials.create`, FR-036).
Pflicht bei `level = readWrite` mit gewählten `tags` und dann eines davon (`InvalidRights::
DefaultTag`); bei allen Einträgen frei wählbar. Gespeichert im `grant_key` der Zeile der Gruppe
`passwords` in `haex_mcp_agent_grants` (leer = keines); ältere Builds lesen den Schlüssel dieser
Gruppe nicht und sehen dieselben Rechte ohne Tag.

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

## Filter lists and the tracker switch (FR-045)

The choice is the vault-wide `preferences` row `agentBrowser.trackers` (spec 084, so it syncs and
belongs to the vault; before that it was the file `settings.json` next to the downloaded lists). Its
value is JSON with `enabled`, `off` (ids of default lists) and `custom` (a list of objects with `id`,
`name`, `url` and `enabled`); no row means the defaults. The default lists of uBlock
Origin cannot be removed, only switched off by their id; own lists have an `https` address, a name
(the host when empty), an id holzi gives them and can be changed and removed. The addresses of the
default lists live in the code, not in the setting, so an update of holzi may change them.

The downloaded lists are files in `<AppLocalData>/filter-lists/` (spec 084; one file per list; its
modification time is the time of the download, the error of the last download is kept in memory),
device-local, without sync and without secrets; when they are missing, holzi downloads them at the next start.

## DNS over HTTPS (FR-048)

The choice is the `preferences` row `agentBrowser.dns` of the device (spec 084: it lies in the vault,
but only this device reads it; before that it was the file `agent-browser/dns.json`; no secrets): `{ "mode": "automatic" | "secure" | "off", "provider": "quad9" | "cloudflare" | "google"
| "mullvad" | "custom", "custom": "<https template>" }`. No row means automatic with Quad9. The
templates of the presets live in the code, not in the file, so an update of holzi may change them;
`custom` counts only with the provider `custom` and must be an `https` address with a host, no user
info and at most 2048 characters. At the browser's start holzi writes the result into the `Local
State` file of the working directory (`dns_over_https.mode`, `dns_over_https.templates`); the saved
profile's own copy of that key is replaced, every other key stays.

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
- **Browser-Rückfrage**: Art (`site`, `action` (Manuell vor einer Navigation), `submit`, `request`, `fill`, `takeover`, `download`, `newTab`),
  Agent, Website, Inhalt je Art (data-model der Ereignisse in
  [contracts/tauri-commands.md](./contracts/tauri-commands.md)), Frist 5 Minuten.

## Protokoll

Einträge im Agenten-Protokoll aus 021 (`haex_mcp_agent_log_no_sync`): `tool` = Werkzeugname,
`target` = Website und Beschriftung des Ziels („shop.example.de · Jetzt kaufen“), `input` =
Eingaben ohne Geheimnisse und ohne eingetragene Anmeldedaten. Für den eingebauten Agenten gilt das
Protokoll des Chats (Spec 003). Neue Ergebnisse: `denied_site` (Website abgelehnt), `paused`
(Nutzer bedient). Seiteninhalte und Bildschirmfotos nie.
