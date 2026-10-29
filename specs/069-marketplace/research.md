# Research: Marketplace für Erweiterungen

Alle Punkte der Technical Context sind entschieden; es bleibt kein `NEEDS CLARIFICATION`. Quellen:
`@haex-space/marketplace-sdk` 0.1.6 (Typen und Pfade, aus dem npm-Paket gelesen), der Live-Dienst
`https://marketplace.haex.space` (nur lesende Aufrufe am 2026-10-28), haex-vault 2.x @ `8dce379d` und der
holzi-Code (Verweise je Punkt).

## R1 – Die Schnittstelle des Dienstes

**Entscheidung**: HaexVault spricht dieselbe Schnittstelle wie das SDK 0.1.6, direkt unter der
Marketplace-Adresse (kein `/v1`): `GET /health`, `/categories`, `/extensions?page&limit&category&search&sort`,
`/extensions/{slug}`, `/extensions/{slug}/download[?version]`, `/extensions/{slug}/versions`. Antworten sind
JSON in `camelCase`. Die Felder, die HaexVault braucht, stehen in
[contracts/marketplace-api.md](contracts/marketplace-api.md).

**Am Live-Dienst bestätigt** (2026-10-28):

- `bundleHash` ist SHA-256 in Hex (64 Zeichen) und stimmte für `haex-tetris 0.1.3` mit der geladenen
  Datei überein; `bundleSize` stimmte auf das Byte.
- `downloadUrl` ist eine **signierte, kurzlebige Adresse auf einem anderen Host**
  (`api.marketplace.haex.space/storage/v1/object/sign/…`, Gültigkeit 300 s laut Token). Sie wird sofort
  nach dem Abruf geladen und nie zwischengespeichert; es wird kein `Authorization` an diesen Host gesendet.
- `iconUrl` liegt ebenfalls auf einem anderen Host (öffentlicher Speicher).
- `extensionId` hat die Form `<Herausgeber-Slug>/<Name>` (`haex/haex-tetris`), ist **nicht** der
  Herausgeberschlüssel. Der Schlüssel (`publicKey`) steht nur in den Details.
- `versions` im Listenaufruf kommen neueste zuerst, `publishedAt` der Erweiterung ist oft `null`. Die
  neueste Fassung wird deshalb mit `semver` bestimmt, nicht über die Reihenfolge (Legacy nahm `versions[0]`).
- Die Größe eines Bundles liegt in der Praxis zwischen 1,7 MiB und 24 MiB; die Grenze von 64 MiB passt.

**Das Protokoll gehört diesem Repository** (Entscheidung des Betreibers, 2026-10-28): Der Betreiber stellt
einen öffentlichen Marketplace bereit, und Nutzer dürfen selbst einen hosten. Darum ist
[contracts/marketplace-api.md](contracts/marketplace-api.md) ab 069 der **normative Vertrag** („Marketplace-
Protokoll v1“) und nicht mehr eine Beschreibung des haex-space-Dienstes; Pflicht- und optionale Felder,
Fehlerverhalten und Anforderungen an Betreiber (TLS, Pfadanteil, Anmeldung) stehen dort. Der Host kennt
keine Adresse und keinen Betreiber außer als Zeile in der Liste.

**Alternativen**: das SDK im Frontend nutzen (verworfen: zweiter HTTP-Pfad, Entscheidung des Betreibers);
eine andere Schnittstelle erfinden (verworfen: Dienst und Legacy sprechen diese).

**Risiko**: Der Dienst ist ein Live-Endpunkt, kein fester Stand (Constitution IV gilt für Harness-Inhalt,
nicht dafür). Gegenmaßnahme: Der Host liest tolerant (unbekannte Felder ignorieren, Pflichtfelder je Eintrag
prüfen) und der Vertrag steht im Repo, mit Testdaten aus der Live-Antwort (Zufallswerte ersetzt).

## R2 – Wo die Typen herkommen

**Entscheidung**: Die Typen werden **in Rust** nachgebaut (`marketplace/model.rs`, `serde`, nur die
benötigten Felder) und per `ts-rs` ins Frontend gebracht (wie alle Bindings, `src/types/bindings/`). Das
`marketplace-sdk` wird **nicht** als Abhängigkeit aufgenommen.

**Begründung**: Der Client im SDK besteht aus einer Datei und einer `fetch`-Schleife; die Typen sind
klein. Eine eigene Umsetzung in Rust vermeidet ein zweites Typmodell und die Abhängigkeit vom Paket.

