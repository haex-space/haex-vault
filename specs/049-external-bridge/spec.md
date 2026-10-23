# Feature Specification: External Bridge für haex-pass-browser

**Feature Branch**: `049-external-bridge`

**Created**: 2026-10-10

**Status**: Planned (2026-10-21)

**Input**: User description: "Eine External Bridge, damit die bestehende Browser-Erweiterung
`haex-pass-browser` (haextension, `apps/haex-pass-browser`) mit holzi statt mit haex-vault 2.x (Legacy)
arbeitet. Die Erweiterung spricht heute mit haex-vault 2.x über einen verschlüsselten WebSocket
(Standardport 19455, Protokoll v2 mit erklärten Berechtigungen im Handshake, Umschläge mit
Schlüsseltausch und AES-GCM, Zulassung je Client, Ping/Pong, `authorizationUpdate`). Ziel: Die
unveränderte oder nur minimal veränderte Erweiterung verbindet sich mit holzi, wird in holzi
zugelassen und kann ausfüllen (Einträge und TOTP holen), Zugangsdaten speichern und über den
vorhandenen Passkey-Dienst Passkeys anlegen und benutzen. 034 FR-032 und 036 FR-036 verlangen,
dass die Bridge an dieselbe Zugriffsprüfung andockt, ohne das Datenmodell zu ändern. Die
Lesezeichen-Synchronisation der Erweiterung ist zu prüfen und ausdrücklich ein- oder
auszuschließen."

## Begriffe

- **External Bridge**: der Dienst in holzi, der auf dem eigenen Gerät Verbindungen von
  Browser-Erweiterungen annimmt und ihre Anfragen an den Passwortmanager weitergibt. Nicht zu
  verwechseln mit der Brücke zwischen holzi und dem Erweiterungsrahmen einer haextension (Spec 017,
  ADR-0004 Richtung B); beide teilen sich nur die Begriffe Freigabe und Anfrage.
- **Client**: eine Browser-Erweiterung, die sich mit der External Bridge verbindet, hier
  `haex-pass-browser`. Ein Client ist eindeutig durch sein **Schlüsselpaar**, das er beim ersten
  Start in seinem Browserprofil anlegt; seine **Kennung** (Fingerabdruck) folgt aus dem öffentlichen
  Schlüssel. Den **Namen** gibt der Client selbst an („haex-pass Browser Extension“); er ist nicht
  geprüft. Die **Herkunft** ist die Adresse der Erweiterung, die der Browser beim
  Verbindungsaufbau mitschickt (`chrome-extension://…`, `moz-extension://…`).
- **Erklärung**: die Berechtigungen, die ein Client im Handshake erklärt (Protokoll v2). Heute
  erklärt `haex-pass-browser` „Passwörter, Lesen und Schreiben, alle Einträge“.
- **Zulassung**: die Entscheidung des Nutzers in holzi, dass ein Client sich verbinden darf, mit
  seiner **Freigabe** für den Passwortmanager im Sinn von 034 (Art „Lesen“ oder „Lesen und
  Schreiben“, Bereich „alle Einträge“ oder Einträge mit bestimmten Tags) und einem
  **Standard-Tag** für Einträge, die der Client neu anlegt. Eine Zulassung ist **gemerkt** (bleibt
  bestehen) oder **vorläufig** (gilt bis zum Schließen der Vault), wie eine Berechtigung in 017.
  Eine gemerkte Zulassung gilt immer **geräteeigen**: nur auf dem Gerät, auf dem sie erteilt
  wurde.
- **Sperre**: die gemerkte Ablehnung eines Clients. Ein gesperrter Client wird abgewiesen, ohne
  dass holzi fragt, bis der Nutzer die Sperre aufhebt.
- **Methode**: eine benannte Anfrage des Clients, etwa `get-items`. Die Namen sind die von
  `HAEX_PASS_METHODS` aus `apps/haex-pass-browser/src/logic/haexPassApi.ts` (haextension, siehe
  unten).
- **Anwesenheitsbestätigung**: ein Dialog von holzi für genau eine Passkey-Zeremonie (Anlegen oder
  Anmelden), in dem der Nutzer die Gegenstelle, die Herkunft und das Konto sieht und bestätigt.
  Erst sie erlaubt dem Passkey-Dienst, in den Authenticator-Daten „Nutzer anwesend“ (UP) zu
  setzen (ADR-0009).
- **Registrierbare Domain**: die Domain eines Hosts eine Stufe unter seinem öffentlichen Suffix
  (`www.example.de` und `login.example.de` haben beide `example.de`). Für IP-Adressen und
  `localhost` gilt der Host selbst.
- **Aufrufer**, **Freigabe**, **Kopfdaten**, **Einzelabfrage**: wie in Spec 034.
- **Anfrage**, **Erlauben**, **Merken**: wie in Spec 017 (User Story 3).

## Beziehung zu bestehenden Specs

- **Spec 034** (Passwortmanager): Die External Bridge ist der dort in FR-032 vorgesehene weitere
  Aufrufer. Jede Anfrage geht durch den Dienst und seine Zugriffsprüfung (FR-024 bis FR-031 dort)
  mit dem Client als Aufrufer und seiner Freigabe. Datenmodell und Bedeutung einer Freigabe bleiben
  unverändert. Diese Spec schreibt zwei Regeln, die 034 Z11 einer späteren Spec überlässt: Aufrufer
  mit Freigabe lesen den aktuellen **TOTP-Code** (nie das Secret) eines Eintrags im Bereich
  (FR-024) und die **Voreinstellungen des Generators** (FR-027).
- **Spec 036** (Passwortmanager-Redesign): Die Passkey-Funktionen anlegen, bestätigen und auflisten
  (FR-023 bis FR-036 dort) sind die Grundlage der Passkey-Methoden; die Bridge baut keinen zweiten
  Passkey-Weg. Diese Spec liefert den in 036 FR-025 und ADR-0009 offen gelassenen vertrauenswürdigen,
  zeremoniegebundenen Anwesenheitsnachweis (FR-030 bis FR-034); damit ändert sie 036 FR-025 und den
  Vertrag `contracts/passkey-service.md` dort für Zeremonien über die Bridge: UP wird nach einer
  Anwesenheitsbestätigung gesetzt. Zähler 0 (036 FR-026) und Herkunftsprüfung (036 FR-025) bleiben.
- **Spec 017** (Erweiterungs-Host): Anfrage-Dialog, Warteschlange gleicher Anfragen, „Merken“,
  „vorläufig bis zum Schließen der Vault“ und die Regel „Verweigerung schlägt Erteilung“ gelten
  für Clients der Bridge sinngemäß. Ein Client ist keine haextension: Er hat kein Manifest, kein
  Bundle und keine eigenen Tabellen, und er erreicht keine Host-Funktion außer den Methoden dieser
  Spec.
- **ADR-0004**: Die Wand um das Sprachmodell gilt auch hier. Die Bridge erreicht weder Chat noch
  Agent noch ein Modell (FR-041).
- **ADR-0007**: Geheimnisse liegen in der Vault und sind durch Freigaben geschützt. Die Bridge
  ist ein weiterer Eingang mit eigenem Aufrufer; der Aufrufer ergibt sich aus der Verbindung,
  nie aus einer Angabe im Inhalt einer Anfrage.
- **ADR-0009**: Dort steht, dass der Passkey-Dienst weder UP noch UV setzt, bis es einen
  vertrauenswürdigen, zeremoniegebundenen Nachweis gibt. Diese Spec liefert ihn für UP und, mit
  erneuter Eingabe des Vault-Passworts, für UV (FR-033); der Plan hält die Änderung als neues ADR
  fest, das ADR-0009 in diesem Punkt ablöst.
- **ADR-0001** und **ADR-0003**: Gemerkte Zulassungen sind geräteeigene Daten der Vault. Die Bridge
  läuft in der einen Vault-Sitzung des Prozesses und nur, solange sie offen ist.
- **Spec 043** (Android): Auf Android gibt es keine External Bridge (FR-043).
- **Geplante Spec 021** (MCP-Server, externe Agenten): eigener Aufrufer mit eigenen Freigaben;
  diese Spec berührt sie nicht.
- **Vorlage haex-vault 2.x**: Repository `https://github.com/haex-space/haex-vault-legacy`, Revision
  `8dce379d94e18fcd42c3b73686a06f984ca3f574` (dieselbe wie in 034 und 036), Pfade
  `src-tauri/src/external_bridge/` (Protokoll, Umschläge, Zulassung, Server),
  `src/composables/handlers/useCoreExternalRequestHandlers/` (Methoden für Passwörter, Passkeys und
  Lesezeichen), `src/composables/useExternalAuth.ts` und
  `src/components/haex/extension/dialog/external-auth.vue` (Zulassungsdialog),
  `src/components/haex/system/settings/external-clients/` (Verwaltung) und
  `src-tauri/src/extension/permissions/` (Prüfung und Rückfrage). holzi übernimmt das Protokoll so,
  dass die Erweiterung es unverändert spricht, und schließt die Lücken der Vorlage: keine Prüfung
  der Herkunft (FR-005) und der Kennung (FR-006), UP und UV ohne Bestätigung (FR-031), Herkunft
  fest `https://<Kennung>` (FR-032), Aufgabe der Gegenstelle in falscher Kodierung (FR-035),
  Prüfung ausgeschlossener Passkeys über den Bereich hinaus (036 FR-024). haex-vault 2.x ist kein
  Maßstab für Oberfläche.
