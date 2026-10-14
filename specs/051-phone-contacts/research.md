# Research: Adressbuch mit Abgleich zum Telefon-Adressbuch

Entscheidungen für den Plan von Spec 051. Jede nennt die Wahl, die Begründung und die verworfenen
Alternativen. Ausgangspunkt war eine Abfrage des Graphen (graphify) des Abzweigs zu Passwortmanager,
Android-Plugin, Geräteliste, Nostr und Erweiterungs-Berechtigungen; danach gezielte Lektüre.

## R1 Kontakte als eigener Dienst nach dem Muster des Passwortmanagers

- **Entscheidung**: Neues Modul `src-tauri/src/contacts/` mit `ContactsService` als einzigem Zugang zu
  den Tabellen `haex_contacts_*`, gebaut wie `PasswordsService` (`passwords/service/mod.rs`): privates
  `VaultDb`, jede Methode nimmt zuerst den `Caller` und ruft eine reine Prüffunktion in
  `contacts/access.rs`. Schreiben über `VaultDb::write` (ein `CrdtTransaction`, ein HLC).
- **Begründung**: FR-028 verlangt einen Zugang mit einer Prüfung für alle Aufrufer. Das Muster ist in
  holzi erprobt (Spec 034, ADR 0007), auch die Ablehnung als `NotFound` (FR-031).
- **Alternativen**: Kontakte als Daten einer mitgelieferten Erweiterung (verworfen: Kontakte sind
  Kerndaten, Annahme der Spec; Erweiterungen kämen nicht ohne Umweg an Telefon und Visitenkarte);
  direkte SQL-Befehle aus dem Frontend (verworfen: keine Prüfung möglich).

## R2 `Caller` wird gemeinsam genutzt

- **Entscheidung**: `enum Caller` zieht aus `passwords/access.rs` nach `src-tauri/src/caller.rs`;
  `passwords::access` exportiert ihn weiter unter dem alten Pfad. Grants und Scopes bleiben je Dienst
  (`contacts::access::{Grant, Scope}` mit `Scope::All | Groups(BTreeSet<GroupId>)`).
- **Begründung**: Wer aufruft, ist für alle Dienste dasselbe (eine Regel, eine Stelle). Was ein Grant
  abdeckt, unterscheidet sich (Tags gegen Gruppen).
- **Alternativen**: eigener `Caller` für Kontakte (verworfen: doppelte Regel); gemeinsamer generischer
  Grant-Typ (verworfen: vorzeitige Abstraktion für zwei Dienste mit verschiedenen Bereichen).

## R3 Datenmodell: Kopf, Angaben als offene Zeilen

- **Entscheidung**: Ein Kontakt ist eine Zeile in `haex_contacts` (Namensteile, Organisation, Notiz,
  Foto, Herkunft) und beliebig viele Zeilen in `haex_contacts_values` mit `kind TEXT`, `label`,
  `value` (Text oder JSON für zusammengesetzte Arten wie Anschriften) und `position`. Details in
  [data-model.md](./data-model.md).
- **Begründung**: haex-crdt löst Konflikte je Spalte (LWW). Je Angabe eine Zeile heißt: zwei Geräte
  ändern zwei Nummern desselben Kontakts ohne Konflikt; eine neue Art ist nur ein neuer `kind`-Wert,
  keine Migration (FR-007); eine Zeile mit unbekannter Art bleibt unberührt, solange der Editor nur
  geänderte Zeilen schreibt.
- **Regel für Schreiber**: Ändern heißt `UPDATE` der betroffenen Zeilen, nie „alle löschen und neu
  einfügen“ (sonst verliert ein älteres Gerät unbekannte Arten, und Löschen und Einfügen derselben ID in
  einer Transaktion lässt das Löschen gewinnen, `storage/models.rs:327`).
- **Alternativen**: ein JSON-Feld je Kontakt (verworfen: eine Änderung überschreibt gleichzeitige
  Änderungen anderer Angaben); eine Spalte je Art (verworfen: jede neue Art eine Migration, alte
  Versionen verlieren nichts, kennen aber die Spalte nicht).

## R4 Herkunft und Abgleichsziele

