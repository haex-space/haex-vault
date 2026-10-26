# Recherche: Kontakte teilen und der Kontakt „Ich“ (Spec 060)

Entscheidung, Begründung, verworfene Alternativen. „Geprüft“ heißt: im Code gelesen (Stand
`origin/main` `0274fe61`); „Annahme“ heißt: aus dem Code geschlossen, nicht ausgeführt.

## R1 Der Ich-Kontakt als feste Zeile

- **Entscheidung**: Migration `0042_contacts_self` legt in `haex_contacts` eine Zeile mit fester Kennung
  an (`SELF_CONTACT_ID`, eine in `contacts/ids.rs` festgeschriebene UUID). Alle übrigen Spalten sind
  konstant: `origin_kind = 'holzi'`, `origin_device` = die Vault-Scope-Kennung (nil-UUID),
  `origin_caller = 'internal:self'`, `created_at = updated_at` = fester Zeitpunkt. Jedes Gerät schreibt
  dadurch dieselbe Zeile, und beim Sync treffen sich gleiche Werte.
- **Begründung**: Der Nutzer will den Kontakt „zur Build-Zeit“. Geprüft: `haex-crdt` stempelt Zeilen, die
  eine Migration ohne HLC einfügt, beim nächsten Öffnen (Lebenszyklus Schritt 6, `database/stamp.rs`),
  sodass sie wie jede Zeile synchronisieren; Spalten für HLC dürfen im INSERT nicht stehen. Ein Vorbild
  mit Seed in eine getrackte Tabelle gibt es nicht (nur `_no_sync`), einen Bootstrap-Seed (`INSERT OR
IGNORE INTO known_devices`, `identity/bootstrap.rs:57`) schon, mit demselben Prinzip.
- **Verworfen**: Seed im Bootstrap bei jedem Öffnen. Er würde die Zeile nach einem (verbotenen) Löschen
  neu anlegen und bräuchte die Gerätekennung, die nicht auf allen Geräten gleich ist. Zufällige Kennung
  je Gerät: ergäbe nach dem Sync mehrere Ich-Kontakte.
- **Risiko**: Das Stempeln eines Migrations-Inserts ist aus der Doku von `haex-crdt` gelesen, nicht
  ausgeführt. Der Migrationstest (`migrations_contacts_self_tests.rs`) öffnet zwei Vaults und gleicht
  sie ab; schlägt er fehl, wechselt die Umsetzung auf den Bootstrap-Seed mit den festen Werten.

## R2 Was am Ich-Kontakt anders ist

Geprüft, an welchen Stellen ein Kontakt gelöscht, geändert oder mit dem Telefon verknüpft wird:

| Stelle                                      | Änderung                                                                                                       |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `service/contacts.rs` `delete`              | lehnt `SELF_CONTACT_ID` mit `ContactsSelf` ab, für jeden Aufrufer (FR-002)                                     |
| `service/contacts.rs` `update`              | lehnt einen `display_name`-Patch beim Ich-Kontakt ab (FR-003); Namensteile bleiben änderbar                    |
| `service/contacts.rs` `check_identity`      | der Ich-Kontakt darf leer sein (er ist sofort da), sonst scheitert jede Änderung an einem leeren               |
| `service/targets.rs`                        | Abgleichsziele für den Ich-Kontakt abgelehnt (FR-006)                                                          |
| `phone/run.rs` `unlinked_headers`           | filtert den Ich-Kontakt aus den Kandidaten des Abgleichs, damit kein Telefonkontakt ihm zugeordnet wird        |
| `store.rs` `headers`/`load`                 | neues Feld `is_self`; `Contact.name` bleibt der abgeleitete Name (FR-003a: Agenten sehen den echten Namen)     |
| Frontend `List.vue`, `EntryPage.vue`, Suche | zeigt `t('contacts.self')` statt `name`, steht ganz oben, kein Löschen; die Suche nutzt weiter `name` (FR-003) |

`phone/apply.rs:428` löscht nur Kontakte, die vom Telefon kamen; der Ich-Kontakt hat keine
Telefonverknüpfung und wird dort nie erreicht.

## R3 Das Format der Weitergabe