- **Client haex-pass-browser**: Repository `https://github.com/haex-space/haextension`, Revision
  `ab4e0b0ac9a05488659836a7526a250beab9dfd9` (Merge des Begleit-PR haex-space/haextension#113 und seiner
  Korrektur #114, 2026-10-21; die Planung lief gegen `c8133b94b37ca90f7b17011f4f58f8c9ab91d1f0`,
  die erste Pin `a365af8a89f57a31427b85bfef5fe0f510253d30` spricht dasselbe Protokoll, aber
  `apps/haex-pass/app/api/external.ts` gibt es seitdem nicht mehr), Pfade
  `apps/haex-pass-browser/src/background/` (`protocol.ts`, `connection.ts`, `crypto.ts`,
  `keypairStorage.ts`, `main.ts`, `passkeyOrigin.ts`), `apps/haex-pass-browser/src/contentScripts/`
  (`webauthn-inject.ts`, `webauthn-bridge.ts`, `passkey-consent.ts`),
  `apps/haex-pass-browser/src/logic/haexPassApi.ts` (`HAEX_PASS_METHODS`, Eingaben und Antworten der
  Passwort-Methoden) und `apps/haex-pass-browser/src/bookmarks/vaultClient.ts`.

## Clarifications

### Session 2026-10-10

Die Antworten sind aus dem Freigabemodell von holzi und dem Verhalten von haex-vault 2.x begründet;
der Betreiber hat sie am 2026-10-10 bestätigt.

- Q: Gehört die Lesezeichen-Synchronisation der Erweiterung dazu? → A: Nein. Sie läuft über
  dieselbe Verbindung (Methoden `bookmarks-*`), braucht aber Lesezeichen-Tabellen und eine
  Synchronisationslogik, die holzi nicht hat. holzi beantwortet diese Methoden mit „nicht
  unterstützt“, ohne zu fragen (FR-028). Eine eigene Spec kann sie später nachziehen.
- Q: Läuft die Bridge nur bei offener Vault? → A: Ja. Ohne offene Vault gibt es weder Einträge
  noch Zulassungen. Schließen oder Sperren der Vault beendet alle Verbindungen und gibt den Port
  frei (FR-003). haex-vault 2.x lauscht dagegen ab dem Start der App.
- Q: Ist die Bridge ab Werk eingeschaltet? → A: Nein. Sie öffnet einen Port auf dem Gerät; der
  Nutzer schaltet sie je Gerät in den Einstellungen ein (FR-001). haex-vault 2.x startet sie ab Werk.
- Q: Welcher Port? → A: Ab Werk 19455 wie in haex-vault 2.x, damit die Erweiterung ohne Änderung
  ihrer Einstellung verbindet. Ist der Port belegt (etwa weil haex-vault 2.x läuft), sagt holzi das
  und bietet an, einen anderen zu wählen; die Erweiterung hat dafür schon eine Einstellung.
- Q: Auf welches Freigabemodell bildet holzi die Erklärung ab? → A: Auf die Freigabe aus 034
  (Art und Bereich), ohne Änderung der Semantik. Der Zulassungsdialog zeigt die Erklärung und
  lässt den Nutzer Art und Bereich enger wählen, nie weiter (FR-010). Neue Einträge bekommen das
  Standard-Tag der Zulassung, damit sie im Bereich des Clients bleiben (034 FR-028).
- Q: Wie merkt sich holzi eine Zulassung? → A: Wie eine Berechtigung in 017: mit „Erlaubnis
  merken“ (ab Werk nicht gesetzt) dauerhaft, sonst vorläufig bis zum Schließen der Vault. Gemerkt
  gilt sie nur auf diesem Gerät, weil der Client in einem Browser auf diesem Gerät lebt und sich
  nur mit holzi auf diesem Gerät verbindet.
- Q: Wie liefert holzi zu `get-items` Passwörter, wenn Listen nach 034 FR-026 keine Geheimnisse
  enthalten dürfen? → A: `get-items` ist keine Liste, sondern eine Abfrage für genau eine Adresse.
  holzi sucht die passenden Einträge über die Kopfdaten und liest jeden Treffer mit einer
  Einzelabfrage des Dienstes (FR-020). So bleiben die Regeln des Dienstes unverändert, und die
  Erweiterung bekommt die Form, die sie erwartet.
- Q: Wer bestätigt die Anwesenheit bei Passkeys? → A: Der Nutzer in einem Dialog von holzi, je
  Zeremonie, ohne „Merken“ (FR-030). Die Auswahl in der Erweiterung („über haex-vault 2.x oder den
  Browser“, merkbar je Seite) zählt nicht als Nachweis: holzi kann sie nicht prüfen, und gemerkt
  ist sie nicht an eine Zeremonie gebunden.
- Q: Setzt holzi nach der Anwesenheitsbestätigung auch UV (Nutzer verifiziert)? → A: Nur, wenn
  die Gegenstelle UV verlangt (`userVerification: "required"`) und der Nutzer in der
  Anwesenheitsbestätigung das Passwort der Vault erneut eingibt. Ein Klick auf „Bestätigen“ zeigt
  nur, dass jemand am entsperrten Gerät war, nicht wer; UV ohne erneute Prüfung wäre eine falsche
  Behauptung gegenüber der Gegenstelle (FR-033).
- Q: Gibt es die Bridge auf Android? → A: Nein. holzi läuft auf Android nicht im Hintergrund
  (043 FR-007), und haex-vault 2.x startet sie dort ebenfalls nicht.
- Q: Leitet holzi Anfragen an installierte haextensions weiter (in haex-vault 2.x `requestedExtensions`
  mit Aktionen)? → A: Nein, nur an den Passwortmanager (Ziel `__core__`/`core`); alles andere wird
  abgelehnt (FR-029).

### Session 2026-10-21

Beim Planen gegen den Stand der Erweiterung auf haextension `main` (`c8133b94`) bestätigt der
Betreiber:

- Q: Die Erweiterung ruft `update-item` nicht mehr auf, und `create-item` schickt nur Titel,
  Adresse, Benutzername und Passwort. Baut holzi trotzdem Ändern, TOTP-Angaben und Symbol? → A:
  Nein. holzi bietet nur, was der Client nutzt: `update-item` wird wie die Lesezeichen mit „nicht
  unterstützt“ beantwortet (FR-028), `create-item` übernimmt nur die vier Felder (FR-022), alles
  andere in der Anfrage wird übergangen.