**Alternativen**: SDK nur für Typen (verworfen: bindet das Frontend an eine Fassung, die der Host gar nicht
benutzt).

## R3 – Wo der geladene Bundle liegt

**Entscheidung**: Im Arbeitsspeicher des Hosts, in genau einem Platz (`PendingBundle { id, bytes, source,
name, version }`, ≤ 64 MiB). Frei wird er bei: Abbrechen/Schließen der Vorschau, erfolgreicher Installation,
neuem Download, Schließen des Vaults, nach 10 Minuten ohne Zugriff. Der Webview bekommt nur die `id`.

**Begründung**: „Nichts bleibt auf der Platte“ (Spec US1-6) ohne Aufräumlogik für Temp-Dateien, auch nicht
nach einem Absturz; die Bytes erreichen das Webview nie (Legacy kopierte sie als `Array.from(Uint8Array)`
durch die IPC, bei 24 MiB eine Verdopplung und eine Falle). 64 MiB sind auf Android tragbar.

**Alternativen**: Temp-Datei in `app_cache_dir` (verworfen: Rest nach Absturz, Pfad-Behandlung auf
Android); Bytes ins Webview und zurück (verworfen, s. o.).

## R4 – Wie die Installation die Quelle bekommt

**Entscheidung**: `extension_install_preview` und `extension_install` ersetzen `file: PickedFile` durch
`source: BundleSource` (`{ kind: "file", file }` oder `{ kind: "marketplace", id }`). Eine Funktion
`read_bundle(source)` in `extensions/registry/install.rs` liefert die Bytes (Datei wie bisher über
`read_bundle_file`, sonst aus dem Speicherplatz); danach läuft derselbe `install_preview` und `install`.
`InstallDialog.vue` bekommt `source` und ruft damit dieselben zwei Commands.

**Begründung**: Eine Implementierung pro Regel; es gibt keine zweite Installationsroutine und keinen
zweiten Dialog. Alle Aufrufer werden gesucht und geändert (siehe Aufgabe „jede Stelle“, Memory
`holzi-every-call-site`): `InstallDialog.vue`, Dev-Laden (`extension_dev_load` bleibt Datei), e2e-Skripte
und alle Stellen mit `extension_install`.

**Alternativen**: zwei neue Commands `marketplace_install_*` (verworfen: dupliziert die Argumente und die
Bestätigungslogik); ein künstlicher `PickedFile` wie `pending:<id>` (verworfen: schmuggelt eine Quelle in
einen Typ, der „vom Nutzer gewählte Datei“ heißt).

## R5 – Frist, Parallelität, Fortschritt

**Entscheidung**:

- Katalog: alle Quellen parallel (`futures::future::join_all`), je Quelle Gesamtfrist **8 s** (Verbindung
  und Antwort). Eine Quelle, die nicht fertig wird, zählt als „Zeitüberschreitung“, die anderen warten
  nicht länger. `marketplace_catalog` ist abbrechbar durch ein neueres Aufrufen (Suche tippen).
- Download: Verbindungsfrist 10 s, **Leerlauffrist 20 s** zwischen zwei Datenstücken, keine feste
  Gesamtfrist (große Dateien, langsame Netze); Fortschritt über einen `Channel<DownloadProgress>`
  (Bytes geladen, Gesamtgröße) höchstens alle 100 ms. Abbruch über `marketplace_download_cancel(id)`.
- Frist und Leerlauffrist werden in `mod.rs` als Konstanten geführt und in Tests über eine Uhr bzw. einen
  einstellbaren Wert verkürzt (kein Warten in Tests).

**Alternativen**: Gesamtfrist für den Download (verworfen: bricht legitime langsame Downloads ab).

## R6 – Anmeldung, Weiterleitungen, `http`

**Entscheidung** (FR-027):

- Ein `reqwest::Client` je Marketplace-Aufruf-Art, `redirect::Policy::custom`: Weiterleitungen werden
  **nur gefolgt, wenn Schema-Herabstufung (`https`→`http`) ausgeschlossen ist** und, sobald Zugangsdaten
  im Spiel sind, nur innerhalb desselben Hosts und Ports. Bis zu 5 Weiterleitungen. `reqwest` entfernt
  `Authorization` bei Hostwechsel ohnehin; der Test prüft es trotzdem (kein Vertrauen auf Standardwerte).