- **Entscheidung**: Herkunft als Spalten in `haex_contacts` (`origin_kind`, `origin_device`,
  `origin_caller`, `origin_account`), beim Anlegen geschrieben, danach von keinem Pfad geändert
  (Prüfung im Dienst). Abgleichsziele als Tabelle `haex_contacts_targets` mit ID
  `UUIDv5(contact_id, vault_device_uuid)` und dem Telefonkonto.
- **Begründung**: Ziele werden auf jedem Gerät gesetzt (auch am Desktop) und müssen auf das Telefon
  kommen, also synchronisierte Daten. Die abgeleitete ID verhindert doppelte Ziele ohne UNIQUE (UNIQUE
  hält den Sync an).
- **Alternativen**: Ziele über Gruppen (verworfen: der Betreiber nannte Tags als Möglichkeit, die Spec
  hat Ziele je Kontakt mit Mehrfachauswahl und „ganze Gruppe“ als Bedienweg, FR-018c).

## R5 Abgleich mit Android: Kotlin dünn, Logik in Rust

- **Entscheidung**: Das Plugin `holzi-android` bekommt Befehle, die nur Zeilen des Contacts Providers
  lesen und schreiben ([contracts/android-plugin.md](./contracts/android-plugin.md)). Abgleich,
  Zusammenführen, Konfliktbasis, Massenlösch-Sperre und Zielkonto liegen in
  `src-tauri/src/contacts/phone/` hinter einem Trait `PhoneBook`, getestet auf dem Desktop mit einer
  Fälschung.
- **Begründung**: ADR 0010: keine zweite Umsetzung einer Regel auf Android; testbar ohne Emulator.
- **Alternativen**: Android-SyncAdapter mit eigenem Konto (verworfen: holzi soll die Konten des
  Nutzers abgleichen, nicht ein eigenes anlegen; braucht einen Dienst, der im Hintergrund läuft, Spec 043
  FR-018 verbietet das); `ContentObserver` (verworfen: feuert auch für eigene Schreibvorgänge, und
  holzi läuft ohnehin nur im Vordergrund).

## R6 Änderungen erkennen: `RawContacts.VERSION` je Zuordnung

- **Entscheidung**: Je Telefon und Roh-Kontakt speichert holzi `raw_contact_id`, Konto, `SOURCE_ID`,
  `VERSION` und die **Basis**: die zuletzt abgeglichene Fassung der abgebildeten Angaben (JSON). Beim
  Abgleich liest holzi `_ID, VERSION, DELETED, ACCOUNT_*, SOURCE_ID, CONTACT_ID` aller Roh-Kontakte und
  vergleicht:
  - neue ID → übernehmen oder zusammenführen
  - höhere `VERSION` → Daten lesen, Drei-Wege-Vergleich Basis/Telefon/holzi je Angabe
  - fehlende ID oder `DELETED=1` → Löschung auf dem Telefon
  - nach eigenem Schreiben neue `VERSION` sofort zurücklesen und speichern, damit eigene Änderungen nicht
    als Telefonänderung zurückkommen
- **Begründung**: `DIRTY` gehört dem Sync-Adapter des Kontos; `CONTACT_LAST_UPDATED_TIMESTAMP` und
  `DeletedContacts` gelten nur für zusammengefasste Kontakte. `VERSION` steigt bei jeder Änderung des
  Roh-Kontakts oder seiner Daten. Die Basis macht den Vergleich je Angabe möglich (FR-022, FR-023).
- **Zeitpunkte**: nach dem Öffnen der Vault, bei `RunEvent::Resumed` (`lib.rs:606`, neuer `Wake` in
  `sync/resume.rs`) und nach eingehenden Änderungen an Kontakten (Abonnement auf
  `vault-data-changed`), entprellt auf einen Lauf.
- **Alternativen**: vollständiger Vergleich aller Daten bei jedem Lauf (verworfen: SC-002 bei 2.000
  Kontakten nicht haltbar).

## R7 Zusammengefasste Kontakte des Telefons

