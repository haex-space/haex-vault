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

**Nochmals geprüft am 2026-11-01** (Frage: Tauri-Webview statt Chromium, damit es auf jeder
Plattform gleich läuft): verworfen. Tauri bringt keinen eigenen Browser mit, sondern nutzt die
Web-Engine des Systems, und die trägt die Schutzregeln nicht überall:

| Fähigkeit                                              | WebView2 (Windows) | WebKitGTK (Linux)            | WKWebView (macOS)          | Android WebView |
| ------------------------------------------------------ | ------------------ | ---------------------------- | -------------------------- | --------------- |
| Ändernde Anfragen vor dem Server anhalten (R5 Stufe 2) | ja                 | nur mit Modul im Web-Prozess | nein                       | ja              |
| holzis Code in einer eigenen Welt (R4, R6)             | ja                 | ja                           | ja                         | nein            |
| Echte Eingabe-Ereignisse (R4)                          | ja                 | nur über eigene Ereignisse   | nur über eigene Ereignisse | ja              |

Dazu verweigern Google und manche Banken die Anmeldung in eingebetteten Webviews. Jede Engine
bräuchte einen eigenen Ausführer, und wo eine Fähigkeit fehlt, müsste das Bedienen fehlen (wie
FR-035). Der Agenten-Browser auf dem Desktop deckt die Ziele (Recherche, Formulare mit Daten aus
dem Passwortmanager) ohne diese Lücken; Android bleibt ohne Agenten-Browser (R13).

## R3 Browser-Profil verschlüsselt auf dem Gerät

**Entscheidung** (Betreiber, 2026-10-29): Was holzi verschlüsselt auf der Platte ablegt, liegt in
einem **verschlüsselten Ordner aus Spec 048**; der Agenten-Browser bringt kein eigenes
Archivformat und keinen eigenen Schlüssel mit.

- **Zwischen zwei Läufen**: Beim Ende des Agenten-Browsers (spätestens beim Schließen der Vault)
  packt holzi die bleibenden Teile des Profils (Cookies, Local Storage, IndexedDB, Session
  Storage, `Preferences`, `Local State`) als ZIP und schreibt es als Datei `profile.zip` in den
  verschlüsselten Ordner „Profil“ unter `<AppLocalData>/agent-browser/<Instanz>/` (ein
  Geräteordner, 048 US7). Caches (`Cache`, `Code Cache`, `GPUCache`,
  `Service Worker/CacheStorage`) kommen nicht hinein. Das Datenverzeichnis gehört zu dieser
  Installation; die Datei wandert nicht mit der Vault.
- **Während des Laufs**: Chromium braucht ein normales Verzeichnis. Es liegt unter
  `$XDG_RUNTIME_DIR` (Linux, im Arbeitsspeicher), `$TMPDIR` (macOS) bzw. `%LOCALAPPDATA%\Temp`
  (Windows), nur für den Nutzer lesbar. holzi löscht es am Ende jedes Laufs und vor jedem Start,
  also auch die Reste eines abgestürzten Laufs.
- **Anmeldedaten** liegen nie im Profil als gespeicherte Passwörter: das Speichern und
  automatische Ausfüllen von Chromium ist aus (FR-002); Anmeldedaten kommen aus dem
  Passwortmanager und neue werden dort gespeichert (R7, FR-036). Im Profil bleiben nur
  Sitzungs-Cookies und Daten der Websites.

**Begründung**: Eine Verschlüsselung, ein Format, eine geprüfte Implementierung (048, mit eigenen
Tests auf allen Betriebssystemen). Ohne entsperrte Vault ist das Profil unlesbar. Ein zusätzlicher
verschlüsselter Container für den laufenden Browser wäre je Betriebssystem anders (unter Windows
nur mit EFS, das Windows Home nicht hat) und schützt nur gegen jemanden, der das Benutzerkonto
schon hat; den Diebstahl des Geräts deckt die Festplattenverschlüsselung des Systems.