- Zugangsdaten werden nur an `https://` gesendet; das Formular lehnt „Bearer/Basic mit `http://`“ ab
  (FR-027, US5-6) und der Client sendet sie auch dann nicht, wenn die Zeile es erlaubt (zweite Schranke).
- Die signierte Download-Adresse und Symbole werden **ohne** Zugangsdaten geladen. Eine
  `http://`-Download-Adresse ist nur erlaubt, wenn die Adresse des Marketplace selbst `http://` ist
  (selbst gehostet im lokalen Netz); sonst wird abgelehnt.
- Die Weiterleitungslogik aus `extensions/web.rs` wird **nicht** wiederverwendet: sie ist an die
  Berechtigungsprüfung für Erweiterungen gebunden (Frage vor jeder Weiterleitung). Ein gemeinsamer Kern
  wäre eine Abstraktion ohne zweiten echten Nutzer (Rust-Regel der Constitution).

**Rest-Risiko**: Ein bösartiger Marketplace könnte eine Download-Adresse in ein internes Netz setzen
(SSRF). Die Antwort wird nur gehasht und als Bundle geprüft, sie erreicht den Nutzer nicht; es werden keine
Zugangsdaten gesendet. Akzeptiert und im Vertrag vermerkt.

## R7 – Mehrere Marketplaces gleichzeitig: Anordnung und Blättern

**Entscheidung**: Alle eingeschalteten Quellen werden gleichzeitig befragt. Seite N ist die Seite N
**jeder** Quelle (mit bis zu 20 Einträgen je Quelle), hintereinander in Rangfolge. Die gemeinsame Liste
wird ausdrücklich **nicht** noch einmal quellenübergreifend nach Downloads sortiert: Das würde die
Rangfolge aufheben und bei Änderungen an den Downloadzahlen Einträge zwischen Quellen und Seiten
verschieben. Innerhalb jeder Quelle gilt die Sortierung nach Downloads absteigend. Einträge verschiedener
Quellen werden **nie** zusammengelegt (FR-012): Ein Eintrag ist durch `(sourceId, slug)` bestimmt, die
Kachel nennt die Quelle, ein Filter „Quelle“ (`sourceId` im Aufruf) beschränkt auf eine. `totalPages` ist
das Maximum der Quellen, `total` die Summe. Das ist stabil (kein Eintrag verschwindet beim Blättern).

**Begründung**: Der Legacy-Weg (Dopplung nach `extensionId`, kleinste Rangnummer gewinnt) lässt eine
beliebige, selbst gehostete Quelle den Eintrag einer anderen verdecken, denn `extensionId` ist eine
Behauptung des Marketplace und nicht überprüfbar. Ohne Zusammenlegen entfällt auch die Sonderregel;
welche Erweiterung wirklich installiert wird, entscheidet der Nutzer in der Vorschau am Herausgeberschlüssel
aus dem signierten Bundle (Spec 017).

**Alternativen**: Dopplung nach `extensionId` (Legacy, verworfen: Verdeckung); Zusammenlegen nur bei
gleichem `publicKey` (verworfen: der Schlüssel steht nur in den Details, ein Detailaufruf je Eintrag wäre
teuer, der Gewinn klein).

## R8 – Wann ist etwas „installiert“ und was ist ein Update

**Entscheidung**: Der Host vergleicht mit `extensions::registry::list` (Name und, wo bekannt, den
Herausgeberschlüssel): Listeneinträge haben nur den Namen, Details zusätzlich `publicKey`. Eine Kachel ist
„installiert“, wenn ein installierter Name gleich dem Namen des Eintrags ist; die Vorschau entscheidet
endgültig (Herausgeberschlüssel plus Name, „gleicher Name, anderer Herausgeber“, Spec 017). „Update
verfügbar“, wenn die **neueste kompatible** Fassung (`semver`, R9) höher ist als die installierte.

**Begründung**: eine Stelle für die Regel (Host), nicht in Vue und nicht noch einmal für die Liste der
Installierten. Legacy glich nur den Namen im Frontend ab.

**Bekannte Grenze**: Zwei Herausgeber mit gleichem Namen zeigen beide „installiert“, bis die Details
(`publicKey`) oder die Vorschau es klären; die Details zeigen den Schlüssel und markieren die Abweichung.

