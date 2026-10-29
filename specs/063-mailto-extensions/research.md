# Research: Extensions als Öffner für E-Mail-Links

Geprüft am 2026-10-28 gegen `origin/main` (c8a79ea6) und die Revision `280cebf63eba72422d1ddd62e1edeb5ad7e7d3cc`
von `vault-sdk`.

## Vorhandene Bausteine

- **Mail-Knopf**: `openTargets('email', …)` in `src/lib/contacts/kinds.ts` liefert `mailto:<adresse>`;
  `ValueActions.vue` öffnet es mit `openUrl` (Plugin `opener`). Dort gibt es keine Auswahl.
- **Manifest**: `haex-bundle` (Repo `vault-sdk`) prüft das Bündel; `extensions/bundle/manifest.rs` liest die
  Felder, die holzi braucht. Ein optionales Feld falschen Typs zählt dort als abwesend, nie als Fehler. Der
  Block `tools` (Spec 018) ist das Vorbild: Prüfung in `crates/haex-bundle/src/tools.rs`, Vertrag in
  `specs/018-extension-agent-tools/contracts/manifest-tools.md`.
- **Bridge**: Methoden stehen in `extensions/bridge/allowlist.rs` als `extension_<bereich>_<verb>`; Ereignisse
  an Frames gehen über `emit_to_frames` (`extension-frame-event`). Ohne offenen Frame gibt es keinen
  Empfänger, ein Ereignis allein geht dann verloren.
- **Fenster der Extension**: `windowManager.openExtension(extensionId)` öffnet oder holt es nach vorn
  (`singleInstance` aus dem Manifest).
- **Deep Link** (Spec 060): Plugin, Posteingang zwischen Prozessen, `useDeepLink`, Speicher `shareInbox`.
  `deep_link::contact_link` lässt nur `haex-vault://contact/…` durch.
- **haex-mail** (`haex-space/haextension@b05d0e039f00122bb4abb7079149fcdb1d663e39`): hat
  `apps/haex-mail/app/components/ComposeDialog.vue` und das Muster `app/plugins/agent-tools.client.ts`
  (`client.tools.register`), das beim Start des Frames etwas beim Client anmeldet.

## R1 Anmeldung im Manifest

- **Entscheidung**: neuer optionaler Block `openers`, ein Array von Zeichenketten, jede eine Art von Link:
  `"openers": ["mail.compose"]`. Form der Art: `^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$`, höchstens 32 Zeichen;
  höchstens 8 Einträge; doppelte Einträge sind ein Fehler. Eine Art, die holzi nicht kennt, ist gültig und
  wird nur nicht angeboten (FR-013). Die Prüfung steht in `haex-bundle` (`openers.rs`, neben `tools.rs`),
  holzi liest die Zeichenketten in `Manifest.openers` und kennt davon nur `mail.compose`.
- **Warum Zeichenketten**: Mehr als die Art braucht eine Anmeldung heute nicht; ein späterer Zusatz
  (zum Beispiel erlaubte MIME-Typen) darf aus einer Zeichenkette ein Objekt machen, solange `openers` der
  Name des Blocks bleibt, und kostet nichts, bevor er gebraucht wird.
- **Verworfen**: ein Eintrag unter `permissions` (die Anmeldung ist keine Berechtigung und gibt keinen
  Zugriff, FR-010); ein eigener Tool-Eintrag in `tools` (Tools sind für den Agenten, nicht für den Nutzer).
- **Unabhängigkeit der Repos**: holzi liest `openers` auch, bevor `vault-sdk` es prüft, denn das Format
  schränkt unbekannte Felder nicht ein und signiert sie. Die Prüfung im `vault-sdk` verschärft nur, es ist
  keine Voraussetzung für holzi (Reihenfolge in R8).

## R2 Den Link lesen