- **Entscheidung**: ein Umschlag (`ShareEnvelope`, postcard) in `haex-vault://contact/<base64url>`; der
  Umschlag trägt Namen, Namensteile, gewählte Angaben und optional die Karte als unveränderte Bytes.
  Einzelheiten in `contracts/share-format.md`.
- **Begründung**: Die Karte (Spec 051) ist signiert und unteilbar; die Angaben sind es nicht und
  brauchen es nicht. Ein Umschlag trennt beides sauber, und dieselbe Struktur gilt für den eigenen und
  für fremde Kontakte (ein Einlesepfad). `postcard` und `base64url` sind schon da (`card/format.rs`).
- **Verworfen**: (a) die Angaben in `ContactCard.values` lassen und die Karte pro Teilen signieren:
  funktioniert nur beim eigenen Kontakt; (b) vCard: Spec 060 „Nicht im Umfang“; (c) JSON statt
  postcard: größer, und die QR-Grenze (2 048 Zeichen) gilt weiter.
- **Grenze**: Gesamttext höchstens 16 384 Zeichen. Der Wert ist eine Obergrenze für Zwischenablage und
  Link, kein technisches Maximum: Android-Intents und die meisten Apps tragen mehr, manche Mail-Apps
  kürzen ab etwa 2 000 bis 8 000 Zeichen (Annahme, nicht gemessen); das Einfügen funktioniert immer
  (Spec, Annahmen).

## R4 Die frische Karte beim Ich-Kontakt

- **Entscheidung**: `own_card::build_for_share(q, selection)` baut eine `ContactCard` aus der wirksamen
  Geräteliste und den Relays (wie `own_card::refresh`), filtert Geräte und Relays nach der Auswahl, lässt
  `values` leer und unterschreibt mit `format::sign_with(card, secret, max_text = None)`. Die Karte wird
  nicht gespeichert.
- **Begründung**: `refresh` enthält den Aufbau schon; er wird in `build_card(q, devices, relays)`
  herausgezogen und von `refresh` (alle Geräte, alle Relays) und vom Teilen (Auswahl) benutzt. Die
  2 048-Zeichen-Grenze von `format::sign` gilt dem QR-Code, nicht der Karte; sie wandert auf den
  Umschlag (Geprüft: `sign` wirft heute `CardTooLarge`, wenn eine Karte ohne Angaben nicht passt, aber
  32 Geräte × 3 Texte passen nicht in 2 048 Zeichen).
- **Auf verknüpften Geräten** (kein Vault-Geheimnis, `keys::vault_secret` ist `None`): keine Karte, der
  Dialog sagt es (FR-016).
- **`issued_at`** ist `max(jetzt, neueste gespeicherte eigene Karte + 1)`; die frische Karte darf die
  gespeicherte eigene nicht unterschreiten, damit sie beim Empfänger „neuer“ ist, wenn der Empfänger
  eine Karte der Vault schon hat. Beim Teilen geschieht kein Schreiben.

## R5 Mehrere Karten je Vault, Hash als Schlüssel

- **Entscheidung** (Betreiber, 2026-10-26): `haex_contacts_cards` wird neu aufgebaut:
  `hash TEXT PRIMARY KEY` (SHA-256 des unterschriebenen Inhalts, den `card/format.rs` als `payload`
  schon kennt), `vault_pubkey TEXT NOT NULL` mit gewöhnlichem Index (kein UNIQUE, Regel am Kopf von
  `migrations_contacts.rs`, Test `unique_indexes == 0`), `contact_id`, `card`, `issued_at`,
  `received_at`. Die Adressen eines Kontakts sind die Vereinigung seiner Karten.
- **Schwellwert**: `received::stored(vault)` liefert die Karte mit der höchsten `list_generation`
  (bei Gleichstand `issued_at`). Nur sie bestimmt `foreign_trust` (`sync/foreign_auth.rs`: „eine Liste
  älter als die der Karte“ wird abgewiesen), damit eine ältere Karte mit einem entfernten Gerät den
  Schwellwert nie senkt. Zum Anwählen (`files/peer/connect.rs`, `extensions/peers/invite.rs`) dient die
  Vereinigung `received::addresses(vault)`.