**Update-Prüfung** (FR-017): Ein Katalogabruf sieht nur eine gefilterte Seite. Darum gibt es eine eigene
Operation `marketplace_check_updates`: Für **jeden installierten Namen** wird jede eingeschaltete Quelle
mit `GET /extensions?search=<Name>&limit=5` gefragt (höchstens 4 Anfragen gleichzeitig), nur genaue
Namenstreffer zählen. Kandidaten werden je `(installedName, sourceId, slug)` geführt. Eine Quelle
aktualisiert oder entfernt nur ihre eigenen Kandidaten; bei einem Fehler bleiben diese erhalten. Ein
Katalogabruf ändert nur die Kandidaten der Quelle für Einträge, die auf der Seite vorkommen. Für die
Installationsliste wird die höchste bekannte kompatible Fassung gewählt, bei Gleichstand die Quelle mit
dem besseren Rang. Gestartet wird die Prüfung beim Öffnen der App und bei „Aktualisieren“, nie im
Hintergrund. (Das Legacy suchte beim Update ebenfalls nach dem Namen, mit `limit: 10`, aber erst auf Wunsch
je Erweiterung.)

## R9 – Kompatibilität mit der HaexVault-Fassung

**Entscheidung**: Die Fassung von HaexVault ist die aus `tauri.conf.json`/`Cargo.toml`
(`env!("CARGO_PKG_VERSION")`), als `semver`. `minAppVersion` und `maxAppVersion` einer Fassung begrenzen sie
(fehlt eine, gilt sie nicht). Nicht passende Fassungen sind im Katalog gekennzeichnet und nicht
installierbar (FR-018). Die Anfragen tragen `X-Platform` (`linux`, `windows`, `macos`, `android`, `ios`) und
`X-App-Version`, wie das SDK es vorsieht; sonst kein Merkmal des Nutzers oder Geräts (FR-007).

**Alternativen**: keine Prüfung wie im Legacy (verworfen: die Felder liefert der Dienst, Spec 017 kennt
Manifeste mit Anforderungen).

## R10 – Voreinstellung statt Standard-Sonderfall

**Entscheidung**: Es gibt keinen eingebauten Marketplace. Die **Voreinstellung** (Name und Adresse des
öffentlichen Marketplace, an einer einzigen Stelle in `marketplace/store.rs`, vom Betreiber festzulegen) ist
eine gewöhnliche Zeile mit fester Kennung `preset-public`:

- **Neuer Vault**: Die Zeile wird einmal angelegt, wenn der Vault seine Anfangsdaten bekommt (Genesis,
  `sync::genesis`; der genaue Ort ist Aufgabe T017). Nicht bei jedem Öffnen.
- **Vault ohne Marketplace** (vor dieser Funktion angelegt oder alles gelöscht): der Leerzustand des Katalogs
  bietet „Öffentlichen Marketplace hinzufügen“ an; das schreibt dieselbe Zeile mit derselben Kennung.
- Löschen, Ausschalten, Umbenennen, Adresse ändern: wie bei jeder Zeile. Nichts legt sie wieder an.

**Begründung**: Die feste Kennung lässt zwei Geräte, die beide anlegen, auf **eine** Zeile zusammenlaufen
(kein Duplikat, SC-006). Weil nichts beim Öffnen anlegt, kann ein spät eintreffender Sync den Marketplace
nicht still wieder einschalten (der Legacy-Fall `ensureDefaultMarketplaceAsync`). Die Adresse im Code ist
keine Sonderbehandlung: der Rest des Codes kennt sie nicht (FR-034), ein Test sucht die Adresse im Quelltext
außerhalb von `store.rs`.

**Alternativen**: Standard als Konstante im Code mit Überschreibungszeile (früherer Stand dieses Plans,
verworfen: ein Sonderfall, der nicht löschbar wäre und die Adresse in die Logik zieht); Zeile in der Migration
(verworfen: läuft je Gerät, gleicher Wettlauf).

## R11 – Zugangsdaten-Eintrag

**Entscheidung**: Der Marketplace hat `credentials_item_id` (Passwort-Eintrag, kein Fremdschlüssel, da der
Eintrag per Sync später eintreffen kann, wie in 038/052). `marketplace/credentials.rs` (neu) liest den
Eintrag als `Caller::Internal { feature: "marketplace" }` mit einer festen Berechtigung, liefert
`present`/`missing`/`syncing` mit denselben Regeln wie `remote_storage::credentials::state_in` und baut die
Kopfzeile (Bearer: Passwortfeld ist das Token; Basic: Benutzername und Passwort, `Zeroizing`). Tag
`haex-marketplace` (für neu angelegte Einträge und beim Wählen, wie 052 FR-006). Der Eintrag wird in
`passwords::usage::EntryUsage` als genutzt gemeldet (Löschwarnung, FR-029).

