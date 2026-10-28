# Research: Browser-Werkzeuge für Agenten

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Jede Entscheidung mit Begründung und verworfenen Alternativen. Die Nummern (R1 …) zitieren plan.md,
data-model.md und die Verträge.

## R1 Steuerung des Agenten-Browsers

**Entscheidung**: Ein eigener, schmaler CDP-Client in Rust (`src-tauri/src/agent_browser/cdp/`):
JSON-Nachrichten mit `serde_json`, Zuordnung von Antworten über `id`, Ereignisse je Sitzung
(`sessionId`, Flat Mode über `Target.setAutoAttach`). Gebraucht werden etwa 35 Methoden aus
`Target`, `Page`, `Runtime`, `DOM`, `Input`, `Fetch`, `Browser`, `Emulation`; sie stehen typisiert
in einem Modul je Domäne.

**Begründung**: Die Verbindung läuft über eine Pipe (R2), nicht über einen Port. `chromiumoxide`
und `headless_chrome` setzen einen WebSocket auf einem Port voraus und bringen den ganzen,
generierten Protokollumfang mit (lange Bauzeit, große Angriffsfläche, Pflege fremder Typen für
Methoden, die holzi nie ruft). Das Protokoll selbst ist einfach; der Aufwand steckt in den Regeln
(R4–R9), die holzi ohnehin selbst schreibt.

**Verworfen**: `chromiumoxide` (Port statt Pipe, schwerer Baum); `headless_chrome` (synchron,
Port); WebDriver BiDi (in Chromium noch nicht vollständig, kein Isolated World); Playwright als
Node-Prozess (zweite Laufzeit neben holzi, nicht in einer Binärdatei).

## R2 Prozess und Verbindung

**Entscheidung**: holzi startet den Browser selbst mit `--remote-debugging-pipe`: Befehle auf
Dateideskriptor 3, Antworten auf 4, Nachrichten durch NUL getrennt. Unter Linux und macOS werden
die Deskriptoren im Kind vor `exec` gesetzt; unter Windows über die CRT-Vererbung
(`STARTUPINFO.lpReserved2`, wie Puppeteer es tut). Kein `--remote-debugging-port`.

Weitere Startparameter: eigenes Profilverzeichnis (R3), `--no-first-run`,
`--no-default-browser-check`, `--disable-sync`, `--disable-extensions`,
`--password-store=basic` (Linux) bzw. `--use-mock-keychain` (macOS), damit Chromium den
Schlüsselbund des Systems nicht anfragt, `--disable-features=AutofillServerCommunication`,
Berechtigungen über `Browser.setPermission` ab Werk abgelehnt (FR-026).

Welcher Browser: der erste gefundene aus einer Liste je Betriebssystem (Chromium, Chrome, Edge,
Brave, jeweils in den üblichen Pfaden und auf `PATH`), sonst ein Angebot, „Chrome for Testing“ in
der Version des Plans herunterzuladen (Größe vorher sichtbar, Prüfsumme gegen die veröffentlichte
Liste). Gewählter Browser und Version stehen in „KI → Browser“.

**Begründung**: Eine Pipe erreicht nur der Elternprozess; ein Port auf Loopback wäre für jedes
Programm des Nutzers offen (FR-004). Die Windows-Variante ist dieselbe Technik, die Puppeteer seit
Jahren nutzt; sie bekommt einen eigenen Test im Windows-Lauf der CI (holzi gilt auf jedem
Betriebssystem gleich).

**Verworfen**: Port mit zufälliger Nummer und Token (Chromium kennt kein Token für den Port);
Firefox (CDP-Unterstützung entfernt, BiDi unvollständig); eingebauter Webview von Tauri (Spec
schließt ihn aus; keine Trennung vom Fenster von holzi).

## R3 Browser-Profil verschlüsselt auf dem Gerät

**Entscheidung**: Ein zufälliger Profilschlüssel (32 Byte) liegt in der Vault in einer
geräteeigenen Tabelle ohne Sync (`haex_agent_browser_no_sync`). Das Profil liegt, solange der
Agenten-Browser läuft, entschlüsselt in einem Arbeitsverzeichnis mit Rechten `0700`:

- Linux: unter `$XDG_RUNTIME_DIR` (tmpfs, beim Abmelden leer),
- macOS: unter dem nutzereigenen `$TMPDIR`,
- Windows: unter `%LOCALAPPDATA%\Temp` mit den Rechten des Nutzers.