- **Prüfung** (FR-025a): `received::verified(row)` prüft beim Lesen `format::from_bytes` (Format,
  Grenzen, Unterschrift), `hash == sha256(payload)` und `vault_pubkey == card.vault`. Eine Zeile, die
  das nicht besteht, wird ignoriert und im Log gemeldet. Beim Speichern berechnet holzi den Hash selbst.
- **Migration** (`0043_contacts_cards_hash`): `DROP TABLE haex_contacts_cards` und neu anlegen; keine
  Zeilen werden kopiert. Geprüft: SQLite hat keinen SHA-256, die Kennung der alten Zeilen ließe sich
  nur in Rust berechnen, und Migrationen sind SQL. Folge: **vorhandene bestätigte Adressen verschwinden
  beim Update und kommen mit dem erneuten Einlesen der Karte zurück**; die Kontakte selbst bleiben. Das
  Vorbild ist `0021_own_device_sync` (`DROP TABLE` ohne Löschmarker, `HOLZI_TRIGGER_VERSION` hoch).
  Operator-Daten sind betroffen; holzi hat sonst keine Nutzer (Spec 054) und das PR nennt es.
- **`HOLZI_TRIGGER_VERSION` 27 → 28**, weil sich die Spaltenform einer getrackten Tabelle ändert
  (`migrations.rs:57-77`: Trigger werden beim Anlegen fest eingebaut). Der Handshake vergleicht
  `crdtTrigger` zwischen Geräten (`sync/handshake.rs:102`); Geräte mit verschiedenem Stand synchronisieren
  erst, wenn beide aktualisiert sind (bestehendes Verhalten, im PR vermerkt).
- **Verworfen**: (a) `(vault_pubkey, issued_at)` als Schlüssel: zwei in derselben Sekunde mit verschiedener
  Auswahl unterschriebene Karten überschrieben sich; (b) zufällige UUID: dieselbe Karte stünde nach dem
  Einlesen auf zwei Geräten doppelt; (c) einzeln geprüfte Adresszeilen statt Karten: die Unterschrift
  ginge verloren; (d) alte Zeilen mit einem Platzhalter-Hash behalten: sie bestünden die Prüfung nie.

## R6 Einlesen: Vorschau und Entscheidungen

- **Entscheidung**: Zwei Befehle statt Kartenvorschau und Annahme der Spec 051:
  `contacts_share_preview(text, target?)` und `contacts_share_accept(text, target, decisions)`.
  Die Vorschau rechnet aus Umschlag und gewähltem Ziel eine Liste von **Posten** (Namensteile,
  Organisation, Position, Notiz, jede Angabe, die holzi-Adressen), jeder mit Zustand (`new`, `same`,
  `conflict`, `invalid`) und den erlaubten Wahlen. `accept` rechnet die Posten aus dem Text **noch
  einmal** und wendet nur Wahlen an, deren Posten-Kennung dazu passt; die Kennung ist `index` im
  Umschlag, kein Wert aus der Oberfläche (FR-022, FR-025a).
- **Wahlen** (FR-024b/c, FR-025): Angabe: `add` (Standard), `update(value_id)`, `skip`; einfeldig:
  `keep` (Standard), `update`; holzi-Adressen: `add` (Standard), `skip`, bei einer alten Karte
  zusätzlich `replace`. Die Vault-Identität eines vorhandenen Kontakts kommt nie in `update`: Der Posten
  heißt `identity` mit Zustand `mismatch` und erlaubt nur ein neues Ziel.
- **Ziel** (`target`): `new` oder `contact(id)`. Der Ich-Kontakt ist als Ziel nie erlaubt (FR-024). Hat
  die Karte eine Vault, die schon bei einem Kontakt steht (`received::stored_vault`), ist dieser Kontakt
  vorgewählt und das einzige erlaubte Ziel (Spec 051 FR-013). Treffer über Telefonnummer oder
  E-Mail-Adresse sind nur `hints`.
- **Begründung**: Alle Regeln gehören in den Dienst, nicht in die Oberfläche. Die Vorschau ist ein
  reiner Lesevorgang; `accept` schreibt in einer Transaktion (alles oder nichts).
- **Verworfen**: Wahlen als Wert-Kopien von der Oberfläche zurückgeben: ließe den Dienst auf Daten
  vertrauen, die er aus dem Text selbst rechnen kann.

## R7 Teilen: Optionen und Bauen