**Abwägung**: Der Schlüssel eines verschlüsselten Ordners stammt aus dem Inhaltsschlüssel der
Vault und ist auf allen Geräten der Vault gleich. Wer die Datei `profile.zip` aus dem
Datenverzeichnis dieses Geräts kopiert und eine entsperrte Vault hat, könnte sie lesen; die Datei
selbst wandert aber nie.

**Verworfen**: Eigenes Archiv (HXBP, STREAM) mit Schlüssel im Zugangsspeicher des
Betriebssystems und verschlüsselter Arbeitscontainer (Review von #618; zweite Verschlüsselung
neben 048, Windows Home ohne Lösung); Profil im Klartext neben der Vault (FR-001); Profil in der
Vault-Datenbank (zu groß, Chromium braucht Dateien).

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
2. **Für jede Seitenanfrage** hält holzi über `Fetch.enable` `POST`, `PUT`, `PATCH`, `DELETE` und
   Formular-Navigationen mit Body an und fragt sie als Absenden nach (Website des Ziels, Art der
   Anfrage, Felder aus dem Formular, wenn bekannt). Die Freigabe ist an die einzelne CDP-Anfrage
   gebunden; es gibt kein zeitbasiertes Fenster, das eine verspätete Anfrage durchlassen könnte.
   „Ablehnen“ bricht die Anfrage ab (`Fetch.failRequest`).

**Geändert am 2026-11-08**: Gefragt wird nur noch, wenn persönliche Daten gehen (Konto, Adresse,
Finanzdaten, Zugangsdaten) oder die Handlung Geld kostet oder nicht rückgängig zu machen ist
(Wortliste ohne senden, absenden, bestätigen, abmelden). Stufe 1 prüft dafür die Felder des
Formulars (Name, `type`, `autocomplete`, Wert); Stufe 2 liest den Rumpf der Anfrage. Der Preis der
fail-closed-Regel (Rückfragen für harmlose Anfragen) war in der Praxis zu hoch.

**Begründung**: Stufe 1 trifft die üblichen Formulare mit klarer Rückfrage vor der Handlung.
Stufe 2 erfüllt „im Zweifel Absenden“ (FR-011), indem jede ändernde Anfrage geprüft wird, auch wenn
ein Seitenskript sie verzögert oder unabhängig von einem sichtbaren Klick auslöst. Rückfragen für
legitime schreibende Hintergrundanfragen sind der Preis dieser fail-closed-Regel und werden in
SC-003 mitgemessen.

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
(`PasswordsService::headers_for_page`), der den heutigen Filter aus
`browser_bridge/methods/items.rs` übernimmt (eine Regel, zwei Nutzer; der ponytail-Vermerk dort
nennt genau diesen Schritt). Die Werte schreibt holzi in der eigenen Welt direkt in die Felder
(Wertzuweisung plus `input`/`change`-Ereignisse) und merkt sich die Felder als „gefüllt“.

Schutz vor Rückfluss (FR-019, FR-022):

- Das Leseskript liefert für Passwortfelder und gefüllte Felder nie einen Wert.
- holzi entfernt aus jedem Seitentext, jedem Elementnamen und jeder Eingabe-Rückmeldung
  vorkommende eingetragene Werte (genau und als URL-, Base64- und HTML-Entity-Kodierung).
- Sobald Zugangsdaten in eine Sitzung eingetragen wurden, verweigert holzi Bildschirmfotos für den
  Rest dieser Sitzung. Ein neues Dokument reicht nicht aus, um eine Seite zu beweisen, dass keine
  Zugangsdaten übernommen wurden; ein neues, sauberes Browsersitzungsprofil hebt die Sperre auf.
- Nur über `https` und nur, wenn die Website des Tabs zum Eintrag passt (FR-020); in einem
  eingebetteten Rahmen nur, wenn dessen Website passt.

Neue Anmeldedaten (FR-036): Das Werkzeug `browser.credentials.create` bekommt die Nummer des
neuen Passwortfelds und den Benutzernamen. holzi erzeugt das Passwort, trägt es in dieses Feld
(und ein Wiederholungsfeld desselben Formulars) ein, merkt die Felder als „gefüllt“ und legt nach
der Rückfrage einen Eintrag mit Benutzername, Passwort und der Website des Tabs im
Passwortmanager an. Lehnt der Nutzer ab, leert holzi die Felder wieder.