Beim Beenden des Browsers (spätestens beim Schließen der Vault) packt holzi die bleibenden Teile
(Cookies, Local Storage, IndexedDB, Session Storage, `Preferences`) in ein Archiv, verschlüsselt es
mit XChaCha20-Poly1305 im STREAM-Verfahren (Blöcke zu 64 KiB) und legt es neben die Vault-Datei
(`agent-browser/<vault>.profile`); dann löscht es das Arbeitsverzeichnis. Caches (`Cache`,
`Code Cache`, `GPUCache`, `Service Worker/CacheStorage`) kommen nicht ins Archiv. Beim nächsten
Start entschlüsselt holzi das Archiv in ein frisches Arbeitsverzeichnis. Reste eines abgestürzten
Laufs löscht holzi beim nächsten Start, bevor es etwas Neues anlegt.

**Begründung**: Chromium schreibt sein Profil als normale Dateien; transparent verschlüsseln kann
holzi es nicht. Das Archiv erfüllt FR-001: ohne entsperrte Vault unlesbar, über Neustarts
erhalten, nie auf anderen Geräten (der Schlüssel steht in einer `_no_sync`-Tabelle, das Archiv
liegt nur auf diesem Gerät). Die Konstruktion ist dieselbe wie bei den Gerätenamen
(`sync/content_keys_names.rs`) und den verschlüsselten Ordnern.

**Grenze**: Während der Browser läuft, liegt das Profil entschlüsselt auf der Platte (unter Linux
im Speicher). Ein Absturz kann es bis zum nächsten Start liegen lassen. Das steht in der ADR.

**Verworfen**: Profil im Klartext neben der Vault (FR-001); Schlüssel aus dem Inhaltsschlüssel der
Vault abgeleitet (der wandert auf alle Geräte; ein gestohlenes Archiv wäre auf jedem Gerät der
Vault lesbar); Profil in die Vault-Datenbank (zu groß, Chromium braucht Dateien).

## R4 Seite lesen und Elemente ansprechen

**Entscheidung**: holzi legt je Agenten-Tab eine **eigene Welt** (`Page.createIsolatedWorld`) an
und führt dort ein Leseskript aus. Es liefert:

- Titel, Adresse, lesbaren Text (Hauptinhalt zuerst, gekürzt nach FR-032, Kürzung angegeben),
- eine nummerierte Liste der bedienbaren Elemente: Art (Link, Schaltfläche, Feld, Auswahl,
  Kästchen …), zugänglicher Name, Wert (bei Passwortfeldern und gefüllten Feldern nie, R7),
  Zustand (aktiv, ausgewählt, Pflichtfeld), Lage im sichtbaren Bereich ja/nein.

Die Nummern gelten bis zur nächsten Änderung des Dokuments; holzi merkt sich zu jeder Nummer das
DOM-Objekt (`backendNodeId`). Klicks gehen als echte Mauseingaben (`Input.dispatchMouseEvent`) auf
die Mitte des Elements, Tippen über `Input.insertText` bzw. `Input.dispatchKeyEvent`.

**Begründung**: In der eigenen Welt sieht das Skript dasselbe DOM, aber die Seite kann seine
Funktionen weder ersetzen noch aufrufen. Die Nummernliste ist das Vorgehen von browser-use und
spart dem Modell Bildschirmfotos. Echte Eingaben verhalten sich wie ein Nutzer und lösen dieselben
Prüfungen der Seite aus.

**Verworfen**: Skript in der Welt der Seite (manipulierbar); nur Barrierefreiheitsbaum
(`Accessibility.getFullAXTree`; langsam und auf großen Seiten unvollständig, bleibt als Quelle für
Namen); Klick über `element.click()` (umgeht Prüfungen, kein echtes Nutzerereignis).

## R5 Absenden erkennen

**Entscheidung**: Zwei Stufen.

1. **Vor dem Klick** prüft das Leseskript das Ziel: Schaltfläche oder `input` vom Typ
   `submit`/`image` in einem Formular, `button` ohne Typ in einem Formular, Element mit einem
   zugänglichen Namen aus einer Wortliste (de/en: kaufen, bestellen, bezahlen, zahlen, senden,
   absenden, abschicken, löschen, entfernen, veröffentlichen, posten, buchen, bestätigen,
   kündigen, überweisen; buy, order, pay, checkout, send, submit, delete, remove, publish, post,
   book, confirm, cancel subscription, transfer), Enter in einem Formularfeld. Treffer = Absenden.