- **Entscheidung**: `contacts_share_options(contact_id)` liefert die Zeilen des Dialogs (Kennungen,
  Art, Bezeichnung, Wert, vorgewählt = alles), beim Ich-Kontakt die Geräte und Relays als eigene Zeilen
  und `can_sign`, beim fremden Kontakt die Zeile „holzi-Adressen (bestätigt)“ nur mit gespeicherter
  Karte. `contacts_share_build(contact_id, selection)` baut Text, QR (`Option<String>` SVG, nur
  `len <= 2048`) und die Größe. Ein Befehl für beide Fälle; die Auswahl lebt nur im Dialog (FR-010).
- **Auswahl**: `ShareSelection { values: Vec<String>, organization: bool, job_title: bool, note: bool,
holzi: HolziSelection }` mit `HolziSelection::{None, Card, Own { devices, relays }}`. Der Dienst
  prüft sie gegen den Kontakt (jede Kennung muss ihm gehören), nie gegen den Aufrufer.
- **Name** (FR-011): `share::name_for(head, vault_for_npub)`; einzige Stelle der Regel.

## R8 Aktion für Agenten (`contacts.share`)

Geprüft in `chat/tools/*`, `files/agent/share_confirm.rs`, `contacts/agent.rs`:

- **Entscheidung**: Aktion `contacts.share`, `effect: 'destructive'`, `scope: 'contacts.read'`,
  `runner: 'native'` mit eigenem Ausführer `ContactsShareAgent` (Vorbild `files/agent/shares.rs`, in
  `lib.rs` neben `SharesAgent` angemeldet). Er ruft `ContactsAgent::grants(Read)` (Voraussetzung
  „Kontakte lesen“, FR-029), liest den Kontakt über den Dienst mit `hidden()` (keine Auskunft über
  Existenz, Spec 051 FR-031), fragt über ein neues `ContactShareConfirm` (ein `PromptPark<Request,
Option<ShareSelection>>`, Ereignis `contact-share-confirm-request`, Befehl
  `contact_share_confirm_answer`, 120 s) und baut die Weitergabe erst mit der bestätigten Auswahl.
  Ablehnung, Abbruch oder Ablauf: `denied_by_user` ohne Inhalt (FR-032).
- **Eine Abfrage, nicht zwei**: Heute fragt der Chat bei `Risky` im Modus „Manuell“ und „Automatisch“
  selbst (`chat/tools/permission.rs`, `tool_round.rs`), im Modus „Plan“ lehnt er ab. Ein neues
  Merkmal `Tool::confirms_itself()` (Vorbild `always_ask`) lässt `tool_round` die allgemeine Abfrage
  überspringen, behält aber die Ablehnung im Plan-Modus. Der Dialog der Aktion ist die einzige Abfrage
  und lässt die Auswahl ändern, was die allgemeine Abfrage (`PermissionPrompt.vue`, nur Lesen) nicht kann.
- **Keine Dauer-Erlaubnis**: wie `ShareConfirm` („Every time, without remember and whatever the
  approval mode“). Geprüft: Es gibt für Aktionen keine gemerkte Zustimmung zu `Risky`.
- **Erweiterungen**: keine Schnittstelle (Spec 051 FR-029).
- **Externe Agenten (MCP)**: Der Server ist noch nicht gebaut (`Caller::ExternalAgent` ist nur ein Typ).
  Weil die Bestätigung im Ausführer der Aktion liegt, gilt sie später ohne Umbau auch dort.
- **Verworfen**: `effect: 'write'` mit eigener Bestätigung (wie `files.shares.save`): widerspricht dem
  ausdrücklichen Wunsch „zerstörerisch“ und nähme der Aktion die Sperre im Plan-Modus.

## R9 Deep Link

Geprüft: in dieser Arbeitskopie gibt es weder `tauri-plugin-deep-link` noch Single-Instance, keinen
`onOpenUrl`, keinen Intent-Filter. Die Legacy-App (`haex-vault`, Schema `haexvault`) nutzt das Plugin
2.4.9, aber nur für Desktop, ohne Single-Instance (Absicht: ein Prozess je Vault) und ohne Android.
Brauchbar ist ihr `useDeepLink.ts` (Zwischenspeicher im Modul, Wiederabspiel nach dem Öffnen der Vault).