- **Entscheidung**: ein Parser in Rust (`openers/mail.rs`), der einzige Ort für Form, Grenzen und
  Ablehnung. Die Oberfläche reicht den Link nur durch und zeigt, was Rust zurückgibt.
  1. Schema `mailto:` ohne Rücksicht auf Groß-/Kleinschreibung; der Rest bis `?` sind Empfänger,
     getrennt durch `,`, einzeln prozent-dekodiert.
  2. Nach `?`: Paare `name=wert`, getrennt durch `&`. Bekannt sind `to`, `cc`, `bcc` (hängen an die
     Empfänger), `subject` und `body` (der erste gilt). Alle anderen Namen, auch `in-reply-to` oder
     `x-…`, werden ignoriert, nie als Kopfzeile weitergegeben (FR-007).
  3. Prozent-Dekodierung als UTF-8; ein ungültiges UTF-8 oder ein kaputtes `%xx` lehnt den Link ab.
  4. In Empfängern, Kopie, Blindkopie und Betreff lehnt ein `\r` oder `\n` (nach dem Dekodieren) den
     Link ab; im Text bleiben Zeilenumbrüche, `\r\n` wird zu `\n`.
  5. Empfänger: nicht leer, höchstens 254 Zeichen, genau ein `@` mit nicht leerem Teil davor und danach,
     keine Leerzeichen oder Steuerzeichen. Mehr prüft holzi nicht; die Extension prüft ihre Adressen
     selbst. Ein Link ohne Empfänger ist gültig (`mailto:?subject=…`).
  6. Grenzen (FR-008): höchstens 50 Empfänger je Feld, Betreff höchstens 998 Zeichen, Text höchstens
     16 KiB (Bytes), der ganze Link höchstens 32 KiB. Darüber lehnt holzi ab, es kürzt nicht.
- **Verworfen**: Lesen im Fenster (der Ort wäre dann zweimal da, und Tests liefen nur im Browser);
  die Kiste `url` (`mailto:` hat keinen Pfad im Sinn von `Url`, und sie normalisiert Zeichen, die hier
  genau erhalten bleiben sollen). `percent-encoding` ist schon eine Abhängigkeit.

## R3 Übergabe an die Extension

- **Entscheidung**: Anfrage parken, Extension zieht sie ab.
  1. `opener_mail_open(extensionId, link)` liest den Link neu (R2), prüft, dass die Extension installiert,
     aktiviert und für `mail.compose` angemeldet ist, und legt die Anfrage in `OpenRequests` ab: höchstens
     eine je Extension, eine neue ersetzt die alte; nur im Speicher.
  2. Rust sendet `opener-request` (ohne Inhalt) an die offenen Frames der Extension; das Fenster ruft
     `openExtension(extensionId)`, wodurch es entsteht oder nach vorn kommt.
  3. Die Extension ruft `extension_opener_take` auf, wenn ihr Frame startet und wenn `opener-request`
     eintrifft; die Methode gibt die Anfrage zurück und löscht sie. Ohne Anfrage gibt sie `null` zurück.
  4. Holt niemand sie in 15 s ab, verfällt sie; Rust sendet `opener-request-expired` an das Fenster, das
     sagt, dass die Extension nicht geantwortet hat (FR-009). Die anderen Wahlen bleiben offen.
- **Warum Abholen**: Ein Ereignis allein geht verloren, wenn die Extension nicht läuft (Kaltstart); mit
  dem Abholen beim Start gilt derselbe Weg für kalt und warm. Die Extension fragt nur nach der Anfrage,
  die der Nutzer ihr gegeben hat; `take` gibt nie etwas anderes.
- **Verworfen**: den Link in die Adresse der Extension schreiben (Frames werden von holzi geladen, die
  Bridge ist der erlaubte Weg, und 16 KiB in einer Adresse sind fehleranfällig); Push allein; eine
  dauerhafte Warteschlange auf der Platte (FR-017 verbietet es für Links von außen, und der Mail-Knopf
  braucht sie nicht).

## R4 Mail-Knopf

- **Entscheidung**: `ExtensionSummary` bekommt `openers: string[]` (nur die bekannten Arten, also
  `["mail.compose"]` oder leer); der Speicher der Extensions hält die Liste ohnehin aktuell
  (`extension-status-changed`), also braucht die Auswahl keinen eigenen Abruf und folgt Aktivieren,
  Deaktivieren und Entfernen sofort (SC-006). `useMailOpeners()` filtert die aktivierten mit
  `mail.compose`. In `ValueActions.vue` ersetzt `MailOpenMenu.vue` den Mail-Knopf nur, wenn die Liste nicht
  leer ist, sonst bleibt `openUrl` wie heute (FR-001, SC-002, FR-023).
- Menü: shadcn-Dropdown wie das „+“-Menü der Kontakte: „Mit dem System öffnen“, dann je Öffner eine Zeile mit
  dem Namen der Extension (Anzeigename aus `i18n` des Manifests, sonst Name), Tastatur und Beschriftung durch
  das Bauteil.
- **Verworfen**: ein eigener Befehl `openers_list` (zweite Wahrheit neben dem Speicher).

## R5 Registrierung beim System

- **Entscheidung**: `mailto` kommt als zweites Schema in `plugins.deep-link` (`desktop.schemes` und eine
  Zeile in `mobile`), `capabilities/default.json` ist schon richtig. Zur Entwicklung registriert
  `deep_link::register_for_development` nur noch `haex-vault` (`register("haex-vault")`) statt
  `register_all()`: holzi darf sich auf dem Rechner einer Entwicklerin nie zum mailto-Programm machen
  (FR-016). Installierte Pakete tragen `mailto` als mögliches Programm ein; die Wahl bleibt im System.