2. **Danach** hält holzi über `Fetch.enable` jede ändernde Anfrage der Seite an (`POST`, `PUT`,
   `PATCH`, `DELETE`, Formular-Navigation mit Body), die innerhalb von 3 Sekunden nach einer
   ändernden Agenten-Handlung oder bis die Seite zur Ruhe kommt ausgelöst wird, und fragt sie als
   Absenden nach (Website des Ziels, Art der Anfrage, Felder aus dem Formular, wenn bekannt).
   Ändernde Anfragen außerhalb dieses Fensters und ohne Agenten-Handlung gehen durch (die Seite
   lädt selbst nach). „Ablehnen“ bricht die Anfrage ab (`Fetch.failRequest`).

**Begründung**: Stufe 1 trifft die üblichen Formulare mit klarer Rückfrage vor der Handlung.
Stufe 2 erfüllt „im Zweifel Absenden“ (FR-011), ohne jeden Klick zu erfragen: Was die Seite
tatsächlich wegschickt, wird geprüft, auch bei Schaltflächen ohne sprechenden Namen und bei Seiten,
die mit eigenem Skript senden. Der Preis sind Rückfragen bei Seiten, die nach einem Klick
Statistik senden; das misst SC-003 mit.

**Verworfen**: Jeden Klick auf eine Schaltfläche als Absenden (in Auto unbrauchbar); nur die
Wortliste (verfehlt Skript-Formulare); alle ändernden Anfragen immer anhalten (fragt beim Laden
vieler Seiten).

## R6 Markierung, Eingreifen und Stoppen

**Entscheidung**: Die Markierung ist ein Element mit geschlossenem Shadow Root, das die eigene
Welt in jedes Dokument eines Agenten-Tabs einsetzt (`Page.addScriptToEvaluateOnNewDocument` mit
`worldName`). Die Knöpfe melden sich über eine **Bindung nur in der eigenen Welt**
(`Runtime.addBinding` mit `executionContextName`); Ereignisse mit `isTrusted == false` werden
verworfen. Zusätzlich setzt holzi den Fenstertitel des Tabs mit einem Präfix („Agent: …“) und
zeigt jede Sitzung in seiner eigenen Oberfläche (FR-015), dort ist „Stoppen“ verbindlich.

Eine echte Eingabe des Nutzers erkennt holzi an vertrauenswürdigen Maus- und Tastenereignissen in
der eigenen Welt, die nicht von einer Agenten-Handlung stammen (holzi kennt die Zeitfenster seiner
eigenen `Input.*`-Aufrufe), und pausiert die Sitzung (FR-017).

**Begründung**: Die Seite kann die Bindung der eigenen Welt nicht aufrufen und keine
vertrauenswürdigen Ereignisse erzeugen; sie kann die Markierung höchstens verdecken. Darum ist die
Anzeige in holzi die verbindliche, die Markierung im Tab die bequeme.

**Verworfen**: Markierung in der Welt der Seite (fälschbar); Browser-Erweiterung im
Agenten-Browser (FR-002: ohne Erweiterungen); eigenes Fenster von holzi über dem Browser
(Fenster-Stapel ist je Betriebssystem anders, verdeckt Inhalte).

## R7 Anmeldedaten eintragen

**Entscheidung**: Das Werkzeug `browser.credentials.fill` bekommt nur die Nummer des Formulars oder
nichts. holzi sucht die passenden Einträge über einen Dienst im Passwortmanager
(`passwords::site::entries_for_origin`), der den heutigen Filter aus
`browser_bridge/methods/items.rs` übernimmt (eine Regel, zwei Nutzer; der ponytail-Vermerk dort
nennt genau diesen Schritt). Die Werte schreibt holzi in der eigenen Welt direkt in die Felder
(Wertzuweisung plus `input`/`change`-Ereignisse) und merkt sich die Felder als „gefüllt“.

Schutz vor Rückfluss (FR-019, FR-022):

- Das Leseskript liefert für Passwortfelder und gefüllte Felder nie einen Wert.
- holzi entfernt aus jedem Seitentext, jedem Elementnamen und jeder Eingabe-Rückmeldung
  vorkommende eingetragene Werte (genau und als URL-, Base64- und HTML-Entity-Kodierung).
- Solange das Dokument gefüllte Felder enthält, verweigert holzi Bildschirmfotos dieses Tabs
  („Bildschirmfoto nach dem Eintragen erst auf der nächsten Seite“); eine neue Seite hebt das auf.
- Nur über `https` und nur, wenn die Website des Tabs zum Eintrag passt (FR-020); in einem
  eingebetteten Rahmen nur, wenn dessen Website passt.

**Begründung**: Das Modell braucht das Passwort nicht, nur das Ergebnis. Das Verweigern von
Bildschirmfotos ist grob, aber sicher, weil holzi nicht erkennen kann, ob eine Seite das Passwort
sichtbar macht; nach der Anmeldung wechselt die Seite fast immer.