- **Entscheidung**: Ein holzi-Kontakt bildet einen zusammengefassten Android-Kontakt ab
  (`Contacts.LOOKUP_KEY`); jeder beteiligte Roh-Kontakt hat eine eigene Zuordnungszeile mit derselben
  `contact_id`. Beim Lesen werden die Angaben aller Roh-Kontakte vereinigt (gleiche Werte einmal). Eine
  geänderte Angabe schreibt holzi in den Roh-Kontakt, der sie hält; eine neue Angabe in den Roh-Kontakt
  des Abgleichsziels, sonst in den ersten beschreibbaren. Roh-Kontakte aus nur lesbaren Konten
  (`RAW_CONTACT_IS_READ_ONLY`, Sync-Adapter ohne `supportsUploading`) sind nur Quelle.
- **Begründung**: Edge Case der Spec („eine Person ist in holzi ein Kontakt“); vermeidet Doppel aus
  Google- und Messenger-Konten.
- **Alternativen**: ein holzi-Kontakt je Roh-Kontakt (verworfen: Doppel).

## R8 Zusammenführen beim ersten Abgleich

- **Entscheidung**: Vergleichsschlüssel:
  - Name: Anzeigename und „Vorname Nachname“ in beiden Reihenfolgen, NFKC, Kleinschreibung, Leerraum
    zusammengefasst (`unicode-normalization` ist vorhanden).
  - Telefonnummer: nur Ziffern und führendes `+`; gleich, wenn die längere mit der kürzeren endet und
    die kürzere mindestens 7 Ziffern hat (deckt `+49 170 …` gegen `0170 …`).
  - E-Mail: getrimmt, Kleinschreibung.
  - Eindeutig = Name gleich und mindestens eine Nummer oder E-Mail gleich, und genau ein Kandidat.
    Unsicher = nur Name gleich, nur Nummer/E-Mail gleich, oder mehrere Kandidaten (FR-019).
- **Begründung**: Klärung der Spec; keine neue Abhängigkeit.
- **Alternativen**: `phonenumber`-Crate (verworfen: große Metadaten, eine Abhängigkeit für einen
  Vergleich, den die Endungsregel ausreichend abdeckt).

## R9 Massenlöschung

- **Entscheidung**: Vor dem Anwenden zählt der Abgleich die Löschungen je Richtung. Mehr als 10 und
  mehr als 10 % der abgeglichenen Kontakte dieses Telefons → keine Löschung, Lauf hält an, Frage an den
  Nutzer (FR-016). Fehlende Berechtigung oder ein verschwundenes Konto (alle Roh-Kontakte eines Kontos
  weg, Konto nicht mehr in der Kontenliste) gelten nie als Löschung, sondern lösen die Zuordnung
  (FR-026).

## R10 Fotos

- **Entscheidung**: Fotos liegen inhaltsadressiert in `haex_contacts_binaries` (SHA-256, wie
  `haex_passwords_binaries`), höchstens 512 × 512 Pixel, JPEG, höchstens 200 KiB (FR-005). Verkleinert
  wird mit `image` + `fast_image_resize` wie in `files/thumbnails.rs`. Vom Telefon gelesen wird das
  große Foto (`DisplayPhoto`); zurückgeschrieben nur, wenn sich das holzi-Foto geändert hat (Vergleich
  über den Hash der Basis, nicht über das Telefonbild, weil der Provider beim Schreiben neu skaliert).
- **Alternativen**: Tabelle des Passwortmanagers mitnutzen (verworfen: fremder Dienst, eigene
  Aufräumregeln).

## R11 Suche

- **Entscheidung**: Die App lädt die Kopfdaten aller Kontakte (Name, Organisation, Nummern, E-Mails,
  Gruppen) und sucht im Frontend nach Teilzeichenfolgen in einem vorberechneten Suchtext je Kontakt
  (Kleinschreibung; Nummern zusätzlich nur als Ziffern). Agenten und Erweiterungen suchen im Dienst mit
  derselben Normalisierung (eine Funktion in Rust, Frontend nutzt die gleiche Regel aus
  `src/lib/contacts/search.ts` mit geteilten Testfällen).
- **Begründung**: SC-007 (5.000 Kontakte, < 200 ms) ist mit einem Durchlauf über vorbereitete Texte
  erreichbar; die Spec verlangt „enthält“, keine unscharfe Suche. Kein FTS5 (holzi nutzt es nirgends).

## R12 Visitenkarte

