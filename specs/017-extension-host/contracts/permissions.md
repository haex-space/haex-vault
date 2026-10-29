# Vertrag: Berechtigungen

Begründung: [research.md](../research.md) R15, R18–R21. Modul `src-tauri/src/extensions/permissions/`
(rein, ohne Datenbank testbar).

## Arten, Aktionen, Ziele

| Art (`kind`)    | Aktionen                   | Ziel                                                                                                         | Abgleich                                                                    | Geltungsbereich gemerkt |
| --------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------- | ----------------------- |
| `database`      | `read`, `readWrite`        | Präfix einer Erweiterung (`<pk>__<name>__*`) oder eine ihrer Tabellen                                        | exakt zerlegt (sql-policy)                                                  | vault-weit              |
| `filesystem`    | `read`, `readWrite`        | Pfad (Ordner mit Unterordnern) oder Datei                                                                    | Präfix aus ganzen Pfadteilen (`Path::starts_with`) auf dem aufgelösten Pfad | vault-weit              |
| `web`           | HTTP-Methode oder `*`      | `*`, `schema://host/pfad*`, `*.domain`, Domain                                                               | wie HV `manager/url.rs`, jede Weiterleitung neu                             | vault-weit              |
| `notifications` | `show`                     | `*`                                                                                                          | –                                                                           | vault-weit              |
| `passwords`     | `read`, `readWrite`        | Tag oder `*`                                                                                                 | über 034 (`Grant`, `Scope`)                                                 | vault-weit              |
| `remoteStorage` | `read`, `readWrite`, `add` | Kennung des Speichers (Spec 038) oder `*`; bei `add` der Host eines Endpunkts (`host:port`, `host`) oder `*` | exakt; bei `add` wie `mail`                                                 | vault-weit              |
| `mail`          | `fetch`, `send`, `poll`    | `*`, `host:port` oder `host` (alle Ports)                                                                    | Host ohne Rücksicht auf Groß-/Kleinschreibung                               | vault-weit              |
| `shell`         | `execute`                  | Programm (kanonischer Pfad) oder `*`                                                                         | exakt nach Auflösung                                                        | **Gerät**, ohne Wahl    |

- `readWrite` deckt `read`, nicht `add` (Spec 038 FR-009b: einen Endpunkt vorschlagen).
- Manifest-Schreibweisen werden vereinheitlicht: `readWrite`/`read_write`, `http`/`web`,
  `cloudStorage`/`remoteStorage`, Feld `operation` oder `action`.
- Kategorien im Manifest, die holzi nicht anbietet (`spaces`, `identities`, `bookmarks`, `syncServers`,
  `syncRules`), zeigt der Installationsdialog als „wird von holzi nicht unterstützt“; sie werden nie gespeichert.
- Ein `database`-Ziel, das kein Präfix einer Erweiterung ist (etwa `chat_*`), wird beim Speichern abgelehnt.

## Auswertung

Für eine Anfrage (Erweiterung, Art, Aktion, Ziel) auf Gerät D:

1. Kandidaten: gemerkte Zeilen mit `vault_device_uuid ∈ {Nil, D}` und vorläufige im Speicher, deren Ziel passt
   und deren Aktion die angefragte deckt.
2. Ein Kandidat mit `denied` → **verweigert** (1002).
3. sonst einer mit `granted` → **erlaubt**.
4. sonst (`ask` oder keiner) → **Anfrage** (1004).

Feste Regeln vor der Auswertung (keine Berechtigung ändert sie): Kerntabellen (sql-policy), gesperrte Pfade
(R19), Funktionen, die es für Erweiterungen nicht gibt (bridge.md), deaktivierte Erweiterung (8002).

## Installation