**Verworfen**: Passwort tippen lassen (das Modell sähe es); Schwärzen von Feldern im Bild
(versagt, wenn die Seite den Wert anderswo anzeigt).

## R8 Rechte, Aufrufer und Rückfragen

**Entscheidung**:

- **Rechtegruppe** `browser` in `mcp_server::grants::Rights` (021 FR-008): `enabled`,
  `sites: Map<Website, SiteGrant { level: read|operate, fill_remembered: bool }>`,
  `user_browser: bool`. Gespeichert in `haex_mcp_agent_grants` mit `grant_group = 'browser'`
  (`grant_key` = Website, `level`, `scope_tags` für die gemerkte Anmeldung) und je einer Zeile für
  `enabled`/`user_browser`. Keine Migration der Tabelle nötig.
- Der **eingebaute Agent** hat dieselbe Form als Geräteeinstellung (`agent_browser.builtin`),
  bearbeitet in „KI → Browser“.
- Die Browser-Werkzeuge sind **native Katalogaktionen** (ADR 0011) im Bereich `browser`, mit
  `confirmsItself` für alle ändernden und zerstörenden Werkzeuge. Die allgemeine Freigabe von
  Manuell und Auto entfällt also; der Browser-Dienst fragt selbst, nach Freigabestufe und Art
  der Handlung, genau einmal je Handlung. Plan lehnt ändernde Werkzeuge vorher ab.
- **Voraussetzungen aus 021**, die diese Spec mitliefert (oder 021 PR E, je nachdem, was zuerst
  kommt): `NativeExecutor::run` bekommt einen `NativeCall { caller, mode }`; der MCP-Zugang führt
  native Aktionen direkt aus statt über das Fenster und beachtet `confirmsItself` in `call_gate`.
- Die **Rückfragen** des Browsers (Website zulassen, Absenden, erste Anmeldung, Eingreifen erbeten,
  Download über 100 MB, von einer Seite geöffneter Tab) parken auf `prompt_park.rs` mit 5 Minuten,
  im Fenster als ein Dialog mit Arten, wie der Aufrufdialog aus 021 PR C.

**Begründung**: Eine Rechte-Funktion für Liste und Prüfung (021 R3); keine neue Tabelle; dieselbe
Regel für eingebauten und externen Agenten; keine doppelten Rückfragen.

**Verworfen**: Browser-Werkzeuge als eigene Rust-Werkzeuge außerhalb des Katalogs (zweiter Weg an
`find_actions`, Prüfskripten und MCP-Liste vorbei); Website-Rechte als `scope_tags` am Bereich
`browser` (unlesbar, keine Stufe je Website).

## R9 Websites, Weiterleitungen und Sperren

**Entscheidung**: Website = Ursprung (Schema, Host, Port); in der Oberfläche als Host gezeigt, ohne
Standardport. Jede Navigation eines Agenten-Tabs (auch Weiterleitung, Rahmen der obersten Ebene,
`window.open`) läuft über `Fetch.requestPaused` für Dokument-Anfragen: holzi prüft die Website
vor dem Laden gegen die Rechte und fragt nach FR-007 oder bricht ab. Eingebettete Rahmen fremder
Websites dürfen laden (sonst bricht fast jede Seite), werden aber nur gelesen und bedient, wenn
ihre Website erlaubt ist. Immer gesperrt: `file:`, `javascript:`, `data:` als Dokument,
`chrome:`/`edge:`/`brave:`/`devtools:`, Adressen mit Zugangsdaten, und `http:` für Websites mit
„bedienen“ (nur lesen über unverschlüsselte Verbindungen).

**Begründung**: Die Prüfung vor dem Laden verhindert, dass eine fremde Website auch nur einmal
Inhalte an das Modell gibt. Rahmen zu sperren würde Anmeldungen und Zahlungen fast aller Seiten
brechen; Bedienen bleibt trotzdem an die Rechte gebunden.

## R10 Nicht vertrauenswürdige Inhalte (Prompt Injection)

**Entscheidung**:

- Jedes Ergebnis mit Seiteninhalt hat die Form
  `{ "untrusted_page_content": { "site": …, "text": …, "elements": [ … ] }, "note": … }`;
  die Beschreibung jedes Browser-Werkzeugs sagt, dass Seiteninhalte keine Anweisungen sind.
- Für den eingebauten Agenten setzt der Chat den Inhalt zusätzlich in die Abgrenzung, die er für
  Werkzeugergebnisse ohnehin hat (Spec 003).
- Keine Seite kann eine Rückfrage überspringen, eine Website zulassen, ein Recht ändern oder eine
  Bindung von holzi aufrufen (R4, R6, R8).