- **Entscheidung**: Struktur `ContactCard` (postcard), unterschrieben mit der Vault-Identität über
  `signing::sign(Domain::ContactCard, …)` (neuer Tag `holzi-contact-card/v1`). Inhalt: Version,
  Vault-Identität, `issued_at`, Generation der Geräteliste, Geräte (Geräteschlüssel, iroh-Endpunkt,
  iroh-Relay falls bekannt), Nostr-Relays (`ServerConfig::effective_nostr_relays`), Name, gewählte
  Angaben. Text-Form `holzi:card:` + base64url(postcard ‖ Signatur), dieselbe Zeichenkette als Link und
  QR-Code (`qrcode` wie `LinkCode::qr_svg`). Höchstens 2 KiB, sonst lässt holzi beim Erstellen
  Angaben weg und sagt es. Format in [contracts/contact-card.md](./contracts/contact-card.md).
- **Wer unterschreibt**: nur Hauptgeräte (`keys::vault_secret()` ist dort `Some`). Die zuletzt
  erstellte eigene Karte liegt synchronisiert in `haex_contacts_own_card`, damit verknüpfte Geräte sie
  zeigen. Neu erstellt wird beim Wechsel der Geräteliste (die fünf Stellen, die eine Liste
  unterschreiben, rufen danach `own_card::refresh`), bei Änderung der Nostr-Relays und bei Änderung der
  gewählten Angaben (FR-010).
- **Warum nicht die unterschriebene Geräteliste mitgeben**: Ihre Regeln (Generation, `base_list_hash`)
  kann ein Fremder ohne frühere Listen nicht prüfen, und Gerätenamen sind mit dem eigenen
  Inhaltsschlüssel versiegelt. Die Karte bestätigt die Geräte mit der Unterschrift der Vault-Identität.
- **Alternativen**: Nostr-Profil (kind 0) veröffentlichen (verworfen: öffentlich, verrät Geräte an
  alle).

## R13 QR-Code einlesen

- **Entscheidung**: `tauri-plugin-barcode-scanner` (offizielles Tauri-Plugin, Android und iOS; Version
  2.x, beim Einbau auf die neueste veröffentlichte festgelegt), nur in `cfg(mobile)` registriert. Die
  Kamera-Berechtigung fragt das Plugin selbst. Am Desktop: Link einfügen (Zwischenablage).
- **Begründung**: holzi hat keinen Scanner. Ein Scanner im WebView bräuchte Kamerafreigaben über den
  `WebChromeClient`, die Spec 043 bewusst nicht öffnet.
- **Alternativen**: `html5-qrcode`/`jsQR` im WebView (verworfen, siehe oben); eigener Kotlin-Scanner mit
  ML Kit (verworfen: das offizielle Plugin tut genau das).
- **Abweichung von ADR 0010**: ADR 0010 hält Android-Code in `holzi-android`; das Plugin ist wie
  `tauri-plugin-notification` ein Upstream-Plugin und kein eigener Android-Code. Kein Widerspruch.

## R14 Erweiterungen

- **Entscheidung**: `PermissionKind::Contacts`, Aktion nur `read`, Ziel `*` oder Gruppenname
  (`Target::Tag`, wie Passwörter). Brückenmethoden `extension_contacts_list`, `_read`, `_search` in
  `extensions/contacts.rs` nach dem Muster von `extensions/passwords.rs` (`permissions`, `grants_of`,
  `refused` mit 1004 für „fragen“). Neue API `contacts` im vault-sdk (eigener PR, Revision im Plan der
  Umsetzung mit vollem SHA). Vertrag in [contracts/bridge.md](./contracts/bridge.md).
- **Alternativen**: SQL-Lesezugriff auf `haex_contacts_*` (verworfen: Spec 017 FR-025, Kerntabellen
  bleiben zu).

## R15 Agenten

- **Entscheidung**: Scopes `contacts.read` und `contacts.write` in `src/lib/actions/scopes.ts`; Aktionen
  `contacts.search` (read), `contacts.get` (read), `contacts.update` (write) in
  `src/lib/actions/contactsActions.ts`. Die Rückfrage mit Merken nutzt die Tabelle
  `agent_file_permissions` und den Dialog aus Spec 044 mit einer neuen Art `contacts`
  (`kind='contacts'`, `target='*'` oder `group:<id>`, Status erlaubt/verweigert) statt einer zweiten
  Tabelle; Ereignis und Dialog werden dafür um die Art erweitert.