- Q: Gehört eine Änderung an der Erweiterung zu 049? → A: Ja, ein kleiner Begleit-PR in
  haextension: die Antwort von `passkey-list` im Hintergrund korrigieren (ohne sie erreicht keine
  Passkey-Anmeldung holzi) und die Lesezeichen-Synchronisation bei „nicht unterstützt“ als nicht
  verfügbar zeigen. Die Texte nennen weiter „HaexVault“: Das ist der Produktname, den holzi
  übernimmt (haextension#114). Danach wird die Erweiterung neu
  veröffentlicht.
- Q: Wo liegen die Einstellungen der Bridge? → A: In der vorhandenen Kategorie „Erweiterungen“ als
  Abschnitt „Browser-Erweiterungen“ mit eigener Unterseite; keine neue Kategorie.

### Session 2026-10-23

Auslöser war die Frage, ob sich ein Fork von haex-pass-browser oder ein anderes Programm bei holzi
als die Erweiterung ausgeben kann. Die Antworten folgen aus der Herkunftsprüfung (R2) und der
Zulassung; der Betreiber hat die Ergänzung am 2026-10-23 beauftragt.

- Q: Bekommt ein Fork der Erweiterung ohne Zutun des Nutzers Zugriff? → A: Nein. Er erzeugt ein
  eigenes Schlüsselpaar, also eine neue Kennung, und löst den Zulassungsdialog aus (FR-008). Eine
  gemerkte Zulassung gilt nur für das Gerät und die Kennung, für die sie erteilt wurde (FR-009,
  FR-012).
- Q: Erkennt holzi, ob eine Erweiterung die offizielle ist? → A: Bisher nicht. Der Name ist eine
  Angabe des Clients, und jede `chrome-extension://`- oder `moz-extension://`-Herkunft wird
  angenommen; ein Fork mit demselben Namen sieht im Dialog fast aus wie das Original. Neu: holzi
  kennt die Herkunft der offiziellen Erweiterung und sagt im Dialog und in den Einstellungen, ob ein
  Client sie ist, sich nur so nennt, unbekannt ist oder nicht prüfbar (FR-045, FR-046). Das ist
  eine Hilfe für die Entscheidung des Nutzers, keine Zugangsgrenze.
- Q: Und in Firefox? → A: Dort ist die Herkunft je Installation eine zufällige UUID; holzi kann
  sie nicht mit dem Add-on abgleichen. Der Dialog sagt das, statt „bekannt“ oder „unbekannt“ zu
  behaupten.
- Q: Muss jede Instanz einer Erweiterung einzeln zugelassen werden? → A: Ja. Eine Instanz ist das
  Schlüsselpaar einer Installation in einem Browserprofil. Firefox und Chrome auf Gerät A sind zwei
  Instanzen, und dieselbe Erweiterung auf Gerät B ist eine weitere; jede braucht ihre eigene
  Zulassung (FR-047). Das galt schon, war aber nicht als Anforderung festgehalten.
- Q: Beweist ein Client den Besitz seines Schlüssels? → A: Ab Protokoll 3 ja (FR-048). Bisher
  reichten Kennung, öffentlicher Schlüssel und Herkunft, und die sind nicht geheim: Ein lokales
  Programm, das sie kennt, galt als die zugelassene Instanz und konnte blind schreiben. Mit dem
  Nachweis kommt nur durch, wer den privaten Schlüssel der Instanz hat.
- Q: Was geschieht mit Clients ohne Nachweis (haex-pass-browser 1.3.0 und älter)? → A: holzi weist
  sie ab und sagt ihnen, dass sie aktualisieren müssen; keine Übergangszeit und kein Schalter
  (FR-049). Der Betreiber hat das am 2026-10-23 entschieden.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Browser-Erweiterung verbinden und in holzi zulassen (Priority: P1)

Anna nutzt holzi auf ihrem Laptop und hat in Firefox die Erweiterung haex-pass installiert. In den
Einstellungen von holzi schaltet sie die External Bridge ein. Die Erweiterung verbindet sich, und
holzi zeigt einen Dialog: „haex-pass Browser Extension“ (vom Client angegeben), Fingerabdruck,
Herkunft `moz-extension://…`, erklärt: Passwörter lesen und schreiben, alle Einträge. Anna wählt
„Einträge mit Tag web“, Standard-Tag „web“, setzt „Erlaubnis merken“ und lässt zu. Die Erweiterung
zeigt „verbunden“.

**Why this priority**: Ohne Verbindung und Zulassung geht nichts anderes; hier entscheidet sich,
dass nur Clients, die der Nutzer kennt, an Geheimnisse kommen.

**Independent Test**: holzi mit offener Vault und eingeschalteter Bridge, die Erweiterung in der
gepinnten Revision unverändert in Chromium und Firefox laden; Verbindung, Dialog, Zulassen mit und
ohne „Merken“, Neustart von Browser und holzi, erneutes Verbinden ohne Dialog (gemerkt) und mit
Dialog (vorläufig).

**Acceptance Scenarios**:

1. **Given** eine offene Vault und die Bridge eingeschaltet, **When** ein neuer Client sich
   verbindet, **Then** zeigt holzi den Zulassungsdialog mit angegebenem Namen (als Angabe des
   Clients gekennzeichnet), Fingerabdruck, Herkunft und Erklärung, und der Client erfährt
   „wartet auf Zulassung“.
2. **Given** der Dialog, **When** Anna zulässt, **Then** erhält der offene Client sofort die
   Nachricht, dass er zugelassen ist, ohne neu zu verbinden.
3. **Given** eine gemerkte Zulassung, **When** sich derselbe Client nach einem Neustart wieder
   verbindet, **Then** ist er ohne Dialog zugelassen; **Given** eine vorläufige, **When** die
   Vault geschlossen und wieder geöffnet wurde, **Then** fragt holzi erneut.
4. **Given** der Dialog, **When** Anna ablehnt und „merken“ setzt, **Then** wird der Client ab
   jetzt ohne Dialog abgewiesen, bis sie die Sperre in den Einstellungen aufhebt; ohne „merken“
   gilt die Ablehnung bis zum Schließen der Vault.
5. **Given** ein zugelassener Client, **When** er sich mit einer geänderten Erklärung meldet (etwa
   nach einem Update der Erweiterung), **Then** fragt holzi erneut, statt die alte Zulassung zu
   verwenden.
6. **Given** die Bridge ist ausgeschaltet oder keine Vault ist offen, **When** die Erweiterung zu
   verbinden versucht, **Then** findet sie keinen Dienst und zeigt „nicht verbunden“.
7. **Given** der Dialog für einen Client mit der Herkunft der offiziellen Erweiterung, **When** er
   erscheint, **Then** steht dort „Bekannte Erweiterung“ mit ihrem Namen; für einen Client, der
   sich so nennt, aber eine andere Chromium-Herkunft hat, steht dort eine Warnung, dass er sich nur
   so nennt (FR-045).
8. **Given** der Dialog für einen Client aus Firefox, **When** er erscheint, **Then** sagt er, dass
   holzi die Erweiterung dort nicht prüfen kann, und rät, nur zuzulassen, was der Nutzer gerade
   selbst installiert hat (FR-045).
9. **Given** die Erweiterung in Firefox auf Gerät A ist zugelassen und gemerkt, **When** sich die
   Erweiterung in Chrome auf Gerät A oder in einem Browser auf Gerät B mit derselben Vault
   verbindet, **Then** zeigt holzi für jede dieser Instanzen einen eigenen Zulassungsdialog
   (FR-047).

---

### User Story 2 - Anmeldedaten im Browser ausfüllen, auch TOTP (Priority: P1)

Anna öffnet `https://login.example.de`. Die Erweiterung erkennt das Formular, fragt holzi nach
Einträgen für die Adresse und bietet zwei Konten an. Anna wählt eines, Benutzername und Passwort
stehen im Formular. Auf der nächsten Seite verlangt die Seite einen Einmalcode; die Erweiterung
holt den aktuellen Code von holzi.

**Why this priority**: Ausfüllen ist der Hauptzweck der Erweiterung.

**Independent Test**: Einträge mit `https://example.de`, `https://www.example.de/login` und
`https://bad-example.de`, mit und ohne Tag „web“, einer im Papierkorb, einer mit TOTP, einer mit
einem Verweis auf einen Eintrag außerhalb des Bereichs anlegen; `get-items` für
`https://login.example.de` und `get-totp` aufrufen und die Antworten gegen die Erwartung prüfen.

**Acceptance Scenarios**:

1. **Given** Einträge für `example.de` und `www.example.de` mit Tag „web“, **When** der Client
   Einträge für `https://login.example.de/x` holt, **Then** erhält er beide mit Kennung, Titel,
   Adresse, Benutzername, Passwort, eigenen Feldern, Autofill-Aliasen und dem Hinweis auf TOTP; den
   Eintrag für `bad-example.de` erhält er nicht.
2. **Given** ein passender Eintrag ohne Tag aus dem Bereich, im Papierkorb oder einer holzi-Funktion
   (034 Z14), **When** der Client Einträge holt, **Then** fehlt er, ohne dass die Antwort sein
   Dasein verrät, und holzi fragt nicht.
3. **Given** ein Eintrag mit TOTP im Bereich, **When** der Client den Code holt, **Then** erhält er
   den aktuellen Code und die Restzeit in Sekunden, nie das Secret.
4. **Given** ein Benutzername, der auf einen Eintrag außerhalb des Bereichs verweist, **When** der
   Client den Eintrag holt, **Then** fehlt dieses Feld (ADR-0009), und alle übrigen sind da.
5. **Given** eine Anfrage ohne Adresse, **When** der Client sie schickt, **Then** lehnt holzi ab;
   es gibt keinen Weg, alle Einträge auf einmal zu holen.

---

### User Story 3 - Neue Zugangsdaten aus dem Browser speichern (Priority: P2)

Anna legt bei einem Shop ein Konto an. Im Popup der Erweiterung lässt sie ein Passwort nach ihrer
Standard-Voreinstellung aus holzi erzeugen und speichert Titel, Adresse, Benutzername und Passwort.
Der Eintrag erscheint sofort im Passwortmanager von holzi, mit dem Tag „web“.

**Why this priority**: Ohne Speichern muss Anna neue Konten von Hand in holzi übertragen; es baut auf
US1 auf.

**Independent Test**: Mit Freigabe „Lesen und Schreiben“ für Tag „web“ und Standard-Tag „web“
Voreinstellungen holen, einen Eintrag anlegen und prüfen, dass er Tag, Verlauf und Werte hat und
übergangene Felder (Ordner, TOTP, Symbol) nicht übernommen sind; dasselbe mit „Lesen“ (Anfrage in
holzi); `update-item` scheitert mit „nicht unterstützt“.

**Acceptance Scenarios**:

1. **Given** eine Zulassung mit „Lesen“, **When** der Client die Voreinstellungen des Generators
   holt, **Then** erhält er die Standard-Voreinstellung und die Liste aller Voreinstellungen.
2. **Given** eine Zulassung „Lesen und Schreiben“ mit Standard-Tag „web“, **When** der Client einen
   Eintrag anlegt, **Then** entsteht er mit dem Tag „web“ und einem ersten Stand im Verlauf, und
   der Passwortmanager zeigt ihn ohne Neuladen.
3. **Given** derselbe Client, **When** er `update-item` schickt, **Then** antwortet holzi „nicht
   unterstützt“, ohne zu fragen, und nichts ändert sich (FR-028).
4. **Given** eine Zulassung nur mit „Lesen“, **When** der Client anlegen will, **Then** zeigt holzi
   eine Anfrage wie in 017; erlaubt Anna, gelingt es, lehnt sie ab oder antwortet sie nicht
   rechtzeitig, scheitert es und nichts ändert sich.
5. **Given** eine Anlege-Anfrage mit zusätzlichen Feldern (`groupId`, TOTP-Angaben, `iconBase64`,
   `tags`), **When** der Client speichert, **Then** legt holzi den Eintrag nur mit Titel, Adresse,
   Benutzername, Passwort und Standard-Tag an.

---

### User Story 4 - Mit Passkey anmelden und Passkey anlegen (Priority: P2)

Anna registriert bei einer Seite einen Passkey. Die Erweiterung fragt sie, ob haex-pass oder der
Browser ihn verwalten soll; sie wählt haex-pass. holzi kommt nach vorn und zeigt: „Passkey für
example.de als anna@example.de anlegen? An Eintrag: Neuer Eintrag“. Sie bestätigt. Später meldet
sie sich mit dem Passkey an; holzi fragt wieder, diesmal mit der Wahl zwischen ihren zwei Konten
bei der Seite.

**Why this priority**: Passkeys sind der zweite Zweck der Erweiterung, aber ohne US1 und US2 nicht
nutzbar; die Anmeldung braucht den Begleit-PR an der Erweiterung (siehe unten).

**Independent Test**: Gegen eine Test-Gegenstelle mit einer gängigen WebAuthn-Server-Bibliothek, die
UP verlangt: Passkey über die Bridge anlegen (mit Bestätigung), anmelden (mit Bestätigung),
Signatur und Flags von der Gegenstelle prüfen lassen; ohne Bestätigung, mit falscher Herkunft und
mit abgelaufener Bestätigung darf nichts entstehen und nichts signiert werden.

**Acceptance Scenarios**:

1. **Given** eine Zulassung „Lesen und Schreiben“, **When** der Client einen Passkey anlegen will,
   **Then** zeigt holzi eine Anwesenheitsbestätigung mit Gegenstelle, Herkunft, Benutzer und der
   Wahl des Eintrags (neuer Eintrag ab Werk, oder ein vorhandener im Bereich mit derselben
   registrierbaren Domain); erst nach „Bestätigen“ entsteht der Passkey am Eintrag, und die
   Gegenstelle akzeptiert die Registrierung.
2. **Given** ein Passkey im Bereich, **When** der Client sich anmelden will und Anna bestätigt,
   **Then** erhält er eine Signatur, die die Gegenstelle mit gesetztem UP annimmt, und „zuletzt
   benutzt“ ist gesetzt; der Zähler bleibt 0.
3. **Given** mehrere passende Passkeys, **When** der Client sich anmelden will, **Then** wählt Anna
   das Konto in der Anwesenheitsbestätigung.
4. **Given** eine Anfrage, deren Herkunft nicht zur Gegenstelle passt oder die keine Herkunft
   nennt, **When** der Client sie schickt, **Then** lehnt holzi ab, ohne einen Dialog zu zeigen, und
   die Erweiterung fällt auf den Browser zurück.
5. **Given** eine Anwesenheitsbestätigung, **When** Anna ablehnt oder nicht innerhalb der Frist
   antwortet, **Then** entsteht nichts, nichts wird signiert, und der Client erhält eine Ablehnung.
6. **Given** eine Zulassung nur mit „Lesen“, **When** der Client einen Passkey anlegen will,
   **Then** fragt holzi zuerst nach der Freigabe (wie US3, Szenario 4) und danach nach der
   Anwesenheit.

---

### User Story 5 - Zugelassene Clients verwalten, ändern und widerrufen (Priority: P2)

Anna öffnet in den Einstellungen von holzi den Punkt „Browser-Erweiterungen“. Sie sieht, dass die
Bridge auf Port 19455 läuft, ihre Firefox-Erweiterung zugelassen ist (Tag „web“, Lesen und
Schreiben, zuletzt verbunden vor fünf Minuten) und eine alte Chromium-Installation gesperrt ist.
Sie engt die Firefox-Zulassung auf „Lesen“ ein und widerruft eine dritte, die sie nicht mehr
nutzt.

**Why this priority**: Ohne Übersicht und Widerruf bleibt eine Zulassung unkontrolliert bestehen;
es baut auf US1 auf.

**Independent Test**: Zwei Clients zulassen, einen sperren; in den Einstellungen Art und Bereich
ändern, widerrufen, Sperre aufheben, Port ändern und die Bridge aus- und einschalten; jeweils das
Verhalten offener Verbindungen prüfen.

**Acceptance Scenarios**:

1. **Given** zugelassene und gesperrte Clients, **When** Anna die Einstellungen öffnet, **Then**
   sieht sie je Client angegebenen Namen, Fingerabdruck, Herkunft, Art, Bereich, Standard-Tag,
   gemerkt oder vorläufig, Zeitpunkt der Zulassung und letzte Verbindung.
2. **Given** ein verbundener Client, **When** Anna ihn widerruft, **Then** erhält er sofort die
   Nachricht, dass er nicht mehr zugelassen ist, wartende Anfragen scheitern, und beim nächsten
   Verbinden fragt holzi neu.
3. **Given** ein verbundener Client, **When** Anna Art oder Bereich ändert, **Then** gilt die
   Änderung ab der nächsten Anfrage.
4. **Given** der Port ist belegt, **When** Anna die Bridge einschaltet, **Then** zeigt holzi „Port
   belegt“ mit dem Hinweis, dass eine andere holzi-Instanz oder haex-vault 2.x ihn nutzen kann, und
   bietet an, einen anderen Port zu wählen.

---

### User Story 6 - Webseiten und fremde Programme kommen nicht durch (Priority: P1)

Eine Webseite, die Anna besucht, versucht im Hintergrund, sich mit `ws://localhost:19455` zu
verbinden und sich als „haex-pass Browser Extension“ auszugeben. holzi weist sie ab, bevor ein
Dialog erscheint. Ein Client mit einer gefälschten Kennung kommt ebenso nicht durch.

**Why this priority**: Die Bridge öffnet einen Port, den jede Seite im Browser erreicht. Was hier
nicht stimmt, macht die Zulassung wertlos, weil der Nutzer einem gefälschten Dialog zustimmen
könnte.

**Independent Test**: Verbindungen mit einer Webseiten-Herkunft (`https://…`, `http://…`, `null`),
ohne Herkunft, mit Protokoll v1, ohne Erklärung, mit einer Kennung, die nicht zum Schlüssel passt,
mit zu großen Nachrichten und mit vielen Handshakes hintereinander aufbauen und prüfen, dass keiner
einen Dialog oder eine Antwort mit Daten auslöst.

**Acceptance Scenarios**:

1. **Given** eine Verbindung, deren Herkunft keine Browser-Erweiterung ist, **When** sie aufgebaut
   wird, **Then** schließt holzi sie vor dem Handshake, ohne Dialog.
2. **Given** ein Handshake mit Protokoll v1 oder ohne Erklärung, **When** er ankommt, **Then**
   antwortet holzi mit dem Fehler `PERMISSIONS_DECLARATION_REQUIRED` und schließt die Verbindung.
3. **Given** ein Handshake, dessen Kennung nicht aus dem mitgeschickten öffentlichen Schlüssel
   folgt, **When** er ankommt, **Then** weist holzi ihn ab, ohne Dialog.
4. **Given** derselbe unbekannte Client verbindet sich hundertmal hintereinander, **When** holzi
   fragt, **Then** steht genau ein Zulassungsdialog für ihn da.
5. **Given** ein Programm kennt Kennung, öffentlichen Schlüssel und Herkunft einer zugelassenen
   Instanz, aber nicht ihren privaten Schlüssel, **When** es sich damit verbindet, **Then** kann es
   die Aufgabe nicht beantworten, und holzi schließt die Verbindung ohne Dialog und ohne eine
   Anfrage auszuführen (FR-048).
6. **Given** ein Client mit Protokoll 2 (haex-pass-browser 1.3.0 oder älter), **When** er sich
   verbindet, **Then** antwortet holzi mit dem Fehler `PROTOCOL_UPDATE_REQUIRED` und schließt, ohne
   Dialog (FR-049).
7. **Given** die Antwort auf eine Aufgabe aus einer anderen Verbindung, **When** ein Client sie
   schickt, **Then** weist holzi ihn ab wie bei einer falschen Antwort.

### Edge Cases

- **haex-vault 2.x läuft auf demselben Gerät**: Beide wollen Port 19455. Wer zuerst lauscht, hat ihn;
  holzi meldet „Port belegt“ (US5). Die Erweiterung verbindet sich mit dem, der lauscht; welchen
  Dienst sie erreicht, sieht der Nutzer daran, wo der Zulassungsdialog erscheint.
- **Zwei holzi-Instanzen** (Spec 030, je eine Vault): Nur eine kann die Bridge auf einem Port
  betreiben; die andere meldet „Port belegt“. Es gibt keine automatische Ausweichportwahl, weil die
  Erweiterung den neuen Port nicht kennen würde.
- **Vault wird geschlossen oder gesperrt, während eine Anfrage, ein Zulassungsdialog oder eine
  Anwesenheitsbestätigung offen ist**: Alles endet mit einer Ablehnung; nichts wird halb
  geschrieben oder signiert.
- **Zwei Browser oder zwei Profile auf einem Gerät**: Zwei Instanzen mit je eigenem Schlüssel;
  holzi fragt für jede (FR-047). Firefox Sync und Chrome Sync übertragen den Schlüssel nicht, weil
  er in der IndexedDB des Profils liegt.
- **Erweiterung neu installiert oder Browserprofil gewechselt**: Neues Schlüsselpaar, also ein neuer
  Client mit neuem Fingerabdruck; holzi fragt. Der alte bleibt in der Liste, bis der Nutzer ihn
  widerruft.
- **Zwei Clients mit demselben angegebenen Namen**: Der Dialog unterscheidet sie an Fingerabdruck
  und Herkunft; den Namen kennzeichnet er als Angabe des Clients und sagt, ob die Herkunft die einer
  bekannten Erweiterung ist (FR-045).
- **Offizielle Erweiterung aus dem GitHub-Release oder selbst gebaut**: Chromium vergibt entpackten
  und per Hand installierten Erweiterungen eine andere Kennung als der Store. holzi zeigt sie als
  „nennt sich so“ und sagt im Text dazu, dass das bei einer Installation außerhalb des Stores zu
  erwarten ist; zulassen kann der Nutzer sie wie jeden anderen Client.
- **Anderes eigenes Gerät**: Eine gemerkte Zulassung von Gerät A gilt auf Gerät B nicht; ein
  Browser auf Gerät B wird dort neu zugelassen.
- **Adresse mit IP-Adresse oder `localhost`**: Treffer nur bei gleichem Host; eine registrierbare
  Domain gibt es nicht.
- **Eintrag mit mehreren Adressen oder ohne Adresse**: Ohne Adresse ist er für `get-items` kein
  Treffer; Abgleich wie heute über das Adressfeld aus 034.
- **Sehr viele Treffer für eine Adresse**: holzi liefert alle Treffer im Bereich; jeder ist eine
  Einzelabfrage des Dienstes (SC-002 begrenzt die Zeit).
- **Feld mit einem Verweis, dessen Kette zu tief ist oder einen Kreis bildet**: Das Feld fehlt in
  der Antwort, wie bei einem Verweis außerhalb des Bereichs; holzi liefert nie einen leeren oder
  wörtlichen Platzhalter als Wert (ADR-0009).
- **Weitere Felder beim Anlegen** (`groupId`, `iconBase64`, `otpSecret` und Verwandte, `tags`):
  holzi übergeht sie und legt den Eintrag ohne sie an (FR-022). Ordner zu wählen steht Aufrufern von
  außen ohnehin nicht offen (034 Z11).
- **Passkey-Anfrage ohne Herkunft** (Erweiterungen vor `e0ee77f`): Abgelehnt (036 FR-025 schließt die
  Annahme `https://<Kennung>` aus). Die Erweiterung fällt bei jedem Fehler auf den Passkey-Dialog
  des Browsers zurück; der Nutzer sieht also den Browser-Dialog statt eines Fehlers.
- **Client gibt vor der Anwesenheitsbestätigung auf** (Zeitlimit der Erweiterung): holzi bemerkt das
  nur, wenn die Verbindung endet; dann schließt es den Dialog. Bestätigt der Nutzer einen
  Passkey-Anlegen-Dialog, nachdem der Client aufgegeben hat, bleibt ein Passkey am Eintrag, den die
  Gegenstelle nie bekommen hat; der Nutzer kann ihn im Tab Extra löschen. Die Wartezeit der
  Erweiterung von 130 Sekunden macht das selten.
- **Gleichzeitige Passkey-Zeremonien**: Anwesenheitsbestätigungen stehen nacheinander in der
  Warteschlange; jede gehört zu genau einer Anfrage.
- **Ausgeschlossener Passkey existiert schon** (`excludeCredentials`): Es entsteht kein neuer, der
  Client erhält die Ablehnung aus 036 FR-024, und es erscheint keine Anwesenheitsbestätigung.
  Passkeys außerhalb des Bereichs zählen nicht (036 FR-030).
- **Importierter RS256-Passkey**: Anmelden wird abgelehnt (036 FR-034); keine Bestätigung.
- **holzi-Fenster minimiert oder auf einer anderen Arbeitsfläche**: holzi holt sich für Zulassung,
  Anfrage und Anwesenheitsbestätigung nach vorn; wo das System das verhindert, zeigt es eine
  Systembenachrichtigung, die zum Dialog führt.
- **Unbekannte Methode oder Ziel eine haextension**: Abgelehnt ohne Rückfrage und ohne Wirkung.
- **Lesezeichen-Methoden**: Abgelehnt mit „nicht unterstützt“; die Passwort-Funktionen derselben
  Verbindung arbeiten weiter.
- **Uhr des Geräts falsch**: TOTP-Codes sind falsch wie in der Oberfläche von holzi; die Bridge
  korrigiert nichts.

## Requirements _(mandatory)_

### Functional Requirements

**Dienst und Verbindung**

- **FR-001**: holzi MUSS eine External Bridge anbieten, die der Nutzer je Gerät in den
  Einstellungen ein- und ausschaltet; ab Werk ist sie aus. Die Einstellung und der Port MÜSSEN
  geräteeigen sein.
- **FR-002**: Die Bridge MUSS ausschließlich auf der Loopback-Schnittstelle des Geräts lauschen
  (nie auf anderen Netzschnittstellen; IPv4 und, wo vorhanden, IPv6, weil die Erweiterung
  `localhost` aufruft), ab Werk auf Port 19455; der Nutzer MUSS den Port ändern
  können. Ist der Port belegt, MUSS holzi das in den Einstellungen anzeigen und DARF NICHT still
  auf einen anderen Port ausweichen.
- **FR-003**: Die Bridge DARF nur lauschen, solange eine Vault offen ist. Schließen oder Sperren der
  Vault und Ausschalten der Bridge MÜSSEN alle Verbindungen beenden, offene Anfragen,
  Zulassungsdialoge und Anwesenheitsbestätigungen mit einer Ablehnung beenden, vorläufige
  Zulassungen und Ablehnungen verwerfen und den Port freigeben.
- **FR-004**: Die Bridge MUSS das Protokoll von `haex-pass-browser` in der gepinnten Revision so
  sprechen, dass die Erweiterung ohne Änderung verbindet, zugelassen wird und Passwort-Methoden
  aufruft: Nachrichtenarten Handshake, Handshake-Antwort, verschlüsselte Anfrage und Antwort,
  `authorizationUpdate`, Ping/Pong und Fehler; Umschläge mit Schlüsseltausch je Nachricht und
  AES-256-GCM wie in haex-vault 2.x; Antworten mit `requestId`, `success`, `data`, `error` und, wo
  haex-vault 2.x ihn sendet, `errorCode`.
- **FR-005**: holzi MUSS eine Verbindung vor dem Handshake schließen, wenn der Browser als Herkunft
  keine Browser-Erweiterung meldet (etwa `https://…`, `http://…`, `null`) oder keine Herkunft
  mitschickt. Andere Programme als Browser-Erweiterungen sind in dieser Spec keine Clients.
- **FR-006**: holzi MUSS einen Handshake abweisen, dessen Kennung nicht aus dem mitgeschickten
  öffentlichen Schlüssel folgt (Berechnung wie in `haex-pass-browser`), und einen ohne Erklärung
  mit `PERMISSIONS_DECLARATION_REQUIRED` beantworten und schließen. Ältere Protokolle regelt
  FR-049.
- **FR-007**: holzi MUSS Nachrichten über einer festen Größe und unlesbare Nachrichten ablehnen,
  ohne abzustürzen; die Grenze legt der Plan fest. Ein Client DARF höchstens einen offenen
  Zulassungsdialog haben, egal wie oft er sich verbindet.

**Zulassung**

- **FR-008**: Verbindet sich ein Client, der weder zugelassen noch gesperrt ist, MUSS holzi ihm
  „wartet auf Zulassung“ antworten und den Zulassungsdialog zeigen. Der Dialog MUSS den angegebenen
  Namen (gekennzeichnet als Angabe des Clients), den Fingerabdruck, die Herkunft und die Erklärung
  in Worten zeigen. Er ist ein Dialog von holzi, den der Client weder gestalten noch beantworten
  kann.
- **FR-009**: Der Dialog MUSS „Zulassen“, „Ablehnen“ und den Haken „Erlaubnis merken“ (ab Werk nicht
  gesetzt) bieten. Mit Haken MUSS holzi die Zulassung oder Sperre geräteeigen speichern; ohne Haken
  gilt sie bis zum Schließen der Vault.
- **FR-010**: Beim Zulassen MUSS der Nutzer die Freigabe festlegen: Art („Lesen“ oder „Lesen und
  Schreiben“) und Bereich („alle Einträge“ oder Einträge mit gewählten Tags), höchstens so weit wie
  erklärt, ab Werk wie erklärt. Für „Lesen und Schreiben“ MUSS er ein Standard-Tag wählen, das im
  Bereich liegt; bei „alle Einträge“ DARF er es leer lassen. Die Freigabe hat genau die Bedeutung aus
  034 FR-025.
- **FR-011**: Nach dem Zulassen MUSS holzi einem offenen Client sofort `authorizationUpdate` mit
  „zugelassen“ schicken; nach Ablehnen oder Widerruf `authorizationUpdate` mit „nicht zugelassen“
  und danach die Verbindung schließen.
- **FR-012**: holzi MUSS die Erklärung eines Clients bei der Zulassung speichern und bei jedem
  Handshake vergleichen, ohne Rücksicht auf die Reihenfolge der Einträge. Weicht sie ab, MUSS holzi
  erneut fragen und bis dahin keine Anfrage ausführen.
- **FR-013**: Zulassen, Ändern, Widerrufen, Sperren und Entsperren eines Clients DÜRFEN nur durch den
  Nutzer in der Oberfläche von holzi geschehen; kein Client, keine haextension und keine Aktion des
  eingebauten Agenten oder externer Agenten DARF sie auslösen oder beantworten.

**Freigaben und Anfragen**

- **FR-014**: Jede Methode MUSS über den Dienst des Passwortmanagers mit einem eigenen Aufrufer für
  den Client laufen (Kennung des Clients aus der Verbindung, nie aus dem Inhalt einer Anfrage) und
  mit der Freigabe seiner Zulassung. Kein Weg DARF die Tabellen am Dienst vorbei lesen oder
  schreiben (034 FR-024).
- **FR-015**: Verlangt eine Methode die Art „Lesen und Schreiben“ und hat die Zulassung nur „Lesen“,
  MUSS holzi eine Anfrage wie in 017 zeigen (Erlauben, Ablehnen, Merken; Merken erweitert die
  Freigabe der Zulassung), aber nur, wenn die Handshake-Erklärung des Clients Schreiben einschließt.
  Fehlt Schreiben in der Erklärung, MUSS holzi die Methode ohne Anfrage ablehnen; auch eine gemerkte
  Erweiterung darf nie über die erklärte Freigabe hinausgehen. Aktiviert die Anfrage das Anlegen von
  Einträgen, MUSS der Nutzer vor dem Zulassen einen Standard-Tag innerhalb des tag-begrenzten Bereichs
  wählen; das gilt auch, wenn er die Erweiterung merkt. holzi MUSS die Antwort an den Client bis zu
  120 Sekunden zurückhalten; danach scheitert die Methode mit `PERMISSION_PROMPT_TIMEOUT`. Eine
  gemerkte Ablehnung MUSS ohne Anfrage gelten.
- **FR-016**: Für Einträge außerhalb des Bereichs DARF holzi nie fragen; sie sind für den Client nicht
  vorhanden (034 FR-029), damit eine Frage ihr Dasein nicht verrät.
- **FR-017**: Ein Client DARF keine Einträge löschen und weder Ordner, Tags, Papierkorb, Verlauf,
  Anhänge noch Passkeys ändern oder löschen; die Bridge bietet dafür keine Methode. Einzige
  Ausnahme ist das Standard-Tag, das holzi beim Anlegen setzt (FR-022, FR-034).

**Methoden für Passwörter**

- **FR-018**: Die Bridge MUSS die Methoden `get-items`, `get-totp`, `create-item`,
  `get-password-config`, `get-password-presets`, `passkey-create`, `passkey-get` und `passkey-list`
  mit den Eingaben und Antworten anbieten, die die Erweiterung in der gepinnten Revision schickt und
  liest (`logic/haexPassApi.ts`, `background/connection.ts`). Verlangte Art: „Lesen“ für
  `get-items`, `get-totp`, `get-password-config`, `get-password-presets`, `passkey-get` und
  `passkey-list`; „Lesen und Schreiben“ für `create-item` und `passkey-create`.
- **FR-019**: `get-items` MUSS eine Adresse verlangen und sonst ablehnen. Treffer sind Einträge im
  Bereich, deren Adresse dieselbe registrierbare Domain hat wie die angefragte (bei IP-Adressen und
  `localhost` derselbe Host); Einträge, die nur im Text ähnlich sind (`bad-example.de`), sind keine
  Treffer. Optional angegebene Feldnamen schränken die Treffer auf Einträge mit einem dieser Felder
  ein.
- **FR-020**: holzi MUSS die Treffer über die Kopfdaten des Dienstes finden und jeden Treffer mit
  einer Einzelabfrage des Dienstes lesen. Die Antwort MUSS je Treffer Kennung, Titel (ohne Titel
  „Untitled“ wie haex-vault 2.x), Adresse, Felder (Benutzername, Passwort, eigene Felder und die Marke
  `otp: "TOTP"`, wenn TOTP eingerichtet ist), den Hinweis auf TOTP und die Autofill-Aliase
  enthalten; nie das TOTP-Secret, Notiz, Anhänge oder Passkey-Schlüssel.
- **FR-021**: Verweise in Feldern (ADR-0009) MUSS der Dienst auflösen; ein Feld, dessen Verweis eine
  Quelle außerhalb des Bereichs hat oder nicht auflösbar ist, MUSS in der Antwort fehlen.
- **FR-022**: `create-item` MUSS einen Eintrag mit Titel (ohne Titel der Host der Adresse),
  Adresse, Benutzername und Passwort anlegen, mit dem Standard-Tag der Zulassung, in der obersten
  Ebene, mit erstem Stand im Verlauf, und Kennung und Titel zurückgeben. Alle anderen Felder der
  Anfrage MUSS holzi übergehen (Clarification 2026-10-21).
- **FR-023**: _(entfallen 2026-10-21)_ `update-item` gehört zu den nicht unterstützten Methoden
  (FR-028).
- **FR-024**: `get-totp` MUSS für einen Eintrag im Bereich den aktuellen Code und die Restzeit in
  Sekunden liefern, berechnet im Dienst; das Secret DARF den Dienst nicht verlassen. Für einen
  Eintrag ohne TOTP oder außerhalb des Bereichs antwortet holzi gleich (nicht gefunden).
- **FR-025**: Ein Eintrag, den ein Client anlegt oder ändert, MUSS im Passwortmanager ohne Neuladen
  erscheinen (034 FR-038) und wie jeder Eintrag synchronisieren.
- **FR-026**: Ändern sich Einträge, während ein Client verbunden ist, DARF holzi ihm nichts von sich
  aus schicken; der Client fragt bei Bedarf neu.
- **FR-027**: `get-password-config` und `get-password-presets` MÜSSEN die Voreinstellungen des
  Generators (034) mit jeder Passwort-Freigabe liefern; sie enthalten keine Geheimnisse.
- **FR-028**: Die Methoden `update-item`, `bookmarks-collections-list`, `bookmarks-collection-create`,
  `bookmarks-list`, `bookmarks-upsert`, `bookmarks-delete` und `bookmarks-device-upsert` MUSS holzi
  ohne Anfrage und ohne Wirkung mit einer Ablehnung „nicht unterstützt“ beantworten; die Verbindung
  bleibt bestehen.
- **FR-029**: Jede andere Methode und jede Anfrage an ein anderes Ziel als den Passwortmanager (in
  haex-vault 2.x `__core__`/`core`) MUSS holzi ohne Anfrage und ohne Wirkung ablehnen.

**Passkeys und Anwesenheitsbestätigung**

- **FR-030**: Vor jedem `passkey-create` und `passkey-get` MUSS holzi nach Prüfung von Freigabe und
  Herkunft eine Anwesenheitsbestätigung zeigen: Client, Gegenstelle (Name und Kennung), Herkunft,
  Benutzer, beim Anmelden die Wahl des Kontos, wenn mehrere passen (036 `ChoiceRequired`), beim
  Anlegen die Wahl des Eintrags (ab Werk „Neuer Eintrag“, sonst ein vorhandener Eintrag im Bereich
  mit derselben registrierbaren Domain). Sie bietet „Bestätigen“ und „Ablehnen“, kein „Merken“, und
  verfällt nach 120 Sekunden als Ablehnung.
- **FR-031**: Eine Anwesenheitsbestätigung MUSS an genau eine Anfrage gebunden sein (Client,
  Gegenstelle, Herkunft, Aufgabe der Gegenstelle) und DARF nur einmal gelten. Erst nach ihr DARF
  der Passkey-Dienst anlegen oder signieren und MUSS dann UP setzen. Ohne sie, nach Ablehnen,
  Verfallen, Schließen der Vault oder Ende der Verbindung DARF nichts entstehen und nichts signiert
  werden.
- **FR-032**: Anfragen ohne Herkunft oder mit einer Herkunft, die nicht zur Kennung der Gegenstelle
  passt (036 FR-025), MUSS holzi ablehnen, bevor es eine Anwesenheitsbestätigung zeigt. holzi DARF
  die Herkunft nicht aus der Kennung der Gegenstelle ableiten.
- **FR-033**: UV (Nutzer verifiziert) DARF holzi nur setzen, wenn die Gegenstelle UV verlangt
  (`userVerification: "required"`) und der Nutzer in derselben Anwesenheitsbestätigung das Passwort
  der Vault erneut richtig eingegeben hat; nur dann MUSS die Anwesenheitsbestätigung danach fragen.
  Bei `"preferred"` und `"discouraged"` MUSS holzi UV nicht setzen und DARF nicht nach dem Passwort
  fragen. Ein falsches Passwort oder ein Abbruch der Eingabe MUSS wie Ablehnen wirken (FR-031).
  Anders als haex-vault 2.x, das UV immer setzt, behauptet holzi UV nie ohne Prüfung.
- **FR-034**: Für „Neuer Eintrag“ MUSS holzi zuerst einen Eintrag mit dem Namen der Gegenstelle als
  Titel, der Herkunft als Adresse, dem Benutzer als Benutzername und dem Standard-Tag der Zulassung
  anlegen und den Passkey daran; scheitert das Anlegen des Passkeys, MUSS auch der Eintrag
  entfallen. Passkeys entstehen immer mit ES256, weil die Erweiterung diesen Algorithmus fest
  meldet; fehlt die Liste der Algorithmen der Gegenstelle, gilt ES256.
- **FR-035**: Die Bridge MUSS Kodierungen zwischen der Erweiterung (Base64 mit Auffüllung) und dem
  Passkey-Dienst (Base64URL ohne Auffüllung) übersetzen; die Aufgabe der Gegenstelle MUSS in
  `clientDataJSON` so stehen, wie die Gegenstelle sie erwartet. Antworten MÜSSEN die Felder liefern,
  die die Erweiterung liest (für Anlegen Kennung, öffentlicher Schlüssel, Beglaubigung,
  `clientDataJSON`, Transportarten, interne Passkey-Kennung; für Anmelden Kennung,
  Authenticator-Daten, Signatur, `clientDataJSON`, Benutzerkennung bei auffindbaren Passkeys).
- **FR-036**: `passkey-list` MUSS die Kopfdaten aus 036 FR-027 im Bereich liefern, nie Schlüssel.

**Verwaltung**

- **FR-037**: Die Einstellungen MÜSSEN Ein/Aus, Port und Zustand der Bridge (aus, läuft, Port belegt,
  keine Vault offen, auf dieser Plattform nicht verfügbar) zeigen und je Client angegebenen Namen,
  Fingerabdruck, Herkunft, Art, Bereich, Standard-Tag, gemerkt oder vorläufig, Zeitpunkt der
  Zulassung und letzte Verbindung; gesperrte Clients in einer eigenen Liste.
- **FR-038**: Der Nutzer MUSS Art, Bereich und Standard-Tag eines Clients ändern, ihn widerrufen und
  eine Sperre aufheben können. Ein Widerruf MUSS sofort gelten, auch für offene Verbindungen und
  wartende Anfragen (FR-011); eine Änderung ab der nächsten Anfrage.

**Schutz der Geheimnisse**

- **FR-039**: Geheimnisse, Adressen aus Anfragen und Inhalte von Antworten DÜRFEN weder in Protokolle,
  Fehlermeldungen, Benachrichtigungen noch in Aktionsprotokolle gelangen, auch nicht auf
  Debug-Stufe. Protokolle DÜRFEN Methode, Fingerabdruck des Clients und Art des Ergebnisses
  enthalten.
- **FR-040**: Der private Schlüssel eines Passkeys und das TOTP-Secret DÜRFEN keine Antwort der Bridge
  erreichen (036 FR-028, FR-024).
- **FR-041**: Die Bridge DARF weder Chat, eingebauten Agenten, externe Agenten noch ein Modell
  erreichen; ein Vertragstest MUSS ihre Methoden aufzählen und scheitern, wenn eine davon Code des
  Chats oder der Modelle erreicht (ADR-0004).
- **FR-042**: Fehlerantworten DÜRFEN nicht verraten, ob ein Eintrag oder Passkey außerhalb des
  Bereichs existiert; „nicht gefunden“ und „außerhalb des Bereichs“ sind gleich (034 FR-029).

**Plattformen**

- **FR-043**: Die Bridge MUSS auf Linux, macOS und Windows verfügbar sein. Auf Android DARF holzi sie
  nicht anbieten; die Einstellungen zeigen „auf dieser Plattform nicht verfügbar“.
- **FR-044**: Alle neuen Texte MÜSSEN auf Deutsch und Englisch vorliegen; Dialoge und Einstellungen
  MÜSSEN ab 360 px Breite ohne waagerechtes Scrollen bedienbar sein (034 FR-041).

**Erkennung der Erweiterung**

- **FR-045**: Der Zulassungsdialog und die Listen der Einstellungen MÜSSEN für jeden Client sagen,
  wie weit holzi die Erweiterung erkennt, aus Herkunft und angegebenem Namen:
  - **bekannt**: Die Herkunft steht in der Liste der offiziellen Erweiterungen (FR-046). Der Text
    nennt den Namen aus der Liste.
  - **nennt sich so**: Die Herkunft ist eine Chromium-Herkunft, die nicht in der Liste steht, aber
    der angegebene Name ist der einer Erweiterung der Liste (ohne Beachtung von Groß- und
    Kleinschreibung und Leerzeichen am Rand). Der Text warnt, dass das keine Bestätigung der
    offiziellen Erweiterung ist.
  - **unbekannt**: Eine Chromium-Herkunft, die nicht in der Liste steht, mit einem anderen Namen. Der
    Text sagt, dass holzi diese Erweiterung nicht kennt.
  - **nicht prüfbar**: Eine Firefox-Herkunft (die UUID ist je Installation zufällig) oder eine
    Herkunft, für die die Liste keinen Eintrag derselben Browserart hat. Der Text sagt, dass holzi
    das hier nicht prüfen kann.

  holzi MUSS den Zustand bei jeder Anzeige berechnen und DARF ihn nicht speichern. Kein Zustand
  ändert, was holzi zulässt: Der Dialog, der Haken „Erlaubnis merken“ (ab Werk nicht gesetzt) und
  die Freigabe bleiben gleich, auch bei „bekannt“.

- **FR-046**: Die Liste der offiziellen Erweiterungen MUSS fest in holzi stehen, je Eintrag Name,
  Browserart und Herkunft; sie wird nicht von außen geladen oder von einem Client verändert. Ein
  Eintrag kommt erst hinein, wenn die Herkunft der veröffentlichten Erweiterung feststeht.
  Solange die Liste für Chromium keinen Eintrag hat, MUSS holzi Chromium-Clients als „nicht
  prüfbar“ zeigen, nicht als „unbekannt“ oder „nennt sich so“: Sonst wäre jede Zulassung der
  echten Erweiterung eine Warnung.

**Instanzen und Besitznachweis**

- **FR-047**: Eine Instanz ist das Schlüsselpaar einer Installation der Erweiterung in einem
  Browserprofil. Eine Zulassung oder Sperre MUSS genau für eine Instanz auf genau einem Gerät
  gelten: Gerät, Kennung, öffentlicher Schlüssel, Herkunft und Erklärung MÜSSEN übereinstimmen,
  sonst fragt holzi neu. Sie DARF NICHT auf einen anderen Browser, ein anderes Profil oder ein
  anderes Gerät übergehen, auch nicht über die Synchronisation der Vault.
- **FR-048**: Bevor holzi über einen Client entscheidet (gesperrt, zugelassen oder Dialog), MUSS der
  Client nachweisen, dass er den privaten Schlüssel zu seiner Kennung besitzt:
  - holzi schickt eine Aufgabe aus 32 zufälligen Bytes, versiegelt an den öffentlichen Schlüssel
    aus dem Handshake, zusammen mit dem Schlüssel der Bridge;
  - der Client öffnet sie mit seinem privaten Schlüssel und schickt sie versiegelt an den
    Schlüssel der Bridge zurück;
  - holzi vergleicht in konstanter Zeit. Die Aufgabe gilt nur für diese Verbindung, nur einmal und
    nur bis zur Frist des Handshakes.

  Ohne passende Antwort MUSS holzi mit `PROOF_FAILED` antworten und schließen, ohne Dialog, ohne
  Anfrage und ohne die Verbindung als die Instanz zu notieren. Nach dem Nachweis braucht ein
  wiederholter Handshake derselben Kennung auf derselben Verbindung keinen neuen.

- **FR-049**: Einen Handshake mit Protokoll kleiner als 3 MUSS holzi mit dem Fehler
  `PROTOCOL_UPDATE_REQUIRED` beantworten, dessen Text sagt, dass die Erweiterung aktualisiert werden
  muss, und schließen, ohne Dialog. Es gibt keine Übergangszeit und keinen Schalter dafür.

### Key Entities

- **Client**: Kennung (Fingerabdruck), öffentlicher Schlüssel, angegebener Name, Herkunft,
  gespeicherte Erklärung.
- **Zulassung**: Client, Freigabe (Art, Bereich), Standard-Tag, gemerkt oder vorläufig, Gerät,
  Zeitpunkt, letzte Verbindung; gemerkt geräteeigen in der Vault, vorläufig nur im Speicher.
- **Sperre**: Client, gemerkt (geräteeigen) oder bis zum Schließen der Vault.
- **Einstellung der Bridge**: je Gerät Ein/Aus und Port.
- **Anwesenheitsbestätigung**: Client, Art der Zeremonie, Gegenstelle, Herkunft, Aufgabe der
  Gegenstelle, gewähltes Konto oder gewählter Eintrag, Frist, Ergebnis; nie gespeichert.
- **Methode**: Name, verlangte Art, Eingabe und Antwort nach `logic/haexPassApi.ts` der Erweiterung.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Mit `haex-pass-browser` in der gepinnten Revision ohne Änderung gelingen in Chromium und
  Firefox in 100 % der End-to-End-Läufe: Verbinden, Zulassen, Ausfüllen einer Anmeldung auf einer
  Testseite, Holen eines TOTP-Codes und Speichern neuer Zugangsdaten. Ein Nutzer schafft das vom
  Einschalten der Bridge bis zur ersten ausgefüllten Anmeldung in unter 2 Minuten.
- **SC-002**: In einer Vault mit 5 000 Einträgen beantwortet holzi `get-items` für eine Adresse mit bis
  zu 20 Treffern grob in unter 500 ms.
- **SC-003**: In 100 % der Fälle einer Zugriffsmatrix (Bereich nach Tags und „alle“, Art,
  Papierkorb, Einträge einer holzi-Funktion, Verweise mit Quelle außerhalb des Bereichs, gemerkte
  Ablehnung) erhält der Client genau, was die Regeln aus 034 und 036 erlauben.
- **SC-004**: In Prüfungen mit eingepflanzten Markierungen kommen TOTP-Secrets, private
  Passkey-Schlüssel, Einträge außerhalb des Bereichs und angefragte Adressen 0-mal in Antworten,
  Fehlermeldungen oder Protokollen vor, wo sie nicht hingehören.
- **SC-005**: Gegen eine Test-Gegenstelle, die UP verlangt, gelingen Registrierung und Anmeldung über
  die Bridge nach Bestätigung in 100 % der Läufe (mit dem Begleit-PR an der Erweiterung);
  ohne Bestätigung, nach Ablehnen, nach Verfallen und mit unpassender oder fehlender Herkunft
  entstehen 0 Passkeys und 0 Signaturen.
- **SC-006**: 100 % der Verbindungen mit einer Webseiten-Herkunft oder ohne Herkunft und 100 % der
  Handshakes mit falscher Kennung, Protokoll v1 oder ohne Erklärung enden ohne Dialog und ohne
  Daten.
- **SC-007**: Nach einem Widerruf beantwortet holzi einem offenen Client keine weitere Anfrage; die
  Nachricht „nicht zugelassen“ kommt in unter 1 Sekunde an.
- **SC-008**: Nach dem Schließen der Vault oder dem Ausschalten der Bridge sind in unter 2 Sekunden
  alle Verbindungen beendet und der Port frei.
- **SC-009**: 100 % der Aufrufe von Lesezeichen-Methoden, unbekannten Methoden und anderen Zielen
  enden ohne Anfrage, ohne Dialog und ohne Änderung der Vault.
- **SC-010**: In einer Prüfung mit je einem Client der vier Zustände aus FR-045 zeigen Dialog und
  Einstellungsliste in 100 % der Fälle den passenden Zustand, und in keinem Fall ändern sich die
  Wahlmöglichkeiten des Dialogs oder das, was holzi dem Client zugesteht.
- **SC-011**: 100 % der Verbindungen ohne Antwort auf die Aufgabe, mit falscher Antwort, mit der
  Antwort aus einer anderen Verbindung oder mit Protokoll 2 enden ohne Dialog, ohne ausgeführte
  Anfrage und ohne Änderung der Vault, auch wenn Kennung, öffentlicher Schlüssel und Herkunft die
  einer zugelassenen Instanz sind.
- **SC-012**: In 100 % der Prüfungen löst eine Zulassung keine andere Instanz aus: weder einen
  anderen Schlüssel mit derselben Herkunft noch denselben Schlüssel mit anderer Herkunft noch
  dieselbe Instanz auf einem anderen Gerät der Vault.

## Assumptions

- Die Erweiterung legt ihr Schlüsselpaar als nicht exportierbare Schlüssel in der IndexedDB ihres
  Browserprofils ab (`keypairStorage.ts`); es wird nicht zwischen Browsern synchronisiert. Wer diesen Speicher liest,
  kann sich als der Client ausgeben; das ist dieselbe Grenze wie in ADR-0007 (ein Prozess, der die
  offene Vault lesen kann, liest ihre Geheimnisse).
- Browser schicken beim Aufbau einer WebSocket-Verbindung aus einer Erweiterung deren Herkunft
  (`chrome-extension://…`, `moz-extension://…`), aus einer Webseite deren Webadresse. Ein lokales
  Programm kann jede Herkunft vortäuschen; FR-005 schützt vor Webseiten, nicht vor Programmen auf dem
  Gerät. Der Plan prüft das Verhalten von Chromium und Firefox (research).
- Die Erkennung nach FR-045 ist eine Hilfe für den Nutzer, kein Zugangsschutz: Ein lokales Programm
  kann die Herkunft der offiziellen Erweiterung vortäuschen und würde dann als „bekannt“ gezeigt.
  Ob die offizielle Erweiterung in Chromium eine feste Herkunft hat, steht im Repository nicht fest:
  Das Manifest hat keinen `key`, die Kennung vergibt der Chrome Web Store beim ersten Upload. Bis
  sie feststeht, bleibt Chromium „nicht prüfbar“ (FR-046, T081).
- Ab Protokoll 3 weist ein Client den Besitz seines privaten Schlüssels nach (FR-048). Es bleibt
  eine Grenze: Die Verbindung ist `ws://` auf Loopback ohne Kanalbindung. Wer den Port belegt, bevor
  holzi lauscht, kann zwischen Erweiterung und holzi vermitteln und die Antwort auf die Aufgabe
  weiterreichen; holzi zeigt dann „Port belegt“ (FR-002), und die Erweiterung redet mit dem, der
  lauscht. Das ist dieselbe Grenze wie bei haex-vault 2.x auf demselben Port (Edge Cases).
- Die Umschläge nehmen das gemeinsame Geheimnis des Schlüsseltauschs (X25519) unmittelbar als
  AES-256-GCM-Schlüssel, ohne Ableitung (wie haex-vault 2.x und die Erweiterung). holzi übernimmt das
  aus Kompatibilitätsgründen; der Plan bewertet es.
- Die Verbindung selbst ist unverschlüsseltes `ws://` auf Loopback; vertraulich sind die Inhalte durch
  die Umschläge.
- Die Erweiterung wartet auf die meisten Antworten 10 Sekunden, auf `create-item`,
  `passkey-create` und `passkey-get` 130 Sekunden (`connection.ts`), auf `passkey-list` 3 Sekunden.
  Die 120 Sekunden für Anfragen nach FR-015 und Anwesenheitsbestätigungen passen hinein.
- Die Erweiterung ruft `get-totp` auf, wenn ein Eintrag `hasTotp` meldet und die Seite ein Feld für
  Einmalcodes hat, und tippt die Marke `otp: "TOTP"` nie ein. holzi liefert die Marke trotzdem wie
  haex-vault 2.x, weil die Erweiterung Einträge verwirft, deren Felder keine Zeichenketten sind.
- Die registrierbare Domain bestimmt holzi mit der Liste öffentlicher Suffixe, die die Herkunftsprüfung
  des Passkey-Dienstes schon nutzt (ADR-0009).
- Ein Eintrag hat genau ein Adressfeld (034 FR-001); weitere Adressen je Eintrag sind nicht
  vorgesehen.

## Änderungen an haex-pass-browser

Ausfüllen, TOTP und Speichern (US1 bis US3, US5, US6) funktionieren mit der gepinnten Revision ohne
Änderung. Stand der Änderungen an der Erweiterung am 2026-10-21 (Revision `ab4e0b0a`):

1. **Herkunft mitschicken (MUSS für Passkeys)**: erledigt (`e0ee77f`). `passkey-create` und
   `passkey-get` tragen `origin`, die der Hintergrund aus Tab und Rahmen des Absenders bestimmt
   (`background/passkeyOrigin.ts`, nur der oberste Rahmen). Ohne sie lehnt holzi ab (FR-032).
2. **Längere Wartezeit (MUSS für Passkeys, SOLL sonst)**: erledigt. `create-item`, `passkey-create`
   und `passkey-get` warten 130 Sekunden.
3. **Antwort von `passkey-list` (MUSS für die Anmeldung)**: erledigt (#113). Der Hintergrund
   antwortet `{ success, passkeys }`, das Inhaltsskript liest `response.data`
   (`background/main.ts`, `contentScripts/webauthn-bridge.ts`); die Liste ist immer leer, und jede
   Anmeldung fällt auf den Browser zurück, ohne holzi zu fragen.
4. **Name der Vault-App**: keine Änderung. Die Texte nennen „HaexVault“, den Produktnamen, den
   holzi übernimmt; haex-vault 2.x (Legacy) ist abgelöst. Eine Umbenennung im Begleit-PR (#113)
   wurde in #114 zurückgenommen.
5. **Lesezeichen (SOLL)**: erledigt (#113). Antwortet der Dienst auf `bookmarks-*` mit „nicht
   unterstützt“, zeigen Einrichtung und Optionen der Erweiterung einen Satz statt des rohen Fehlers.
   Die Wiederholung alle fünf Minuten bleibt; sie läuft nur mit einer aktiven Sammlung, die es mit
   holzi nicht geben kann.
6. **Algorithmen (KANN)**: nicht Teil des Begleit-PR. `passkey-create` meldet weiter fest ES256; holzi
   legt ES256 an (FR-034).

Port, Protokollversion, Schlüsselablage, Umschläge und Methodennamen bleiben, wie sie sind. Nach dem
Begleit-PR wird die Erweiterung neu veröffentlicht (Chrome Web Store, addons.mozilla.org).

Ergänzung 2026-10-23, zweiter Begleit-PR für Protokoll 3 (FR-048, FR-049), veröffentlicht als 1.4.0
**vor** holzi mit PR G:

7. **Besitznachweis (MUSS)**: Der Handshake meldet Protokoll 3. Kommt eine Aufgabe, öffnet die
   Erweiterung sie mit ihrem Schlüssel und schickt sie versiegelt an den Schlüssel der Bridge
   zurück. Kommt ohne Aufgabe gleich `handshakeResponse` (holzi vor PR G), läuft sie weiter wie
   bisher, damit 1.4.0 vor holzi veröffentlicht werden kann.
8. **Eigene Kennung zeigen (SOLL)**: Die Optionen zeigen den Fingerabdruck der Instanz in
   Vierergruppen wie der Dialog in holzi, damit der Nutzer beide vergleichen kann.
9. **Fehlertexte (MUSS)**: `PROTOCOL_UPDATE_REQUIRED` und `PROOF_FAILED` erscheinen als Satz statt
   als roher Code.

## Nicht im Umfang

- Lesezeichen-Synchronisation (`bookmarks-*`); holzi hat keine Lesezeichen (FR-028).
- Weiterleiten von Anfragen an installierte haextensions (in haex-vault 2.x `requestedExtensions` mit
  Aktionen) und andere Clients als Browser-Erweiterungen (Kommandozeile, Server).
- Die External Bridge auf Android und anderen Mobilplattformen.
- Löschen von Einträgen, Ordner, Tags verwalten, Papierkorb, Verlauf, Anhänge und Passkeys umbenennen
  oder löschen über die Bridge (FR-017).
- Eine Schlüsselableitung oder TLS im Protokoll. Den Nachweis des Client-Schlüssels bringt
  Protokoll 3 (FR-048, Clarification 2026-10-23); beides andere bräuchte eine weitere abgestimmte
  Änderung an der Erweiterung.
- Benachrichtigungen an Clients über geänderte Einträge (FR-026).
- Ein Anwesenheitsnachweis mit Biometrie oder Systemanmeldung; UV prüft holzi nur über das Passwort
  der Vault (FR-033).
- Andere Beglaubigungsformate als `none` und ein Zähler ungleich 0 (036, Folgearbeiten).
- Änderungen an der Erweiterung über den Begleit-PR hinaus (Abschnitt oben, Punkte 3 bis 5).
- Ändern von Einträgen (`update-item`), TOTP-Angaben und Symbole beim Anlegen (Clarification
  2026-10-21).