- SC-002 wird mit einer festen Sammlung von 20 Testseiten mit eingeschleusten Anweisungen geprüft
  (Testkorpus in `src-tauri/tests/fixtures/agent_browser/injection/`), gegen die Regeln, nicht
  gegen das Verhalten eines Modells.

**Begründung**: Ob ein Modell einer eingeschleusten Anweisung folgt, kann holzi nicht verhindern;
was es dann tun kann, schon. Die Tests prüfen genau das.

## R11 Downloads und Hochladen

**Entscheidung**: `Browser.setDownloadBehavior` mit `allowAndName` in ein Arbeitsverzeichnis;
fertige Dateien verschiebt holzi in den Ordner „Agenten-Downloads“ der Vault-Instanz (geräteeigener
Speicher von holzi, wie andere Dateien von holzi aus 044 erreichbar). Über 100 MB (aus
`Content-Length` bzw. beim Überschreiten) hält holzi den Download an und fragt. Hochladen setzt
die Datei über `DOM.setFileInputFiles`, nur aus Pfaden, die die Dateirechte des Agenten (044)
erreichen, und zählt als Absenden (FR-011).

## R12 Grenzen

**Entscheidung**: 5 Agenten-Tabs je Agent (FR-029), 30 s je Werkzeug (FR-030), Seitentext höchstens
16 000 Zeichen und 300 Elemente je Ergebnis, mit `truncated: true` und Werkzeug `browser.page.read`
mit `from`/`section` für Ausschnitte (FR-032). Agenten-Browser schließt nach 15 Minuten ohne
Sitzung (FR-003). Bildschirmfoto als JPEG (Qualität 70) des sichtbaren Bereichs, höchstens
1280 px breit.

## R13 Plattformen

**Entscheidung**: Agenten-Browser auf Linux, macOS, Windows (`platform::capabilities().agent_browser`).
Android und iOS: kein Agenten-Browser (kein steuerbarer Chromium mit Pipe); die Werkzeuge stehen
nicht im Katalog des Geräts, „KI → Browser“ zeigt „auf diesem Gerät nicht verfügbar“. Stufe 2 (US6)
hängt an der Browser-Erweiterung aus 049, die es nur auf Desktop-Browsern gibt.

## R14 Stufe 2: Tabs im Browser des Nutzers

**Entscheidung**: Die Erweiterung aus 049 bekommt Protokollnachrichten für übergebene Tabs
([contracts/extension-tabs.md](./contracts/extension-tabs.md)): `agentTab.handOver`/`take back`
(Nutzer in der Erweiterung), `agentTab.read`, `.click`, `.type`, `.select`, `.scroll`,
`.navigate`, `.screenshot`, `.fill` und das Ereignis `agentTab.request` für ändernde Anfragen.
Die Erweiterung nutzt `chrome.debugger` nur für den übergebenen Tab (zeigt dabei Chromes eigene
Leiste „wird von einer Erweiterung gesteuert“) und ein Inhaltsskript in einer eigenen Welt für
Lesen und Markierung. Die Regeln (R5–R10) laufen in holzi; die Erweiterung führt nur aus und
meldet. Firefox: ohne `chrome.debugger` nur Lesen und Klicks über das Inhaltsskript; fehlende
Fähigkeiten fehlen dort (FR-035).

**Begründung**: Eine Regel an einer Stelle (holzi), die Erweiterung als dünner Ausführer. Die
Leiste von Chrome macht die Steuerung zusätzlich sichtbar.

## R15 Lieferungen

**Entscheidung**: Wie 021 in PRs aus eigenen Worktrees:

| PR  | Inhalt                                                                                                                                                          |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A   | Spec, Plan, ADR-0019 (proposed), Aufgaben (dieser PR)                                                                                                           |
| B   | Grundlage: `NativeCall` mit Aufrufer und Stufe, native Aktionen über MCP, `confirmsItself` im MCP-Zugang, Rechtegruppe `browser`, Passwort-Zuordnung als Dienst |
| C   | Agenten-Browser: Finden, Start über Pipe, CDP-Client, Profil verschlüsselt, Lesen, Navigieren mit Website-Prüfung (US1)                                         |
| D   | Bedienen, Absenden-Erkennung, Rückfragen, Markierung, Eingreifen, Stoppen, Sitzungsanzeige (US2, US3)                                                           |
| E   | Anmeldedaten eintragen (US4), Downloads und Hochladen (US5)                                                                                                     |
| F   | Tabs im Browser des Nutzers (US6), zusammen mit einer Version der Erweiterung                                                                                   |