**Begründung**: Das Modell braucht das Passwort nicht, nur das Ergebnis. Das Verweigern von
Bildschirmfotos ist grob, aber sicher, weil holzi nicht erkennen kann, ob eine Seite das Passwort
sichtbar macht; nach der Anmeldung wechselt die Seite fast immer.

**Verworfen**: Passwort tippen lassen (das Modell sähe es); Schwärzen von Feldern im Bild
(versagt, wenn die Seite den Wert anderswo anzeigt).

## R7a Andere Daten aus einem Eintrag eintragen (FR-037, FR-038)

**Entscheidung**: Ein eigenes Werkzeug `browser.entry.fill` mit `tab`, `form?` und `purpose`.
Bank- und Kartendaten gehören zu keiner Website, daher sucht holzi nicht nach der Website wie in
R7, sondern lässt den Nutzer in der Rückfrage (Art `entryFill`) einen Eintrag wählen, aus denen,
die die Rechtegruppe Passwörter des Agenten erreicht (`Grant` wie in R7, Prüfungen aus 034). Die
Rückfrage kommt jedes Mal und zeigt die Zuordnung „Feld der Seite ← Feld des Eintrags“ mit
verdeckten Werten; der Nutzer kann Zeilen abwählen.

Die **Zuordnung** ist eine Regel: genauer Name des Felds, Autofill-Aliase des Eintrags (036
FR-001a), Standardaliase, Art des Felds, dazu `autocomplete`-Werte `cc-name`, `cc-number`,
`cc-exp`, `cc-exp-month`, `cc-exp-year`, `cc-csc`. Diese Regel steht heute nur in der Erweiterung
(`apps/haex-pass-browser/src/contentScripts/autofill.ts`, `DEFAULT_ALIASES` und `fillAllFields`).
Damit sie nur einmal existiert, wandert sie in eine Rust-Crate `haex-autofill` im vault-sdk (wie
`haex-bundle`): holzi nutzt sie direkt (auch für `browser.credentials.fill`), die Erweiterung
ihren WebAssembly-Build. Eingabe sind die Fakten der Felder, die das Leseskript ohnehin liefert
(Name, `id`, `autocomplete`, Art, Formular, Rahmen), und die Schlüssel des Eintrags; Ausgabe ist
die Zuordnung. Werte gehen nie durch die Regel.

Eintragen und Schutz vor Rückfluss wie in R7: Schreiben aus holzis Welt, Felder als „gefüllt“
gemerkt, Werte aus Text, Namen und Rückmeldungen entfernt, Bildschirmfotos für den Rest der
Sitzung gesperrt. In einen Rahmen trägt holzi nur ein, wenn der Agent dessen Website bedienen
darf (Zahlungsanbieter betten ihre Felder oft so ein). Die Rückfrage vor dem Absenden (R5) zeigt
die Werte als `•••`, weil die Felder als „gefüllt“ gelten.

**Begründung**: Der Nutzer sieht vor jeder Zahlung, welcher Eintrag wohin geht, und das Absenden
fragt danach noch einmal. Kein Merken, weil sich Betrag und Empfänger mit jedem Formular ändern.
Eine Regel für Erweiterung und Agenten-Browser verhindert, dass beide unterschiedlich ausfüllen.

**Verworfen**: `credentials.fill` um eine Art erweitern (dort wird nach Website zugeordnet und
gemerkt, beides passt nicht zu Zahlungsdaten); den Agenten Feldnamen des Eintrags wählen lassen
(er sähe den Aufbau der Einträge); die Regel in holzi nachbauen (zwei Regeln, die
auseinanderlaufen).

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
`window.open`) und jede Navigation in einem eingebetteten Rahmen laufen vor dem Laden durch
`Fetch.requestPaused`: holzi prüft die Website gegen die Rechte und fragt nach FR-007 oder bricht
ab. Nicht erlaubte Rahmen werden nicht geladen. Immer gesperrt: `file:`, `javascript:`, `data:` als Dokument,
`chrome:`/`edge:`/`brave:`/`devtools:`, Adressen mit Zugangsdaten, und `http:` für Websites mit
„bedienen“ (nur lesen über unverschlüsselte Verbindungen).