- **Entscheidung**:
  1. `tauri-plugin-deep-link = "2.4.9"`, `@tauri-apps/plugin-deep-link`, Schema `haex-vault` in
     `tauri.conf.json` (`plugins.deep-link.desktop.schemes`, `mobile` mit Schema und Host `contact`),
     `deep-link:default` in `capabilities/default.json` und `mobile.json`, `.plugin(deep_link::init())`
     in `lib.rs`.
  2. Android: Intent-Filter `VIEW`/`DEFAULT`/`BROWSABLE` mit `<data android:scheme="haex-vault"
android:host="contact"/>` in der verfolgten `AndroidManifest.xml`; `MainActivity` hat
     `launchMode="singleTask"` schon, `onNewIntent` leitet das Plugin weiter (Annahme: Dokumentation des
     Plugins, nicht ausgeführt; der Android-E2E prüft es).
  3. Frontend: neues `useDeepLink.ts` (`getCurrent`, `onOpenUrl`, Ereignis `deep-link-received`), eine
     wartende URL im Speicher des Moduls, Wiederabspiel nach `sessionRestored` in
     `[instance].vue` (wie `consumeDeepLink`), dort öffnet `wm.app.open` die Kontakte-App auf `/import`.
     Der Text liegt in einem Pinia-Speicher (`useShareInbox`), nicht in der Adresse: 16 KiB in einem
     Query-Parameter wären fehleranfällig. Ohne offene Vault (Wähler/Entsperren) wartet die URL im
     Speicher; derselbe Prozess spielt sie nach dem Entsperren ab (Geprüft: Entsperren navigiert im
     Prozess von `/` nach `/workspace/<name>`). Beim Sperren auf dem Desktop startet die App neu; eine
     Verknüpfung, die dabei eintrifft, geht verloren (Randfall, in der Spec beschrieben).
- **Desktop: mehrere Prozesse**: holzi ist bewusst ein Prozess je Vault-Sitzung
  (`instances/presence.rs`, `chat-multi-instance`-E2E). `tauri-plugin-single-instance` würde das
  brechen und könnte den Neustart beim Sperren stören (`close_effects.rs`: Annahme, nicht geprüft).
  **Entscheidung**: ein kleiner Posteingang im Verzeichnis `<AppLocalData>/link-inbox/`:
  1. Ein neu gestarteter Prozess mit URL im Aufruf (Linux, Windows) legt sie als Datei `<uuid>.url`
     ab, wenn andere holzi-Prozesse laufen (`presence.lock` lässt sich nicht exklusiv sperren), und
     wartet bis 2 s, dass die Datei verschwindet (Rückmeldung „abgeholt“).
  2. Jeder Prozess mit **offener** Vault prüft das Verzeichnis alle 500 ms und holt eine Datei durch
     atomares `rename` in `<uuid>.claimed-<pid>`; wer gewinnt, zeigt die Vorschau (und bringt das Fenster
     nach vorn); der Prozess ohne offene Vault ignoriert den Posteingang.
  3. Holt keiner die Datei ab, löscht der neue Prozess sie und hält die URL selbst, bis der Nutzer
     eine Vault öffnet.
     macOS liefert die URL an die laufende App (`onOpenUrl`); dort ist nichts weiter nötig.
     Bei mehreren Vaults in mehreren Prozessen gewinnt der erste, der abholt. Die Vorschau nennt deshalb
     den Namen der Vault, in die gespeichert würde (Abbrechen ist immer möglich).
- **Risiko / Spike zuerst**: Dieser Teil ist am wenigsten gesichert (Deep Link auf Linux nur mit
  installiertem Paket, im Entwicklungsbetrieb `register_all`; Android-Emulator-Probe wie
  bei den anderen Android-E2E-Szenen). Aufgabe T-Spike (am Anfang): Plugin einbinden, ein Link öffnet
  die App auf Android und Linux. Scheitert der Posteingang, fällt die Umsetzung auf „neuer Prozess
  hält den Link selbst“ zurück (Schritt 3 allein); die Spec bleibt erfüllt, weil die Vorschau dann nach
  dem Entsperren im neuen Prozess erscheint.