- **Android**: Der Filter für `ACTION_VIEW` kommt aus der Konfiguration (Plugin). Viele Apps rufen aber
  `ACTION_SENDTO` auf. **Offen, Spike T001**: ob das Plugin einen Link dieser Aktion an `onOpenUrl` gibt. Wenn
  nicht, kommt ein eigener Filter für `SENDTO` in `AndroidManifest.xml` (außerhalb des erzeugten Blocks) und
  `tauri-plugin-holzi-android` leitet die Daten des Intents an dasselbe Ereignis weiter.
- **Windows**: **Offen, Spike T001**: Schreibt der Installer für `mailto` Einträge unter
  `Software\Classes\mailto` (das macht holzi zum Standard und verletzt FR-016) oder unter
  `RegisteredApplications` (nur wählbar)? Schreibt er das Erste, bleibt `mailto` auf Windows aus dem Paket
  draußen, und die Spec bekommt eine Ausnahme für Windows.
- **macOS**: `CFBundleURLSchemes` macht holzi wählbar, nicht zum Standard.

### Ergebnis des Spikes (T001, 2026-10-29)

Gelesen in den Quellen, nicht auf Geräten ausprobiert: `tauri-plugin-deep-link` 2.6.2
(`android/src/main/java/DeepLinkPlugin.kt`, `build.rs`, `src/lib.rs`), `tauri-utils` 2.10.1
(`src/config/parse.rs`) und `crates/tauri-bundler` von `tauri-apps/tauri` (`bundle/windows/nsis/installer.nsi`,
`bundle/windows/msi/main.wxs`, `bundle/linux/freedesktop/mod.rs`); das CLI hier ist 2.6.2.

- **Android**: Das Plugin gibt nur Intents der Aktion `ACTION_VIEW` (und der ARC-Variante) an `onOpenUrl`
  (`isViewIntent`), in `load` und in `onNewIntent`. Ein Link der Aktion `ACTION_SENDTO` kommt nicht an. Der
  erzeugte Filter hört nur auf `VIEW`. Ein `mailto:` ohne Host und ohne Pfadregeln besteht die Prüfung
  `isDeepLink`, wenn `mobile` `{ "scheme": ["mailto"] }` enthält. Darum: (1) ein eigener Filter für
  `ACTION_SENDTO` mit `mailto` in `AndroidManifest.xml` außerhalb des erzeugten Blocks; (2) `MainActivity`
  schreibt einen `SENDTO`-Intent mit `mailto:`-Daten vor `super.onCreate` und vor `super.onNewIntent` auf
  `ACTION_VIEW` um. So braucht es keinen Weg über `tauri-plugin-holzi-android`; der Rest läuft wie bei Links
  von Spec 060. Offen bleibt nur der Versuch auf einem Gerät (T023 prüft `VIEW`; `SENDTO` bleibt ein
  Handversuch im Quickstart).
- **Windows**: Beides, NSIS (`WriteRegStr SHCTX "Software\Classes\<scheme>" …`, samt `shell\open\command`) und
  WiX (`HKLM … Software\Classes\<scheme>`), schreibt den Klassenschlüssel des Schemas. Für `mailto` würde das
  die Klasse des vorhandenen Mail-Programms ersetzen, holzi zum Standard machen (FR-016 verletzt), und der
  Deinstaller löscht den Schlüssel wieder. `mailto` kommt deshalb **nicht** in das Windows-Paket. Das CLI liest
  `plugins.deep-link.desktop` aus der Konfiguration des Ziels, und `tauri.windows.conf.json` wird über die
  Basis gelegt (`tauri-utils` `parse.rs`): Die Basis führt `["haex-vault", "mailto"]`, die Datei für Windows
  überschreibt `desktop.schemes` mit `["haex-vault"]`. Folge: Auf Windows gibt es keinen Link von außen für
  `mailto`; der Mail-Knopf und die Auswahl im Adressbuch gehen weiter (Ausnahme in FR-016 und SC-005).
- **Linux**: Das Paket trägt `x-scheme-handler/mailto` in `MimeType` der `.desktop`-Datei ein
  (`freedesktop/mod.rs`) und ruft `xdg-mime default` nicht auf. holzi ist damit wählbar. Hat ein Rechner gar
  kein Programm für `mailto`, kann `xdg-open` holzi nehmen; das ist keine Systemänderung durch holzi. Anders
  die Entwicklung: `register_all()` des Plugins ruft `xdg-mime default` und macht holzi zum Standard, darum
  `register("haex-vault")`.