**Wiederverwendung**: `CredentialsEntryPicker.vue` bekommt statt `protocol` eine Eigenschaft
`source: { command, args }` (Aufrufer: Speicherverbindungen wie bisher, Marketplace neu); es wird **kein**
zweiter Picker gebaut. Die Rust-Seite teilt die Hilfen (`from_item`-ähnliche Feldzuordnung) nur dort, wo
der Code gleich ist; ein gemeinsames Modul für „Zugangsdaten eines Dienstes“ wäre eine Abstraktion über
zwei Fälle mit verschiedenen Eigentümern und Feldern (Speicher: Protokolle, Marketplace: Bearer/Basic) und
wird erst gezogen, wenn ein dritter dazukommt.

## R12 – Symbole

**Entscheidung**: Das Frontend fragt `marketplace_icon(source_id, url)`; der Host lädt (ohne
Zugangsdaten, ≤ 256 KiB, nur `image/png|jpeg|webp`, **kein SVG**) und gibt eine `data:`-URL zurück,
zwischengespeichert im Arbeitsspeicher (höchstens 200 Einträge). Höchstens 4 Symbole gleichzeitig.
Jedes Ziel muss öffentlich routbar sein, auch wenn es auf dem konfigurierten Marketplace-Origin liegt;
dadurch bleiben interne Marketplace-Adressen für Katalogaufrufe nutzbar, aber deren Symbole werden
nicht über diesen Abruf geladen. Externe URLs müssen `https` sein. Der öffentliche Speicher
`api.marketplace.haex.space` bleibt zulässig. Private, reservierte, Loopback-, Link-Local-, Multicast-
und nicht spezifizierte Adressen werden abgelehnt. Jede Weiterleitung wird erneut geprüft,
`https` → `http` ist verboten. Die Verbindung verwendet eine geprüfte, festgehaltene DNS-Adresse
(kein DNS-Rebinding). Fehlt ein Symbol oder ist es ungültig, zeigt die Kachel ein Standardsymbol.

**Begründung**: FR-006 (das Webview ruft keine Adresse eines Marketplace auf; die CSP bleibt zu). SVG wird
ausgelassen, damit kein aktiver Inhalt in `<img>` landet (auch wenn `<img>` Skripte nicht ausführt).
Die öffentliche Adressprüfung begrenzt SSRF auf öffentliche Bildserver.

## R13 – Agenten und Erweiterungen sehen den Katalog nicht

**Entscheidung**: Die Commands stehen nur im Tauri-Handler des Hauptfensters. Sie werden **nicht** in der
Registry der Agenten-Aktionen (`*ActionHandlers`, Spec 020/021), nicht im MCP-Server und nicht in der
Prüfstelle der Erweiterungen (`extensions/bridge/dispatch.rs`) eingetragen. Ein Test zählt: Keine Aktion,
kein MCP-Werkzeug und keine Host-Methode trägt den Namen `marketplace*` (FR-030); er schlägt fehl, wenn
jemand eine einträgt, und zwingt zu einer bewussten Entscheidung.

## R14 – Live-Daten und Sync

**Entscheidung**: `haex_marketplaces` ist eine gewöhnliche CRDT-Tabelle (`HOLZI_TRIGGER_VERSION` + 1,
Eintrag in der Liste der synchronisierten Tabellen). Die Einstellungsansicht abonniert Änderungen über das
bestehende `vault-data-changed`/`onVaultTablesChanged` (Spec 028-Live-Daten); Ein/Aus und Rang schreiben
beim Tippen sofort (Spec 023). Kein UNIQUE (ein Konflikt würde den Sync anhalten, Muster der anderen
Tabellen); Sortierlücken und gleiche Ränge werden beim Lesen mit `(rank, name, id)` stabil aufgelöst.

## R15 – e2e und Testmarketplace

**Entscheidung**: Der Rig (Spec 016) bekommt einen kleinen Node-Server, der Katalog, Kategorien, Details und
Download für zwei Erweiterungen liefert, eine davon mit dem bestehenden signierten Test-Bundle
(`haextension-test-bundles`, Wegwerf-Schlüssel). Die Adresse der Voreinstellung lässt sich im
e2e-Build überschreiben (`HOLZI_E2E_MARKETPLACE_URL`, nur in e2e-Builds, wie die anderen Schalter des
Rigs; die genaue Stelle klärt Aufgabe T003 in `tasks.md`). Im Release-Build gibt es keinen
Umgebungsschalter.