- **Verworfen**: Single-Instance-Plugin (Mehrprozess-Entwurf); Sockets oder Named Pipes (plattformspezifisch,
  mehr Code als ein Verzeichnis); Weiterleitung über iroh (die Prozesse gehören zu verschiedenen Vaults).

## R10 Entfernen des Alten

- Entfällt: `contacts_own_card`, `contacts_own_card_set`, `contacts_card_preview`, `contacts_card_accept`,
  `OwnCard.vue`, `ContactsCardSetting.vue`, die Route `/card`, die Vault-Einstellungen
  `contacts.card.own_contact` und `contacts.card.values`, `OwnCard`/`Choice`/`set_choice`/`read` in
  `card/own_card.rs`, `CardImport.vue` (wird `ShareImport.vue`).
- Bleibt: `own_card::refresh`/`refresh_quietly` und die Tabelle `haex_contacts_own_card` (FR-028). Geprüft:
  `refresh` hängt an `sync/servers.rs:255` und den Orten, die Geräte­listen signieren; die Peer-Tests von
  Spec 057 (`extensions/peers/test_support.rs:415`) lesen die gespeicherte Karte. Ob die gespeicherte
  eigene Karte nach dem Ende des Teilens über die Einstellungen noch einen Verbraucher braucht, ist
  offen; entfernen verschiebt sich auf einen eigenen Schritt (kein Umbau von Spec 057 hier).
- Keine Migration der Einstellungen: holzi hat keine Nutzer (Spec 054).

## R11 Sicherheit

- Das Öffnen eines Links speichert nichts (FR-018a); jeder Schreibweg ist `contacts_share_accept` mit
  Wahl des Nutzers, und die Oberfläche ruft ihn nur aus der Vorschau.
- Die Karte wird beim Einlesen, beim Speichern und bei jeder Verwendung der Adressen geprüft (R5).
- Die Vault-Identität eines Kontakts ändert sich über keinen Import (R6).
- Eine Karte mit eigener Vault, mit blockierter Vault (`received::is_blocked`) oder mit ungültiger
  Unterschrift lässt keine Angabe des Umschlags ins Adressbuch (FR-022, Spec 051 FR-039).
- Der Agent bekommt den Text erst nach der Bestätigung (R8); die Abfrage prüft `request_id` und Aufrufer
  wie `ShareConfirm`.

## R12 Tests (Auswahl; vollständige Liste in `tasks.md`)

- Rust: `share/envelope_tests.rs` (Roundtrip, jede Grenze, unbekannte Version, übrige Bytes, Karte
  ungültig/eigene/blockiert), `share/build_tests.rs` (Auswahl wird entfernt, Namensregel, frische Karte
  mit Teilmenge, verknüpftes Gerät ohne Geheimnis), `share/merge_tests.rs` (Handynummer bleibt,
  `add`/`update`/`skip`, einfeldige Felder, Identität nie ersetzt, Vereinigung der Adressen, ältere Karte
  ändert nichts, `replace` nur bei alter Karte), `card/received_tests.rs` (Hash-Schlüssel, Prüfung beim
  Lesen, Zeile unter fremder Vault wird ignoriert), `service/contacts_tests.rs` (Ich nicht löschbar,
  `display_name` gesperrt, leer erlaubt, kein Ziel), `identity/migrations_contacts_self_tests.rs`
  und `migrations_contacts_cards_hash_tests.rs`, `files/agent`-nahe Tests für `ContactsShareAgent`
  (Ablehnung, Auswahl eingeschränkt, ohne Freigabe `NotFound`).
- Frontend: `scripts/check-contacts-share.ts` (Optionen → Auswahl → Anzeige, „Ich“ als Anzeige,
  Posten-Zustände), `check-contacts-search.ts` (Ich über echten Namen).
- E2E: `contacts-share.test.ts` (Ich pflegen, teilen mit Abwahl, in zweiter Vault einlesen,
  Handynummer bleibt, Identität wird nicht ersetzt), `agent-contacts.test.ts` erweitert (Teilen,
  Bestätigen, Ablehnen); Android-Probe für den Deep Link.
- Vor dem Push: `pnpm typecheck:scripts`, `pnpm check:contacts`, `pnpm check:agent-actions`, alle
  `check:*`, `cargo clippy` auch für `aarch64-linux-android` (Rust mit `cfg`-Zweigen), gezielte `cargo test -j 4`.