- **macOS**: unverändert (`CFBundleURLSchemes`).

## R6 Eingang von außen und gesperrte Vault

- **Entscheidung**: `useDeepLink.handle` unterscheidet nach Schema: `haex-vault://contact/…` geht wie bisher
  an `shareInbox`, `mailto:` an den neuen Speicher `mailInbox`. Rust passt `contact_link` zu
  `known_link` an, das beide Schemata durchlässt (jedes mit seiner Grenze: Kontakt-Link 16 KiB wie bisher,
  `mailto:` 32 KiB); der Posteingang zwischen
  Prozessen bleibt derselbe (R9 von Spec 060).
- `mailInbox` hält höchstens einen Link: ein neuer ersetzt den wartenden, auch wenn dessen Dialog offen ist
  (Klärung 2026-10-28). Er lebt nur im Speicher des Prozesses, überlebt den Weg von `/` nach
  `/workspace/<name>` und wird nie gespeichert (FR-017).
- **Ein Link von außen geht an die zuletzt aktive Vault** (Randfall der Spec): Bei mehreren Prozessen holt
  der Prozess mit fokussiertem Fenster eine Datei sofort ab, die anderen erst nach einer Sekunde
  (`claim_next` bekommt `focused: bool`); innerhalb der Frist von 2 s des Abgebens holt so in der Regel das
  Fenster, das der Nutzer zuletzt benutzt hat. Das gilt auch für Links von Kontakten und ändert dort
  nichts außer der Reihenfolge.
- **Dialog**: `MailLinkDialog.vue` in `[instance].vue` zeigt, nach `sessionRestored`, die Zeilen der Öffner und
  „Link kopieren“; ohne Öffner den Satz „Kein Öffner installiert“ und das Kopieren (FR-019). Ein Link, den
  Rust ablehnt (`opener_mail_check`), zeigt „Der Link kann nicht gelesen werden“ ohne Zeilen (Szenario 6 von
  Geschichte 2). Er öffnet nie selbst etwas (FR-020).

## R7 Schleifenschutz

- **Entscheidung** (Klärung 2026-10-28): Bei einem Link von außen bietet der Dialog „Mit dem System öffnen“
  nie an; nur beim Mail-Knopf im Adressbuch ruft holzi `openUrl('mailto:…')` auf. Der Aufruf des Systems
  steht nur in `MailOpenMenu.vue`/`ValueActions.vue`; ein Check (`check-extensions-openers-*.ts`) stellt sicher, dass
  `MailLinkDialog.vue` `openUrl` nicht importiert.
- **Verworfen**: Erkennung des Standardprogramms je Plattform und eine Markierung beim Weiterreichen (siehe
  Klärung).

## R8 Reihenfolge über die Repos

1. holzi kann allein anfangen: Parser, `OpenRequests`, Bridge-Methode, Menü, Dialog, Eingang. Tests laufen mit
   einem Testbündel, dessen Manifest `openers` trägt (gebaut wie `model-jobs.xt`, mit dem Testschlüssel).
2. `vault-sdk`: `openers.rs` in `haex-bundle`, Typ `ExtensionManifest.openers`, Client `client.openers.take()`
   und der Ereignistyp `opener-request`; danach hebt holzi die gepinnte Revision von `haex-bundle` an (T002) und
   `haextension` die SDK-Version.
3. `haextension`: haex-mail meldet sich an und öffnet `ComposeDialog` mit den Feldern, wenn `take()` eine
   Anfrage liefert.
   Ohne Schritt 2 und 3 funktioniert alles bis auf die echte haex-mail.

## R9 Prüfung

- Parser: Tabelle der Fälle aus RFC 6068 (mehrere Empfänger, Kodierung, nicht-ASCII, ohne Empfänger, `%0d%0a`
  im Betreff, Grenzen bei 50/51 Empfängern, 998/999 Zeichen, 16 KiB/+1) mit allen Erwartungen (SC-004).
- Anfragen: parken, ersetzen, abholen, verfallen (mit abgekürzter Frist), nur die gewählte Extension holt.
- E2E Desktop: Testextension mit Anmeldung; Knopf zeigt Menü; Wahl öffnet Fenster, `take` liefert die
  Felder; ohne Anmeldung kein Menü; deaktiviert verschwindet sie.
- E2E Android: `phone.openLink('mailto:…')` (Helfer aus Spec 060): Dialog mit der Zeile; nach dem
  Entsperren; kein „Mit dem System öffnen“.