## R17 – Abgleich mit haex-vault 2.x (Legacy)

Gelesen in haex-vault @ `8dce379d`: `useMarketplaces.ts`, `useMarketplaceCatalog.ts`,
`useExtensionUpdate.ts`, `system/marketplace*.vue`, `settings/extensions/marketplaces.vue`,
`MarketplaceEditDialog.vue`, Migration `0000_jazzy_chat.sql` (Tabelle) und `useMarketplaces.test.ts`.

**Übernommen**

- Mehrere Quellen parallel mit Fehler je Quelle (`Promise.allSettled`, `sourceErrors`); Katalog, Suche,
  Kategorie, Seite, Sortierung nach Downloads, Seitengröße 20.
- Der Download geht über die Quelle, die den Eintrag lieferte (`sourceMarketplaceId`), nie über „irgendeine“.
- Einstellungen: Liste mit Schalter Ein/Aus je Zeile, Menü Bearbeiten/Löschen, Formular zum Anlegen,
  Auth-Arten keine, Bearer, Basic; Rang als Priorität (kleiner = früher).
- Nach Installieren, Aktualisieren und Entfernen aktualisiert sich der Katalog (Legacy-Fehlerkorrektur
  „refresh catalog after extension update“): Kachelzustände folgen `vault-data-changed` (ui.md).
- Version wählen in den Details; Update bewahrt die Daten, Neuinstallation ist eine eigene Bestätigung
  (in holzi bereits Teil der Vorschau, Spec 017).

**Bewusst anders**

| Legacy                                                              | holzi                                                                                               | Grund                                                            |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Dopplung nach `extensionId`, erste Quelle gewinnt (Test `dedupes…`) | Keine Zusammenlegung, jede Kachel mit Quelle (R7)                                                   | eine Quelle darf eine andere nicht verdecken (selbst gehostet)   |
| Token/Passwort als Spalten der Tabelle                              | Verweis auf einen Passwort-Eintrag (R11)                                                            | keine Geheimnisse in Vault-Tabellen, Sync und Protokollen        |
| `is_default`-Spalte, Standard nicht löschbar, Zeile beim Öffnen     | gewöhnliche Zeile `preset-public` (R10)                                                             | generische Konfiguration, kein Sync-Wettlauf                     |
| Netzwerk im Webview über `plugin-http` und SDK                      | Host (Rust, `reqwest`)                                                                              | ein HTTP-Pfad, Weiterleitungs- und Anmelderegeln an einer Stelle |
| `bundleHash` wird übergeben, Prüfung ist ein `TODO`                 | Prüfsumme und Größe sind Pflicht (FR-013)                                                           | ein Dritter liefert die Datei                                    |
| Bytes als `Array.from(Uint8Array)` durch die IPC und zurück         | Bytes bleiben im Host (R3)                                                                          | 24-MiB-Bundles, keine Verdopplung, nichts im Webview             |
| Update: Suche nach Name (`limit: 10`) auf Wunsch je Erweiterung     | Prüfung aller installierten Namen bei App-Öffnung und auf Wunsch, Kandidaten je Quelle (R8, FR-017) | bekannte Updates auch außerhalb der aktuellen Katalogseite       |
| neueste Fassung = `versions[0]`                                     | `semver` (R1)                                                                                       | die Reihenfolge ist nicht zugesichert                            |
| „installiert“ nur nach Name, im Frontend                            | Host, Name plus Herausgeberschlüssel in Details und Vorschau (R8)                                   | eine Stelle für die Regel                                        |
| DID-Anmeldung                                                       | später                                                                                              | Entscheidung des Betreibers, 2026-10-28                          |
| Formular mit „Erstellen/Speichern“ ohne Test                        | „Testen und speichern“ (FR-021)                                                                     | Fehler früh zeigen; Muster aus Spec 038                          |

## R16 – Offene Punkte außerhalb der Spec

- DID-Anmeldung (Legacy) und Rezensionen/Bewertungen schreiben: später, nicht Teil von 069.
- Automatische Updates und Hintergrundabfragen: bewusst nicht (FR-017).
- Eine ADR ist nicht nötig; sollte sich in der Umsetzung ergeben, dass ein Kernprinzip berührt wird, wird
  sie nachgereicht.