**Begründung**: Die Prüfung vor dem Laden verhindert, dass eine fremde Website auch nur einmal
Inhalte an das Modell gibt, solange der Nutzer sie nicht ausdrücklich erlaubt hat. Erlaubte
Rahmen bleiben nutzbar; Rahmen mit nicht erlaubten Ursprüngen warten auf die Website-Freigabe.
Rahmen pauschal zu sperren würde Anmeldungen und Zahlungen vieler Seiten
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
`browser.download` klickt das Element (Rückfragen wie ein Klick) und folgt dem Download, der in
einem Rahmen dieses Tabs beginnt (`Browser.downloadWillBegin`, `Browser.downloadProgress`). Fertige
Dateien verschiebt holzi unter einem sicheren, eindeutigen Namen in den Ordner
`HaexVault/<Vault>` im Download-Ordner des Systems. Über 100 MB (aus `Content-Length` bzw.
beim Überschreiten) fragt holzi; der Browser kann einen Download nicht anhalten, er läuft also
weiter, bleibt aber im Arbeitsverzeichnis, bis der Nutzer zustimmt. Lehnt er ab, bricht holzi ab
(`Browser.cancelDownload`) und löscht, was angekommen ist. Ein Aufruf, der vorher endet, bricht
seinen Download ebenso ab. Hochladen setzt die Datei über `DOM.setFileInputFiles`, nur aus
Pfaden, die die Dateirechte des Agenten (044) erreichen, und sagt für eine fehlende Datei
dasselbe wie für eine, die der Agent nicht erreicht (`file_not_allowed`). Es fragt wie Absenden
(FR-011), lässt aber keine Anfrage der Seite durch: das Absenden des Formulars fragt erneut. Das
Element darf das Dateifeld selbst sein oder ein Element, zu dem es gehört (Beschriftung, Knopf im
selben Formular), weil Seiten das Feld oft verstecken.

**Nachtrag 2026-11-01**: Der Ordner lag zuerst im eigenen Speicher der Vault-Instanz. Die
Dateirechte aus 044 sperren holzis eigene Ordner aber für jeden Agenten, der Agent hätte seine
Downloads nicht lesen können. Der Download-Ordner des Systems erreicht das Recht „Dateien des
Geräts“ ohne Ausnahme in der Regel, und der Nutzer findet die Dateien dort, wo ein Browser sie
ablegt.

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

**Zurückgestellt am 2026-11-01**: Die Ziele brauchen den Browser des Nutzers nicht, weil
Anmeldungen und Zahlungsdaten aus dem Passwortmanager kommen (US4). Die Entscheidung unten bleibt
als Entwurf.

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

## R16 Blocking trackers (revision 2026-11-03)