- Jede erklärte Berechtigung erscheint mit Art, Aktion, Ziel; Vorgabe angehakt. Ein Ziel `*` (außer bei
  `notifications`, deren einziges Ziel es ist) und die Web-Aktion `*` stehen ausgeschrieben und als Warnung
  hervorgehoben da („Alle Dateien und Ordner“, „alle Anfragen“), ebenso in der Anfrage zur Laufzeit und in den
  Einstellungen.
- Der Dialog gruppiert nach Art (FR-067), die Agent-Werkzeuge als eigene Gruppe; jede Gruppe ist eine
  Ziehharmonika, zu Beginn zugeklappt, mit Zähler „angehakt von gesamt“. Eine Gruppe mit einem Ziel `*` (nach
  den Regeln oben) oder der Web-Aktion `*` trägt in der Kopfzeile ein Warndreieck.
- Angehakt → `granted`, abgehakt → `ask`; beides mit `declared = 1`.
- `shell` bekommt die Kennung dieses Geräts, jede andere Art die vault-weite (`PermissionKind::scope_on`). Der
  Dialog bietet keine Wahl; er sagt, dass die Erweiterung in der ganzen Vault installiert wird, und bei `shell`,
  dass die Berechtigung nur auf diesem Gerät gilt.
- Update: nur neu erklärte Berechtigungen werden vorgelegt. Gemerkte Zeilen mit `declared = 1`, deren
  (Art, Aktion, Ziel) das neue Manifest nicht mehr erklärt, werden gelöscht; erklärt ein späteres Update sie
  wieder, gelten sie als neu und werden wieder vorgelegt. Zeilen mit `declared = 0` (zur Laufzeit gemerkt)
  bleiben; erklärt das neue Manifest eine davon, wird sie `declared = 1` mit unverändertem Zustand. Alle
  übrigen Zeilen bleiben unverändert.

## Anfrage zur Laufzeit

1. Rust antwortet der Erweiterung 1004 mit `{resourceType, action, target}` und sendet an die Oberfläche
   `extension-permission-request` mit `requestId`, `extensionId`, `displayName`, `kind`, `action`,
   `target`, `declared`, `deviceScoped` und `targetMissing` (Ziel einer nicht installierten Erweiterung:
   nur „Verweigern“, FR-062).
2. Die Warteschlange im Frontend fasst gleiche (`extensionId, kind, action, target`) zusammen und zeigt eine
   Anfrage nach der anderen: „Erlauben“, „Verweigern“, „Merken“ mit dem Hinweis, wo die gemerkte Entscheidung
   gilt (`deviceScoped`: nur dieses Gerät, sonst alle Geräte); bei `shell` eine deutliche Warnung. Schließen des Dialogs
   bricht ab, verweigert nicht: `extension_permission_cancel {requestId}` nimmt die Frage aus den offenen,
   der nächste gleiche Aufruf fragt wieder.
3. Die Entscheidung geht an `extension_permission_resolve`; Rust speichert (gemerkt) oder hält im Speicher
   (vorläufig) und sendet `extension:permission-resolved` an alle Rahmen der Erweiterung; das SDK wiederholt.
4. Sind alle wartenden Rahmen geschlossen, verschwindet die Anfrage: Rust sendet
   `extension-permission-request-cancelled {requestId}`.

## Einstellungen (Kategorie „Erweiterungen“)

Je Erweiterung: Berechtigungen mit Zustand, Geltungsbereich (vault-weit oder Gerät mit Namen), „erklärt“ oder
„nicht erklärt“; Ändern des Zustands, Widerrufen. Den Geltungsbereich wählt niemand: Eine Zeile, deren Bereich zu
ihrer Art passt, behält ihn (eine Shell-Zeile eines anderen Geräts bleibt dort); eine ältere Zeile, deren Bereich
nicht passt, ersetzt `extension_permission_set` über `replaces` durch eine im Bereich der Art. Vorläufige
Berechtigungen dieses Geräts mit „Entfernen“. Jede Änderung gilt sofort; der Cache in Rust wird verworfen.
