# Research: Passwortmanager-Redesign

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Stand**: 2026-10-03

Jede Entscheidung: **Entscheidung**, **Begründung**, **verworfene Alternativen**. Belege sind
Dateien im Repository (Stand `main` am 2026-10-03, nach den PRs #228, #229, #234) und der
Quellstand von haex-vault 2.x (Legacy) (`8dce379d94e18fcd42c3b73686a06f984ca3f574`); KeePass steht auf dem
GitHub-Mirror `dlech/KeePass2.x`. Die neuen Crates (`ciborium`, `psl`, `url`) sind mit T003
am 2026-10-04 geprüft (R8, R9). Nicht geprüft: das Verhalten der Wischgeste in der
WebKit-Webview unter Linux und Android; das prüft der manuelle Quickstart (R1).

## R1 — Tab-Wischgeste: Swiper

**Entscheidung**: `swiper` 14 (MIT; eingebunden wird nur `swiper/vue`, ohne das Modul
`Keyboard`, weil die Pfeiltasten der Tab-Leiste von reka-ui kommen, siehe unten) im Wrapper
`EntryTabs.vue`. Die Tab-Leiste bleibt `ShadcnTabs` (reka-ui) mit
`TabsTrigger`; die Wischfläche ist ein `Swiper` mit drei Folien, beide über denselben
Zustand (`activeTab`) gekoppelt: Tippen auf einen Trigger ruft `slideTo`, ein Wisch setzt
`activeTab` aus `activeIndex`. Einstellungen:

- `simulateTouch: false`: nur Finger und Stift wischen, nicht die Maus (sonst kollidiert das
  Ziehen mit dem Markieren von Text am Rechner); am Rechner wechseln Tippen und die
  Pfeiltasten der Tab-Leiste (reka-ui bringt sie mit).
  _Geändert 2026-10-05 (PR #287, FR-002):_ Die Maus zieht doch (`simulateTouch` an,
  `threshold: 8`), und das Modul `Mousewheel` (`forceToAxis`) wechselt bei einer waagerechten
  Touchpad-Geste. Markierbare Werte tragen `data-no-swipe`, damit Markieren weiter geht.
- `noSwiping: true` mit `noSwipingSelector: 'input, textarea, select, [data-no-swipe], .swiper-no-swiping'`
  und `touchStartPreventDefault: false`; Codes, Tabellen und waagerecht scrollbare Flächen
  tragen `data-no-swipe` (FR-003).
- `speed` 0, wenn `prefers-reduced-motion: reduce` gilt (FR-002); gelesen mit
  `useMediaQuery('(prefers-reduced-motion: reduce)')` aus VueUse (im Repo bisher nur über
  Tailwind `motion-reduce:`, die Folienbewegung braucht den Wert in JS).
- `resistance: false`, `watchOverflow`: am ersten und letzten Tab geschieht nichts (FR-002).
- Im Bearbeiten gibt es zwei Folien (Details, Extra); Verlauf ist dort nicht wählbar (FR-008).
  Die Folien werden mit `v-if` auf die Tab-Liste gebaut, nicht per CSS versteckt.

**Begründung**: Die Spec und der Auftrag nennen Swiper; haex-vault 2.x benutzt dieselbe
Bibliothek (14.2.0), die Gesten sind dort erprobt. `noSwiping` löst die Randfälle aus der
Spec (Eingabefelder) ohne eigenen Code.

**Verworfen**: `embla-carousel-vue` (steht schon über das Shadcn-Karussell im Baum, hat
`watchDrag`) — würde eine Abhängigkeit sparen, widerspricht aber der ausdrücklichen Vorgabe
und müsste dieselben Randfälle selbst filtern; wer lieber Embla will, tauscht nur
`EntryTabs.vue`. Eigene Pointer-Events (keine Abhängigkeit, aber Trägheit, Abbruch und
Scrollrichtung selbst bauen). **Offen für den manuellen Test**: Verhalten in der Webview von
Linux (WebKitGTK) und Android; fällt Swiper dort aus, bleiben Tippen und Tasten.

**Kosten**: `swiper` ist groß (3,7 MB entpackt), im Bundle aber nur `swiper/vue` plus
`swiper/modules/*` nach Bedarf; Import des Stylesheets nur der Kernteile
(`swiper/css`). Wird in `EntryTabs.vue` lazy geladen (`defineAsyncComponent`), damit die
Liste ohne Swiper startet.

## R2 — Der Ort kennt den Tab

**Entscheidung**: Die Registry (`src/lib/passwords/registry.ts`) bekommt den Abfrageschlüssel
`tab` mit den Werten `details`, `extra` (kein Geheimnis, deshalb in `isSecretFreeLocation`
erlaubt); `entry/:id/history` bleibt als Ort bestehen und öffnet denselben Eintrag auf dem
Tab Verlauf; `entry/:id` ohne `tab` öffnet Details. `?edit` bleibt, und `tab` gilt dort für
Details und Extra. Wird ein unbekannter Wert gelesen (alter Verlauf, gespeicherte Sitzung),
gilt Details.

**Begründung**: FR-006 verlangt, dass Vor, Zurück und die wiederhergestellte Sitzung den Tab
treffen; die Registry ist die eine Stelle, die Orte auf Geheimnisfreiheit prüft
(`check-passwords-routes.ts`).

**Verworfen**: Den Tab nur im Komponentenzustand halten (ginge beim Zurück verloren); die
Folienposition (Zahl) in den Ort schreiben (bricht, wenn Tabs hinzukommen).

## R3 — Verweise im Backend: ein Auflöser, eine Grammatik

**Entscheidung**: Neues Modul `passwords/references.rs` (rein, ohne Datenbank testbar, mit
einer Funktion für das Auffinden und einer für das Ersetzen) und `passwords/references_db.rs`
(die Abfragen). Grammatik, Auflösung, Kreise und Tiefe stehen in
[contracts/references.md](./contracts/references.md). Die Rust-Seite ist die **einzige**
Implementierung; das Frontend zerlegt Text nicht selbst, sondern fragt sie (R11).
Hook-Punkte (vom Rust-Bericht belegt):

- `reveal.rs`: `reveal`, `copy_value`, `secret_item` lösen auf, bevor sie ausliefern;
  `totp_code` nicht (das TOTP-Secret ist kein Verweisfeld, FR-044).
- Listen: `items::load_headers` bleibt roh (die Oberfläche zeigt die Marke);
  `headers_in_scope` (Aufrufer von außen) leert ein Feld, dessen Wert einen Platzhalter
  enthält (`references::find`, FR-047). Die Kennzahl „hat Passwort“ zählt einen Verweis als Passwort.
- Verlauf: `snapshots.rs` kopiert rohe Spalten, FR-049 gilt ohne Änderung.
- Schreiben: `create_item` und `update_item` rufen die Kreisprüfung (R4).

**Begründung**: Alle Geheimnisse verlassen den Dienst über `reveal.rs` und `service/items.rs`;
dort ist die Auflösung genau einmal nötig. Die Grammatik darf nicht in zwei Sprachen leben
(Memory „one implementation per rule“).

**Verworfen**: Auflösen in SQL (Rekursion über Tabellen mit Berechtigungsprüfung ist nicht
ausdrückbar); Auflösen im Frontend (Geheimnisse blieben nicht im Backend, FR-028).

## R4 — Tiefe, Kreise, Ketten

**Entscheidung**: Die Auflösung verfolgt Ketten bis zu **12 Stufen** (`MAX_REFERENCE_DEPTH`).
Einen Kreis erkennt sie an der aktuellen Kette der (Eintrag, Feld) vom gelesenen Feld bis zur
Stufe (ein Stapel, keine Menge über den ganzen Aufruf, sonst wäre derselbe Verweis zweimal in
einem Wert ein falscher Kreis); sie liefert dann `ReferenceError::Cycle`, bei Stufe 13
`ReferenceError::TooDeep`, bei fehlender Quelle und bei einer Quelle außerhalb des Bereichs
`Missing` (eine eigene Art gibt es dafür nicht, damit beides von außen gleich aussieht,
FR-047). **Nie** leerer Text. Beim Speichern prüft `references::validate`, dass die neuen
Platzhalter keinen Weg zurück zum selben Feld des gespeicherten Eintrags eröffnen. Diese
Kreisprüfung durchsucht den erreichbaren Referenzgraphen ohne das Auflösungslimit von 12
Stufen; sonst könnte ein Kreis erst auf Stufe 13 durchrutschen. Ein Verweis auf ein anderes
Feld desselben Eintrags ist kein Kreis.

KeePass (`KeePass/Util/Spr/SprEngine.cs`, GitHub-Mirror `dlech/KeePass2.x`) geht so vor:
`MaxRecursionDepth = 12`, jede verschachtelte Auflösung ruft `CompileInternal` mit
`uRecursionLevel + 1`; ab Ebene 12 liefert es **leeren Text** („Most likely a recursive
reference“), ohne Kreise zu erkennen. Das übernehmen wir bei der Tiefe, nicht beim stillen
Leeren.

**Begründung**: Ein stilles leeres Passwort würde unbemerkt benutzt; ein klarer Fehler
verhindert das (FR-046). Die Kreisprüfung beim Speichern ist billig, weil jeder Wert höchstens
einige Platzhalter trägt.

**Verworfen**: Nur eine Stufe (einfacher, aber Ketten wie „Zweitkonto verweist aufs Hauptkonto,
das auf den Administrator“ gehen dann nicht); Kreise nur über die Tiefe erkennen (ergibt nach
zwölf Runden dieselbe Meldung, aber ohne die Warnung beim Speichern).

## R5 — Passkey-Zähler: synchronisiert immer 0

**Entscheidung**: Der Dienst sendet bei Anlegen und Bestätigen immer `signCount = 0` und
erhöht `passkeys.sign_count` nicht. Ein CRDT-Zähler pro Gerät wäre nicht ausreichend: Zwei
offline bestätigende Geräte könnten denselben Nicht-Null-Wert senden; eine Gegenstelle kann
das nicht von einem Clone, einer Fehlfunktion oder einer Race Condition unterscheiden.

Das entspricht der WebAuthn-Variante für Authenticatoren ohne Signaturzähler. Der Dienst
verzichtet damit bewusst auf Clone-Signale, statt eine unzuverlässige globale Ordnung zu
behaupten. Die Passkey-Tabelle braucht deshalb keine zusätzliche Zählertabelle.

## R6 — Passkey per Verbindung (FR-046)

**Entscheidung**: Neue CRDT-Tabelle `haex_passwords_passkey_links` (Kennung = UUIDv5 aus
Ziel-Eintrag und Passkey, Spalten `item_id` (Ziel), `passkey_id`). Der Kopier-Dialog legt je
Passkey der Vorlage eine Verbindung an. Ein Ziel mit Verbindung zeigt den Passkey mit seinen
Kopfdaten (mit `linked_from` = Quelle); Bestätigen über das Ziel prüft die Zugriffsrechte für
**beide** Einträge (Ziel und Quelle, FR-047) und signiert mit dem Schlüssel der Quelle; der
Zähler gehört dem Passkey, nicht der Verbindung. Löschen des Passkeys oder der Quelle löscht
die Verbindungen (zuerst, wie überall in `trash::purge_item`); „Verweis lösen“ am Ziel löscht
nur die Verbindung.

**Begründung**: Ein Passkey ist kein Text; eine zweite Tabelle mit abgeleiteter Kennung folgt
dem Muster aus 034 (keine UNIQUE-Constraints, UUIDv5, Kinder zuerst löschen) und braucht keine
neue Spalte an einer vorhandenen Tabelle.

**Verworfen**: Eine Spalte `linked_item_id` an `passkeys` (ein Passkey hätte nur ein Ziel);
den Schlüssel kopieren (ein geklonter Authenticator, FR-046).

## R7 — Kopieren und der Kopier-Dialog

**Entscheidung**: Neues Modul `passwords/copy.rs` (Tiefenkopie in **einer** Schreibtransaktion
`db.write`) und `service/copy.rs` (nur Nutzer). Es legt Einträge mit `create_item` neu an,
hängt Anhänge mit `link_attachment` an (die Binärdaten bleiben, dieselbe Prüfsumme),
kopiert bei „Verlauf übernehmen“ die Zeilen `item_snapshots` und `snapshot_binaries` mit neuen
Kennungen, legt Ordner mit `groups::create_group` an und geht den Teilbaum mit einer
rekursiven CTE wie `groups.rs` und `trash.rs`. Optionen: `CopyOptions { title: Exact | Suffix,
history, username_as_reference, password_as_reference, passkeys_as_links }`. Mit einem
Verweis ist der Wert der Kopie der Platzhalter auf den Wert der Vorlage (ist dieser leer,
bleibt er leer; trägt er selbst einen Verweis, zeigt der neue auf die Vorlage, die Tiefe
trägt das). Den Standardzusatz („Kopie“ oder „Copy“) übergibt das Frontend (kein
lokalisierter Text im Backend). Der Dialog ist `CopyDialog.vue` (Dialog am Rechner,
`UiDrawerModal` auf schmalen Fenstern).

**Begründung**: Nur eine Transaktion macht „nichts halb ausgeführt“ (FR-022) möglich;
`items.rs` (687), `groups.rs` (299) und `model.rs` (662) sind zu groß für Erweiterungen.

**Verworfen**: Das Frontend ruft `create_item` je Eintrag (hunderte Aufrufe, nicht atomar,
Geheimnisse müssten durchs Fenster).

## R8 — Passkey-Funktionen im Dienst

**Entscheidung**: Drei Methoden im `PasswordsService` (`service/passkeys.rs` wächst, die
Logik liegt in `passkeys_ops.rs` und dem reinen `webauthn.rs`): `passkey_create`,
`passkey_confirm`, `passkey_list`; Vertrag in [contracts/passkey-service.md](./contracts/passkey-service.md).
Es gibt dafür **keine** Tauri-Commands (die Oberfläche legt nichts an und benutzt nichts,
FR-023); sie werden über Rust-Integrationstests und später durch Spec 017–019/021 und die
External Bridge aufgerufen. Algorithmen: Anlegen mit ES256 (`p256`, Vorrang) oder EdDSA
(`ed25519-dalek`), je nach `pubKeyCredParams` des Aufrufers; Bestätigen für beide; **RS256
bestätigt holzi nicht** (importierte RS256-Passkeys erscheinen, `passkey_confirm` meldet
`UnsupportedAlgorithm`, FR-034): dafür wäre `rsa` als Laufzeit-Abhängigkeit nötig, das
heute nur `0.10.0-rc` in den Dev-Abhängigkeiten ist. Beglaubigung `none` (`fmt: "none"`,
leerer `attStmt`). `clientDataJSON` baut der Dienst selbst aus Typ, Aufgabe und Herkunft
(deterministisch) und gibt es zurück; so kann der Aufrufer Herkunft und Aufgabe nicht
auseinanderfallen lassen. Flags der Authenticator-Daten: BE (0x08), BS (0x10), beim Anlegen
zusätzlich AT (0x40); UP wird ohne einen erfolgreichen, zeremoniegebundenen Presence-Nachweis
nicht gesetzt. UV (0x04) steht nie, weil kein Mensch die Anfrage bestätigt. Die aktuellen
Antworten sind damit für Gegenstellen, die UP oder `userVerification: "required"` verlangen,
bewusst nicht verwendbar. Ein vertrauenswürdiger Presence-Provider ist Voraussetzung, bevor
UP gesetzt werden darf.
Neue Abhängigkeiten: `ciborium` (CBOR für COSE-Schlüssel und Beglaubigung, nicht im
`Cargo.lock`), `psl` (öffentliche Suffixe, R9) und `url` (steht transitiv im Lock; jetzt
direkt).

**Geprüft (T003, 2026-10-04)**: `ciborium 0.2.2`, `psl 2.1.239` und `url 2.5.8` lösen mit
dem Lock auf und bauen in beiden Konfigurationen (Standard und `--no-default-features`). Ein
Wegwerf-Test kodiert eine COSE-EC2-Schlüsselkarte mit `ciborium`, signiert und prüft ES256 als
DER mit `p256` 0.14 (Feature `ecdsa` über die Standard-Features) und EdDSA mit
`ed25519-dalek` 3, und `psl::suffix_str("example.co.uk")` ist `co.uk`. In `p256` 0.14 heißt
`to_encoded_point` jetzt `to_sec1_point`. Die Abhängigkeiten kommen erst mit Stufe 4 in die
`Cargo.toml`, wo sie gebraucht werden.

**Begründung**: Der Rust-Bericht: `derive_public_key` kennt ES256, EdDSA und RS256 (hand-gebaut
für RSA), aber es gibt weder Signieren noch CBOR; `p256` hat die Funktion `ecdsa` über die
Standard-Features.

**Verworfen**: `coset` (typisierter COSE, zieht `ciborium` ohnehin, bringt für drei
Schlüsselarten keinen Gewinn); CBOR von Hand (zu fehleranfällig für Beglaubigungsdaten);
RS256 jetzt (Release-Kandidat-Crate in Produktion, und holzi legt keine RSA-Passkeys an).

## R9 — Herkunftsprüfung (FR-025)

**Entscheidung**: Reine Funktion `origin_matches(origin, rp_id)` in `webauthn.rs`: `origin`
parsen (`url::Url`), Schema `https` (oder `http` bei `localhost`), Host nach IDNA in
Kleinbuchstaben; `rp_id` ebenso; eine IP-Adresse als Host oder `rp_id` wird abgelehnt (eine
RP-ID ist nach WebAuthn ein Domänenname); erlaubt, wenn Host == `rp_id` oder Host endet auf
`.` + `rp_id`, **und** `rp_id` kein öffentliches Suffix ist (`psl::suffix(rp_id)` ist der
ganze String; `localhost` ausgenommen). Das schließt `rp_id = "com"` und `"co.uk"` aus, auch
wenn die Herkunft darauf endet.

**Begründung**: Eine bloße „endet auf“-Prüfung lässt einen Aufrufer die Kennung `com` melden
und einen Passkey für jede `.com`-Seite signieren; die öffentliche Suffixliste ist die
Standard-Abwehr (der Browser macht dasselbe). haex-vault 2.x nimmt `https://<Kennung>` fest an
und prüft nichts.

**Verworfen**: Ohne Suffixliste, nur „mindestens zwei Labels“ (lässt `co.uk` durch).
Die `psl`-Daten sind in die Crate kompiliert (Version 2.1.239 vom Stand der Prüfung, T003);
die Liste wird mit einer Cargo-Aktualisierung erneuert.

## R10 — Ablage, Auswahl, Menüs, Kürzel

**Entscheidung**:

- **Ablage**: Pinia-Store `stores/passwordsClipboard.ts` (Kennungen und Art `cut`/`copy`, im
  Speicher, nicht in Sitzung, Sync oder Betriebssystem-Zwischenablage). Alle Fenster teilen
  heute den Webview und damit den Store (FR-021); native Fenster gibt es noch nicht (020 bereitet
  sie nur vor). Kommen sie, braucht die Ablage einen geteilten Zustand außerhalb des Webviews; `PasswordsApp.vue` zählt seine Instanzen und
  leert die Ablage, wenn die letzte geschlossen wird; ein Wechsel der Vault leert sie über
  den vorhandenen Abmelde-Pfad (`passwords.reset`).
- **Menüs**: ein reiner Baustein `lib/passwords/menus.ts` liefert aus (Art, Papierkorb, Ablage
  gefüllt, Auswahlgröße) die Liste der Einträge; `ShadcnContextMenu` (Rechtsklick) und ein
  `ShadcnDropdownMenu` an der Zeile (Menüknopf, wo es keine rechte Maustaste gibt) zeigen
  dieselbe Liste; damit sind Kontextmenü und Auswahlleiste dieselben Aktionen (FR-019).
- **Kürzel**: reiner Auflöser `lib/passwords/shortcuts.ts` (Ereignis → Befehl, plattformabhängig
  Strg oder Cmd); gehört an `@keydown` des Wurzelelements von `PasswordsApp.vue`, so wirkt er
  nur mit Fokus im Fenster; er enthält die Schutzregeln (Eingabefeld, Dialog, markierter Text)
  und meidet Alt+Pfeil und Meta+Klammer (gehören dem Window Manager, der sie in der
  Aufnahmephase abfängt, `useWmKeyboard.ts`). Pfeiltasten bewegen den Fokus in der Liste
  (`roving tabindex` in `List.vue`).
- **Brotkrumen**: `lib/passwords/breadcrumb.ts` (Pfad aus `tree.ts`, Kürzen auf höchstens
  drei sichtbare Teile bei Schmalheit, der Rest in einem Überlaufmenü); `ShadcnBreadcrumb`;
  jeder Teil ist ein Ablageziel (`dnd.ts`).

**Begründung**: Die Bausteine gibt es (Layer `haex-ui`); die reinen Teile sind mit Node
prüfbar wie `selection.ts`.

**Verworfen**: Eine Tastaturbibliothek (`tinykeys`; zu wenig Bedarf, zusätzliche Abhängigkeit);
den Auswahlzustand in den Ort zu schreiben (Auswahl endet beim Ordnerwechsel, FR-011).

## R11 — Verweise in der Oberfläche

**Entscheidung**: Das Frontend zerlegt Platzhalter **nicht** selbst. Es nutzt drei Commands
(Verträge in [contracts/tauri-commands.md](./contracts/tauri-commands.md)):
`passwords_references_parse(text)` (liefert Teile mit Art, Quelle, Titel der Quelle und
Zustand), `passwords_reference_token(item_id, kind, key?)` (baut den Platzhalter samt
Schutzzeichen) und `passwords_item_key_names(item_id)` (für den Wertwähler). In der
Ansicht stehen Felder mit Verweis als `ReferenceValue.vue` (Marken statt Text, Anzeigen wie
`MaskedValue`: gedrückt halten oder umschalten, der aufgelöste Wert kommt über `reveal`
und bleibt im Bauteil). Im Editor bleibt das Eingabefeld Text; darunter stehen die Marken
des Felds (aus `passwords_references_parse`, entprellt mit 200 ms) mit „entfernen“, daneben
der Knopf „Verweis einfügen“ (`ReferencePicker.vue`: Eintrag suchen in den Kopfdaten des
Stores, dann Wert wählen). Ein vorhandenes Passwort (Modus `keep`) trägt seine Marken aus
`ItemDetail.password_references`, ohne den Wert zu laden.
_Geändert 2026-10-05 (PR #287):_ „Verweis einfügen“ ist ein Link-Symbol im Feld selbst
(Append-Slot, `ReferenceInsert.vue`), neben dem Kopierknopf; `ReferenceField.vue` zeigt nur
noch die Marken unter dem Feld. Den Modus `keep` gibt es nicht mehr: Passwort und eigene
Felder stehen mit ihren Platzhaltern im Feld (034 FR-005), die Marken kommen aus dem Text. Die Suche (`search.ts`) faltet
Platzhalter weg und findet sie nicht als Text.

**Begründung**: Eine Grammatik in einer Sprache (R3); Geheimnisse bleiben im Backend (FR-028).
Ein Rich-Text-Eingabefeld mit Marken im Text wäre groß und fehleranfällig.

**Verworfen**: `contenteditable` mit Marken im Text; eine TS-Kopie der Grammatik.

**ponytail**: Marken stehen unter dem Feld, nicht im Text; ein späteres Rich-Feld ändert nur
`ReferenceValue.vue` und den Editor, nicht die Grammatik.

## R12 — Löschen einer Quelle (FR-048)

**Entscheidung**: Vor dem endgültigen Löschen fragt das Frontend
`passwords_reference_usage(item_ids)` (Zahl der Ziele je Quelle, getrennt in Textverweise und
Passkey-Verbindungen; reine Abfrage `LIKE '%{$<id>:%'` über die vier Textspalten der Details
(`username`, `password`, `url`, `note`) und die Wertspalte der eigenen Felder, plus
`passkey_links`); bei Treffern zeigt der Dialog die Zahl
und die Wahl „Verweise in eigene Werte umwandeln“. `passwords_delete_permanently` bekommt den
Parameter `inline_references: bool`: dann ersetzt die Transaktion zuerst jeden Platzhalter
durch den heutigen Wert (aufgelöst mit den Rechten des Nutzers), nimmt je Ziel einen neuen
Stand und löscht danach; Passkey-Verbindungen fallen weg. Die Warnung „in Benutzung“ aus 034 (`UsageRegistry`, Randfall in 034) bleibt unberührt.

**Begründung**: Alles in einer Transaktion: wird gelöscht, sind die Werte vorher eingesetzt.
Die Abfrage ist ein Tabellenscan über wenige Spalten, bei 5.000 Einträgen schnell.

**Verworfen**: Ein Verweis-Index (Tabelle der Verweise): schneller, aber ein zweiter
Datenbestand, der mit dem Text auseinanderlaufen kann.

## R13 — KeePass-Verweise beim Import (FR-050)

**Entscheidung**: `ImportItem` bekommt `source_ref: Option<String>` (die KeePass-Kennung aus
`entry.id()` in `keepass.rs walk()`); `apply.rs` führt eine Zuordnung Quellkennung → neue
Kennung. Nach dem Schreiben aller Einträge (und vor den Anhängen) läuft ein zweiter Durchgang:
die reine Funktion `import/references.rs::convert(model, id_map)` liefert je Eintrag die
neuen Texte; sie versteht `{REF:<Feld>@<Suche>:<Text>}` mit Feld `U` (Benutzername) oder `P`
(Passwort) und Suche `I` (Kennung, 32 Hex-Zeichen) oder `T`, `U`, `P`, `A`, `N`, `O`
(Treffer muss **genau einer** sein, gesucht in den Quellwerten der Datei); alles andere bleibt
Text. Geschrieben wird je betroffenem Eintrag einmal (neuer Anfangsstand). `ImportReport`
bekommt die Zähler `references_converted` und `references_left_as_text`. Ein Quelleintrag,
der als Duplikat übersprungen wurde, hat keine neue Kennung; sein Verweis bleibt Text und wird
mitgezählt.

**Begründung**: Der Rust-Bericht: Die Kennungen der Einträge werden heute nicht gehalten;
`apply.rs` vergibt `Uuid::new_v4()` je Eintrag. Nur nach dem Schreiben sind die echten
Kennungen bekannt (Duplikate!). `apply.rs` ist mit 677 Zeilen schon zu groß, deshalb eine
eigene Datei.

**Verworfen**: Kennungen vor dem Schreiben festlegen (geht bei übersprungenen Duplikaten ins
Leere).

**Nachtrag (Umsetzung, 2026-10-04)**: Die übersprungenen Duplikate stehen schon vor dem
Schreiben fest (`write_all` berechnet sie zuerst). Deshalb vergibt `import/apply_references.rs`
die Kennungen doch vorab, nur für Einträge, die geschrieben werden, und wandelt die Verweise vor
dem ersten Schreiben um. Ein zweiter Durchgang entfällt, und der Anfangsstand im Verlauf trägt
schon die Platzhalter.

## R14 — Anhangskarten und Lightbox

**Entscheidung**: Karten (`AttachmentCard.vue`) im Raster aus `Attachments.vue`
(Container-Query, 3 → 2 → 1 Spalten). Vorschaubilder erzeugt das Frontend: je Karte im
sichtbaren Bereich (`useIntersectionObserver` aus VueUse, im Repo bisher unbenutzt) holt es
die Bytes über `passwords_attachment_preview`, liest die Pixelgröße aus dem Bildkopf (über
100 Megapixel keine Vorschau, auch nicht in der Lightbox, sonst bringt ein kleines Bild mit
riesiger Fläche die Webview zum Absturz), verkleinert mit `createImageBitmap` und `canvas` auf
320 px (doppelte Kartenbreite für scharfe Bilder auf dichten Bildschirmen), hält das Ergebnis
als WebP-Blob-URL in einem LRU-Zwischenspeicher (200 Stück, nach Prüfsumme) und gibt die vollen
Bytes sofort frei; höchstens zwei gleichzeitig. Ein Bild, das nicht dekodiert, bleibt als
Fehler gemerkt; konnten nur die Bytes nicht gelesen werden, versucht die nächste Anfrage es neu. Die Lightbox ist `photoswipe` 5.4 (MIT, auch in haex-vault 2.x),
zur Laufzeit geladen (`import()`), mit den Bildern des Eintrags in Kartenreihenfolge;
volle Größe wird erst beim Öffnen einer Folie geholt (`passwords_attachment_preview`).
Anhänge ohne Vorschau (PDF, Text, anderes, auch SVG) bieten „Speichern unter“
(`passwords_attachment_save`, vorhanden).

**Begründung**: PhotoSwipe liefert Zoom, Zwicken, Mausrad, Wischen, Pfeiltasten und
Escape (FR-038), erprobt in haex-vault 2.x. Eine Rust-Verkleinerung bräuchte das `image`-Crate
direkt (es steht transitiv im Lock, mit unbekannten Features); das Frontend kann
JPEG, PNG, GIF und WebP ohnehin dekodieren.

**Verworfen**: Verkleinern in Rust (`image` direkt, mehr Binärgröße); eigene Lightbox (Zoom und
Gesten nachbauen); alle Bilder voll laden (SC-008).

**Bewusste Grenze**: Der Webview lädt für ein Vorschaubild kurz die vollen Bytes (bis 25 MiB);
höchstens zwei gleichzeitig begrenzen den Spitzenverbrauch.

## R15 — Verlauf als Tab

**Entscheidung**: `HistoryView.vue` (269 Zeilen) wird in `HistoryTimeline.vue` (Zeitleiste) und
`HistorySnapshot.vue` (Standansicht) geteilt; beide stecken in der dritten Folie von
`EntryTabs.vue`. Die Seite `entry/:id/history` bleibt als Ort bestehen (R2), ihr Inhalt ist
der Tab; Wiederherstellen, Aufdecken und die Änderungsangabe kommen unverändert aus
`usePasswords.ts` (`history_list`, `history_get`, `history_reveal`, `history_restore`).
Relative Zeit mit `Intl.RelativeTimeFormat` (Sprache der Oberfläche); der absolute Zeitpunkt
steht im `title`.

**Begründung**: Nichts am Verlaufsformat ändert sich (034 R3, R5).

## R16 — Aufteilen von `EntryEditor.vue` und `EntryView.vue`

**Entscheidung**: `EntryEditor.vue` steht mit 645 Zeilen über der 500-Zeilen-Grenze und wird
beim Umbau zerlegt: `EntryTabs.vue` (Tab-Leiste + Swiper), `EditorDetails.vue`,
`EditorExtra.vue`, `ViewDetails.vue`, `ViewExtra.vue`; `EntryEditor.vue` und `EntryView.vue`
bleiben die Hüllen (Kopf, Speichern, Konflikt, Entwurf) und werden kleiner als heute.
`draft.ts` bleibt, es gibt keine neuen Entwurfsfelder (Verweise sind Text).

## R17 — Tests und End-to-End

**Entscheidung**: Rust-Einheitstests in `*_tests.rs` (`references_tests` mit Test-Vektoren aus
der Grammatik-Datei, `webauthn_tests` mit festen Schlüsseln und der Prüfung der Signatur mit
dem öffentlichen Schlüssel, `copy_tests`, `passkeys_ops_tests`, `import/references_tests`),
Integration in `src-tauri/tests/` (`passwords_passkeys.rs`, `passwords_references.rs`,
`passwords_copy.rs`); Frontend-Prüfskripte
`check-passwords-menus.ts`, `-shortcuts.ts`, `-breadcrumb.ts`, `-clipboard.ts`,
`-tabs.ts` (Registry); e2e `passwords-organize` (Auswahl, Ausschneiden, Kopieren mit Dialog,
Brotkrumen, Kontextmenü, Kürzel), `passwords-tabs` (Tabs per Tippen und Pfeiltaste, Verlauf,
Wiederherstellen), `passwords-references` (Verweise anlegen, ändern, Quelle löschen),
`passwords-passkeys` (Passkeys im Tab Extra, Kopie per Verbindung, „Verweis lösen“),
`passwords-attachments` (Karten und Lightbox per Tastatur); `passwords-sync-two-devices` bleibt
unverändert. Wischgeste und Lightbox-Gesten sind **nur manuell** (Quickstart): der e2e-Rahmen
(tauri-driver, WebKitWebDriver) hat keine verlässlichen Berührungsgesten, und die
Dateidialoge für Anhänge sind nativ (wie in 034 SC-012, dort schon ausgenommen).

## R18 — ADR

**Entscheidung**: ADR-0009 „Verweise zwischen Einträgen werden im Dienst aufgelöst und nie
aufgelöst gespeichert“ (Grammatik, 12 Stufen, kein stilles Leeren, Bereichsprüfung an der
Quelle, Listen ohne Auflösung) und der Passkey-Dienst (Herkunftsprüfung, synchronisierter
Zähler immer 0, kein UP/UV ohne Presence-Nachweis).
Sie berühren die Aussage aus ADR-0007 („der Schutz ist die Berechtigung“) und erweitern sie.