- **Begründung**: FR-030 und Klärung (auch der eingebaute Agent nur nach Freigabe, Antwort gemerkt).
  Ein Mechanismus für „Agent fragt, Antwort wird gemerkt“, bis Spec 021 Freigaben allgemein regelt.
- **Alternativen**: eigene Tabelle `agent_contact_permissions` (verworfen: zweite Umsetzung derselben
  Regel); Freigabe nur über `agentCallable` (verworfen: kein Merken, keine Ablehnung je Agent).

## R16 Einladung aus dem Adressbuch (US7) – Übertragung

- **Entscheidung**: Gespeicherte NIP-59-Gift-Wraps (kind 1059) mit NIP-40-Ablauf gleich dem Ablauf der
  Einladung, je Empfängergerät einer, `p`-Tag = Geräteschlüssel des Empfängers aus der Karte, gesendet an
  die Nostr-Relays der Karte des Empfängers. Innen: Seal mit dem Geräteschlüssel des Absenders, Rumor
  eigener Art mit den Angaben des Einladungslinks (027 FR-008), dem Einmalgeheimnis und der Karte des
  Absenders. `presence::wrap_payload`/`unwrap_rumor` werden dafür um die gespeicherte Art
  erweitert. Jedes Gerät abonniert kind 1059 mit seinem `p`-Tag auf den eigenen Nostr-Relays. Vertrag in
  [contracts/nostr-invite.md](./contracts/nostr-invite.md).
- **Begründung**: holzi nutzt heute nur flüchtige Ereignisse; Zustellung an Offline-Empfänger braucht
  gespeicherte (FR-036). Das Relay sieht nur einen Einmalschlüssel und den `p`-Tag, weder Inhalt noch
  Vault-Identität des Absenders (FR-035). Der Sync-Server (026) ist nicht gebaut.
- **Zeitpunkt**: Umsetzung erst mit Spec 027 oder 028 (Phasen-Regel); dieser Plan legt nur den Vertrag
  fest. Die Entscheidung D33 des Sync-Entwurfs wird durch ADR 0014 abgelöst.
- **Alternativen**: NIP-17 (verworfen: NIP-17 ist für Chat-Nachrichten mit Relay-Listen kind 10050;
  holzi braucht Gerät statt Person als Ziel); Postfach des Sync-Servers (später zusätzlich, FR-035).

## R17 Einstellungen

- **Entscheidung**: Kategorie `contacts` in `src/lib/settings/registry.ts`, Ansicht
  `src/components/settings/ContactsView.vue`: Abgleich (nur mit Fähigkeit `phoneContacts`), Zielkonto,
  Stand, nicht angekommene Änderungen, Angaben der eigenen Karte, blockierte Vaults. Gerätebezogene
  Werte (`contacts.phone.enabled`, `contacts.phone.target_account`) in `preferences` mit
  `PrefScope::Device`, Auswahl speichert sofort.
- **Fähigkeit**: `PlatformCapabilities.phone_contacts` (true nur unter `ANDROID`).

## R18 Tests

- **Rust**: `contacts/*_tests.rs` für Dienst, Prüfungen, Zusammenführen, Drei-Wege-Vergleich,
  Massenlöschung (Fälschung `FakePhoneBook`), Karte (Unterschrift, Ablehnung, ältere/neuere).
- **Frontend**: Vitest für Suche und Formular-Logik.
- **E2E Desktop**: Adressbuch anlegen/suchen/ändern, zwei Geräte synchronisieren.
- **E2E Android**: `PhoneControls` um `allowContacts`, `refuseContacts`, `insertContact`,
  `queryContacts`, `deleteContact` (über `adb shell content …`) erweitert; Szenarien mit
  `needs: { phone: true }`; Aufräumen der Provider-Zeilen, weil `pm clear` sie nicht löscht.
- **Von Hand**: echtes Telefon mit Google-Konto (Zusammenfassung, nur lesbare Konten, Fotos), siehe
  [quickstart.md](./quickstart.md).