**Decision**: The agent browser asks CDP to hold every request of every resource type
(`Fetch.enable`; R5 holds only documents and requests that may change something, so the patterns
widen to images, styles, scripts, fonts and media); before the submit check, each request goes
through a filter engine and a match fails with `BlockedByClient`. The engine is Brave's `adblock` crate (Rust, MPL-2.0), which
reads the Adblock Plus and uBlock Origin list syntax; only network filters are loaded, so
cosmetic filters, scriptlets and redirect resources never run. The lists are downloaded over
`https` (the default ones from their publishers, the user's own by their address), cached as
files, refreshed after seven days and compiled when the browser starts; a list that fails keeps
its last good copy. The default set is what uBlock Origin enables. A request carries the page it
comes from as its source, as in uBlock: the document of its frame (from `Page.frameNavigated`),
its `Referer` while that frame is unknown; a page of the tab's own frame is its own source and a
frame's document comes from its `Referer`. So third-party rules (`$third-party`) and rules bound to
a website (`$domain=`) work also on pages that send no `Referer`. One engine serves every tab and
is replaced in place when the lists are compiled anew, so a tab open before a late first download
or a refresh gets the new lists. A `$redirect` rule only stops its request, since no stand-in
resource is loaded. A page a list blocks fails the tool with the code `blocked_by_filter`.

**Rationale**: One engine with the syntax the lists are written in, instead of a domain list
that blocks less. Downloading keeps the lists current without a holzi update and keeps their
licences (EasyList: GPLv3 / CC BY-SA 3.0) with their publishers instead of in our binary.

**Rejected**: A snapshot built into the app (stale with every release, licence text to carry); a
plain host list (misses path rules and exceptions).

## R17 Viewing and changing the profile (revision 2026-11-03)

**Decision**: The profile view reads and writes cookies through the running agent browser:
`Network.getAllCookies`, `Network.setCookie` and `Network.deleteCookies` in a session of a page
target (the `Network` domain does not exist on the browser's own target; holzi opens a blank tab
for each call and closes it after), so the values come as the browser holds them and no database file is parsed. "KI →
Browser" starts the browser for it when it is not running (the profile is unlocked then, R3) and
ends it again after 15 minutes without use like any other run (FR-003). The window shows names
and flags, a value only on a click; the commands are window-only and no catalog action an agent can
call (FR-046). Changes are part of the profile and are saved when the browser ends, like cookies
a page set. A cookie is named by its name, its domain as listed (a leading dot makes it another
cookie than one of exactly that host) and its path; a change writes the cookie again with all
other attributes kept, a host cookie by its address so it stays one. The browser limits an expiry
to 400 days and drops a cookie without one (a cookie of the session) when it ends; the window says
the first and refuses an expiry in the past, since deleting is its own action.

**Rationale**: Chromium encrypts cookie values in its files with a key of the operating system;
reading them through the protocol works on every device and needs no second implementation.

## R18 DNS over HTTPS (revision 2026-11-05)

**Decision**: holzi uses Chromium's own secure DNS and sets it through the profile: after the
saved profile is unpacked and before the browser starts, it writes `dns_over_https` into the
`Local State` file of the working directory (`mode`: `off`, `automatic` or `secure`; `templates`:
the provider's URI template), keeping every other key, as it already sets the password-saving
preferences for FR-002. The same file is read by every Chromium-based browser holzi may find
(Chromium, Chrome, Edge, Brave). "Off" is written explicitly, because without the key the browser
upgrades to a provider by itself when the system's resolver is one it knows. The providers are
presets in the code (Quad9 `https://dns.quad9.net/dns-query`, Cloudflare
`https://cloudflare-dns.com/dns-query`, Google `https://dns.google/dns-query`, Mullvad
`https://dns.mullvad.net/dns-query`) plus an own template, which must be `https` with a host and
no user info (at most 2048 characters). The choice is a file `dns.json` in
`<AppLocalData>/agent-browser/`, device-local without sync, and is read when the browser starts;
a running browser keeps what it started with.

**Checked** (Chromium 156, headless, a profile with a seeded `Local State`): with `secure` and a
provider that is not reachable, `https://example.org` does not load; with `automatic` and the same
provider it loads through the system's resolver; with Quad9 it loads in both modes. So the file
takes effect and the modes behave as FR-048 says. Other browsers use the same keys; a manual check
with each is part of M15.

**Rationale**: No second resolver in holzi, no local port, and the user's own browser stays as it
is. The setting is the one the browser's own settings page writes, so it behaves as users expect.

**Rejected**: A DNS proxy in holzi (a second resolver stack, a local port other programs could
use); an operating-system policy (system-wide, needs privileges, would change the user's own
browser); command-line switches (none is documented that sets mode and template, and the
field-trial switches are not supported). Not covered: holzi's own requests (the filter lists)
and a policy of the operating system that sets DNS over HTTPS, which wins over the file.

## R19 Keys, hover, wheel and fullscreen (revision 2026-11-12; FR-049 to FR-052, FR-055, FR-062)

**Decision**: All input goes through `Input.dispatchKeyEvent` and `Input.dispatchMouseEvent` like
the click and the typing of R4, so the page sees trusted events, and through the same `session.own`
wrapper, so the marker (R6) does not take them for the user's input.

- **Keys**: one table in `agent_browser/keys.rs` maps the allowed key names of FR-049 to `key`,
  `code`, `windowsVirtualKeyCode` and, for printable characters, `text`; it is the only place that
  decides what may be pressed, with unit tests. The list is narrow on purpose: `Ctrl+C`/`X`/`V`
  reach the clipboard of the user (the page cannot read it by permission, FR-026, but a paste
  event can), function keys and `Alt` combinations act on the browser. `Ctrl`/`Meta` with A, Z, Y
  send the matching `commands` (`selectAll`, `undo`, `redo`) with the key event, which Chromium
  needs to run an editing command from a synthesized event on some platforms.
- **Enter and Space check**: the page script gets `{ active: true }`, which returns for
  `document.activeElement` (following `shadowRoot.activeElement` down) the same record as
  `{ target }` without scrolling: facts, name, form fields. Step 1 of R5 then decides as for a
  click (Space on a button or checkbox, Enter on a button or in a field of a form). Enter on a
  link navigates; the website check of R9 already holds every navigation.
- **Hover** is a `mouseMoved` to the middle of the element after bringing it into view, then the
  tab settles (the `settle` of R4) and the page is read. It is read-class like scrolling: it
  changes nothing outside the page, and a request the page sends because of it is held by step 2.
- **Wheel** is `mouseWheel` with `deltaX`/`deltaY` at the middle of the element or of the view. The
  script measures the scroll position of the document and of the nearest scrollable ancestor of the
  point before and after, and the result says `moved`. `page` is 0.8 of the view's height (as the
  current scroll step).
- **Fullscreen**: `fullscreenchange` fires in holzi's world too, so the marker script (R6) leaves
  fullscreen with `document.exitFullscreen()` and reports `fullscreen` through the binding; a new
  signal `Signal::Fullscreen` tells the session, which gives the next result the note
  `fullscreen_refused`. The fullscreen is visible for an instant; accepted.

**Spikes before code** (each a fixture and a test that stays, `src-tauri/tests/agent_browser_keys.rs`):

- **S1 Browser accelerators**: does a synthesized `Ctrl+W`, `Ctrl+T`, `Ctrl+L`, `Alt+Left`, `F5`,
  `F11`, `F12` reach the browser's own handling on Linux (X11 and Wayland), macOS and Windows? The
  answer decides whether the refusal list of FR-049 is enough or the call must also be filtered
  after the table; SC-011 pins it.
- **S2 Fullscreen**: does `exitFullscreen()` from the isolated world, called in `fullscreenchange`,
  work for an element and for a video's own fullscreen button on all three platforms?

**Spike results (2026-11-12, Chromium 157, Linux, a visible window, keys sent with
`Input.dispatchKeyEvent` as `rawKeyDown` and `keyUp`)**:

- **S1**: `Ctrl+T` opened a new tab (`chrome://newtab/`) and `Ctrl+N` a new window, and the page
  never received either key: synthesized keys do reach the browser's own accelerators. `Ctrl+L`,
  `Ctrl+W`, `Ctrl+V`, `F5`, `F11`, `F12` and `Alt+Left` were delivered to the page only; the tab
  count, the address, the history and the window state stayed (the tab did not close, no
  developer tools target appeared). So the allowlist of FR-049 is needed, not a blocklist: the
  keys that act on the browser are exactly the ones a short blocklist would forget. macOS and
  Windows are not measured; T107's test stays and runs there (a visible window, so not in CI).
- **S2**: a page's `requestFullscreen()` after a trusted click succeeded; an exit handler that
  runs in holzi's isolated world on `fullscreenchange` left fullscreen again at once. The window
  state stayed `normal`. Not measured for a video's own fullscreen button, macOS and Windows.

**Alternatives rejected**: sending `Alt`/function keys with a blocklist of the dangerous ones (a
blocklist ages badly, an allowlist does not); sending text as `Input.insertText` only (editors that
listen to `keydown` never see it, which is the reason for `mode: keys`).

## R20 The element list (revision 2026-11-12; FR-053, FR-054)

**Decision**: Extend the DOM script (`page_script.js`), not the accessibility tree.

- The facts that step 1 of R5 needs (`tag`, `type`, form membership, the form's fields) come from the
  DOM node; the accessibility tree (`Accessibility.getFullAXTree`) gives roles for custom widgets
  but not these, so a second lookup per node would double the work and a mismatch would weaken the
  guard. One script keeps one implementation and stable numbering.
- **Shadow roots**: the collection walks `querySelectorAll` and descends into every open
  `shadowRoot`. A closed root is not reachable from any world; the marker's own root is closed on
  purpose.
- **Custom widgets**: event listeners cannot be read from an isolated world, so a heuristic lists
  an element that has `tabindex` of 0 or more, or `onclick`, or `cursor: pointer` and is not inside
  an element already listed, with text or a label of at most 80 characters. The scan looks at most
  at 20000 nodes and lists at most 100 such widgets, so a page where everything has a pointer cursor
  does not drown the real elements. If the heuristic still misses widgets in practice,
  `DOMDebugger.getEventListeners` per node is the upgrade path.
- **Frames**: `page::frames` and `script_in` exist (T091 fills in frames); the list continues the
  numbering through the frames of websites the agent may operate, and the tab remembers which
  frame an element number belongs to from the last read. A frame of a website the agent may not
  operate is not listed.
- **Canvas and media**: kinds `canvas` and `media` with `box` (`x`, `y`, `w`, `h` in the view) and
  for media `paused`, `time`, `duration`, `muted`.
- **`browser.media`** calls `play()`, `pause()`, sets `currentTime` and `muted` in holzi's world. The
  autoplay policy would refuse `play()` without a user gesture, so the agent browser starts with
  `--autoplay-policy=no-user-gesture-required`; it also starts with `--mute-audio`, because an
  agent cannot listen and a sound at the user's desk is a surprise (the `muted` of an element is
  its own property and unaffected).

## R21 Screenshots with marks, regions and a view (revision 2026-11-12; FR-056 to FR-058)

**Decision**: holzi captures once as PNG, draws the marks itself and encodes one JPEG.

- **View**: `Page.getLayoutMetrics` gives the scroll position, the size and the zoom of the visible
  area; `Page.getFrameTree` gives the `loaderId` of the main frame, which changes with every
  navigation to a new document. The tab keeps these with the image scale (`min(1280 / width, 1)`,
  the scale the screenshot already uses). A call with a position reads them again and compares; a
  difference is `view_changed`, none stored is `no_view`. A layout that moves under the same scroll
  position is not detected; R22 checks the element really hit.
- **Positions**: image pixel `(x, y)` is CSS pixel `(x / scale, y / scale)` of the view, which is
  what `Input.dispatchMouseEvent` takes.
- **Marks**: the page script returns, for the elements in view, number, kind, name and rectangle
  (`{ marks: true }`, at most 80 in the page's order; the result says when there are more);
  holzi draws a box and a number tag with a built-in 5×7 digit font, so no font file and no new
  dependency. The `image` crate is already a dependency (JPEG, PNG). Drawing in Rust instead of
  an overlay in the page keeps the page from seeing or changing it and is testable with a fixed
  image.
- **Region**: `clip` of `Page.captureScreenshot` with a scale so the crop is at most twice the
  density of the full screenshot and at most 1280 pixels wide. The result states `region` and
  `zoom`, and positions stay in the full space; the tool description tells the model the
  conversion (`x = region.x + px / zoom`).
- Every capture goes through `allows_screenshots` (R7), marks and regions included.

## R22 Positions: clicks and drags (revision 2026-11-12; FR-059 to FR-062)

**Decision**: A position is turned into an element first, and the element's rules apply.

- **What is under the point**: `DOM.getNodeForLocation` gives the node and its frame (it pierces
  shadow roots and out-of-process frames); `DOM.resolveNode` with holzi's world of that frame gives
  an object, and a function call runs the facts collection of `{ target }` on its nearest operable
  ancestor. No operable ancestor means no element: a change, step 2 as backstop, the label of the
  question is "Position (x, y)".
- **Click**: `input::click` takes the button and a count; a double click sends the press and
  release pairs with `clickCount` 1 and 2. A right click fires `contextmenu` in the page.
- **Drag**: `mouseMoved`, `mousePressed`, at least eight interpolated `mouseMoved` with
  `buttons: 1`, `mouseReleased`, inside 3 seconds, in one function that always sends the release,
  also on error or stop. A draggable element would start the operating system's drag; with
  `Input.setInterceptDrags(true)` Chromium reports `Input.dragIntercepted` instead and holzi
  answers with `Input.dispatchDragEvent` (`dragEnter`, `dragOver`, `drop`), then turns
  interception off. Both ends are resolved as above; the stricter classification wins, so dragging
  onto a "Löschen" zone asks.
- **Spikes before code** (`src-tauri/tests/agent_browser_pointer.rs`):
  - **S3 Native drag**: does a drag of an `<img draggable>` and of a `<a>` end cleanly on all three
    platforms with interception, and is no button left held when the page ignores it?
  - **S4 Right click**: does `contextmenu` through CDP leave a native menu open in headful
    Chromium? If yes on any platform, `button: right` is dropped from FR-059 for that platform and the
    contract says so (FR-035's rule: what cannot be done safely is missing, not weaker).
- **Logging**: positions and button, never the image.

## R15 Lieferungen

**Entscheidung**: Wie 021 in PRs aus eigenen Worktrees:

| PR  | Inhalt                                                                                                                                                           |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A   | Spec, Plan, ADR-0019 (proposed), Aufgaben (dieser PR)                                                                                                            |
| B   | Grundlage: `NativeCall` mit Aufrufer und Stufe, native Aktionen über MCP, `confirmsItself` im MCP-Zugang, Rechtegruppe `browser`, Passwort-Zuordnung als Dienst  |
| C   | Agenten-Browser: Finden, Start über Pipe, CDP-Client, Profil verschlüsselt, Lesen, Navigieren mit Website-Prüfung (US1)                                          |
| D   | Bedienen, Absenden-Erkennung, Rückfragen, Markierung, Eingreifen, Stoppen, Sitzungsanzeige (US2, US3)                                                            |
| E0  | vault-sdk: Crate `haex-autofill` mit der Zuordnungsregel der Erweiterung (R7a), eigener PR dort, in holzi gepinnt                                                |
| E1  | Anmeldedaten und andere Daten aus dem Passwortmanager eintragen (US4, R7, R7a)                                                                                   |
| E2  | Downloads und Hochladen (US5)                                                                                                                                    |
| –   | haextension: Erweiterung nutzt den WebAssembly-Build von `haex-autofill` statt ihrer eigenen Regel                                                               |
| F   | zurückgestellt (2026-11-01): Tabs im Browser des Nutzers (US6)                                                                                                   |
| G   | Revision 2026-11-03: dialogs close only through their own buttons (haex-ui `dismissible`, then used in holzi), the send dialog shows what is sent (T095, FR-012) |
| H   | all websites level (FR-043)                                                                                                                                      |
| I   | page request dialog with remembered rules (FR-044)                                                                                                               |
| J   | tracker filters with default and own lists (FR-045, R16)                                                                                                         |
| K   | profile view and editing (FR-046, R17)                                                                                                                           |
| L   | DNS over HTTPS: settings and effect (FR-048, R18)                                                                                                                |
| M   | DNS over HTTPS: settings page                                                                                                                                    |
| N0  | Revision 2026-11-12: spec, research, contract, tasks (this PR)                                                                                                   |
| N1  | Stage A: keys, hover, wheel, fullscreen, spikes S1, S2 (US7, R19)                                                                                                |
| N2  | Stage A: shadow roots, widgets, canvas and media, `browser.media` (US7, R20)                                                                                     |
| N2b | Stage A: elements of frames, routed by number (US7, R20)                                                                                                         |
| N3  | Stage B: view, marks, regions (US8, R21)                                                                                                                         |
| N4  | Stage B: click by position, drag, model gating, spikes S3, S4 (US8, R22)                                                                                         |
