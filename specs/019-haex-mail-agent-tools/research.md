# Research: haex-mail für den Agenten, Regeln und Regel-Oberfläche

Stand der geprüften Quellen (Constitution IV: Repository, voller Commit, Pfad):

- holzi: Commit `1b02229c` (Basis dieses Branchs), `src-tauri/src/extensions/mail/`,
  `src-tauri/src/extensions/bridge/allowlist.rs`, `src-tauri/src/chat/tools/action_tool.rs`.
- haex-space/haextension, Commit `e2622baefd4c4b1c0e93e6cbe53bc6feff8751f5`, `apps/haex-mail/`.
- haex-space/vault-sdk, Branch `feat/agent-tools` Commit `cf2dbcb8849f8be62d71fa957dbfa43d131756ab`
  (MCP-Baustein), `main` Commit `ee0a0306a938d785a95a4eed76e8cde29205880d` (ohne ihn).

## R1: Lieferung in drei Etappen, Reihenfolge

**Entscheidung**: E1 vault-sdk veröffentlicht `feat/agent-tools`. E2 besteht aus zwei unabhängigen Teilen:
holzi bekommt drei Bridge-Methoden, vault-sdk bekommt die Gegenstücke. E3 ist haex-mail und hängt an der
veröffentlichten Version aus E1 und E2b.

**Begründung**: Der MCP-Baustein ist fertig und gegen holzis Mitschnitte geprüft (`cf2dbcb8` auf dem
Branch, nicht auf `main`). E2a (holzi) hat keine Abhängigkeit zum SDK und lässt sich mit dem Testserver
prüfen. E2b ist reiner TypeScript-Code. haex-mail braucht beides in einer Version, denn `sdk.tools` und
die drei Methoden kommen aus derselben Bibliothek.

**Verworfen**: Ein gemeinsamer Branch in allen drei Repos (nicht einzeln prüfbar, FR-034); E2b vor E1
(zwei SDK-Releases kurz nacheinander ohne Gewinn).

## R2: Host-Befehl Suche

**Entscheidung**: `extension_mail_search {imap, mailbox, criteria, limit?}` → `{uids, total}`. `criteria` ist
ein Objekt (`text`, `from`, `to`, `subject`, `since`, `before`, `unseen`, `flagged`), kein roher
IMAP-Ausdruck. Der Host baut `UID SEARCH` selbst, maskiert Zeichenketten und verweigert Steuerzeichen
(wie `check_mailbox`). Treffer kommen als UIDs, neueste zuerst, höchstens 1000 (`MAX_ENVELOPES`);
`total` nennt die Zahl aller Treffer.

**Begründung**: Ein Objekt verhindert, dass Mailtext oder Modelleingabe IMAP-Befehle einschleust
(Prompt-Injection, FR-005). UIDs allein halten die Antwort klein; Umschläge holt die Erweiterung mit dem
vorhandenen `fetch_envelopes` (`uidList`). Die Berechtigung ist `fetch` wie bei allen IMAP-Befehlen
(FR-032).

**Risiko (erste Aufgabe in E2a)**: Nicht-ASCII-Suchtext. IMAP verlangt `CHARSET UTF-8` und ein Literal
oder `UTF8=ACCEPT`. Der Test klärt, ob `async-imap` ein Literal im `UID SEARCH` unterstützt. Fällt das
aus, antwortet der Host mit Fehler 3001 für Nicht-ASCII, und haex-mail filtert dann Umschläge der
neuesten 1000 Mails lokal (Rückfall, in der Antwort gekennzeichnet).

**Verworfen**: Roher IMAP-String (Injektion); Suche in der Erweiterung über alle Umschläge (SC-004 verlangt
Treffer außerhalb der geladenen 50, und 5000 Umschläge je Suche sind zu teuer).

## R3: Host-Befehl Ordner anlegen

**Entscheidung**: `extension_mail_create_mailbox {imap, mailbox}` → `{created: boolean}`. Existiert der
Ordner schon (LIST vorher, oder `ALREADYEXISTS`-Antwort), kommt `created: false` ohne Fehler (Edge Case
„Ordnername schon vergeben“). Berechtigung `fetch`; Name nach `check_mailbox`.

## R4: Host-Befehl endgültig löschen

**Entscheidung**: `extension_mail_delete_messages {imap, mailbox, uids}` → `null`, höchstens 1000 UIDs.
Der Host prüft zuerst `UIDPLUS`; fehlt es, antwortet er mit 2005 („the server cannot UID EXPUNGE“) **bevor**
er etwas markiert. Dann `UID STORE +FLAGS (\Deleted)` und `UID EXPUNGE` nur für diese UIDs.

**Begründung**: Ein `EXPUNGE` ohne UID würde auch fremde `\Deleted`-Mails des Ordners entfernen (FR-012).
`move_messages` geht denselben Weg und hat dafür dieselbe Regel (`imap.rs`, `move_messages`); die neuen Funktionen liegen in `imap_manage.rs`, weil `imap.rs` mit 460 Zeilen sonst die Grenze von 500 reißt.

## R5: Aufbau der Werkzeuge in haex-mail

**Entscheidung**: Handler in `app/tools/` nach Themen (`accounts.ts`, `mailboxes.ts`, `messages.ts`,
`drafts.ts`, `rules.ts`), je Datei unter 500 Zeilen. Ein Plugin `app/plugins/agent-tools.client.ts`
registriert alle Handler mit `client.tools.register(name, handler)`, sobald der Client steht. Jeder
Handler wartet zuerst auf `haexVault.initializeAsync()` und `accountsStore.loadAccountsAsync()` und nutzt
dann dieselben Stores wie die Oberfläche (`accounts`, `mail`, neu `rules`). Schemas, Titel,
Beschreibungen und Beispielsätze stehen nur im Manifest (`haextension/manifest.json`), nicht im Code.

**Begründung**: Wird haex-mail für einen Aufruf im Hintergrund gestartet (018 FR-012), lädt kein Seitenaufruf
den Store. Ohne das Warten greift der Handler auf einen leeren Store zu. Die Stores zu nutzen heißt, dass
Werkzeug und Oberfläche dieselbe Zugangsdaten-Auflösung (`getCredentialsCachedAsync`) haben und Mails nie
doppelt eingelesen werden (FR-030).

## R6: Mails finden

**Entscheidung**: `find_messages` fragt immer den Server, nicht den Cache. Filter, die IMAP kennt
(ungelesen, markiert, Absender, Empfänger, Betreff, Text, Zeitraum), laufen über `search`; danach holt
die Erweiterung die Umschläge der angeforderten Seite. Den Filter „Anhang“ kennt IMAP SEARCH nicht; er
wirkt nach dem Holen auf die Umschläge der Treffer (höchstens 1000). Sortierung: `date` (Standard, UID
absteigend, ohne Zusatzkosten), `sender` und `subject` wirken auf die 200 neuesten Treffer
(`window: 200` im Ergebnis). Ohne `account` laufen die Abfragen parallel über alle Konten und werden nach
Datum zusammengeführt. Seitengröße: Standard 25, Obergrenze 50 (die Eingabe darf das Schema nicht
begrenzen, der Handler kürzt auf 50).

**Begründung**: Der Cache enthält nur geladene Umschläge (SC-004). Die Obergrenze 50 hält das Ergebnis bei
rund 15 KiB, weit unter der Grenze von 64 KiB (`agent_tools/tool.rs`, `MAX_RESULT_BYTES`). Das Schema
darf nur `type`, `description`, `properties`, `required`, `items`, `enum` enthalten (`schema_in_subset`);
Zeiträume sind deshalb Zeichenketten (`YYYY-MM-DD`), Zahlen haben keine Schranken im Schema.

**Verworfen**: Immer lokal filtern (verfehlt SC-004); alle Sortierfelder auf allen Treffern (bis zu 1000
Umschläge je Aufruf für eine Sortierung).

## R7: Ordnerliste und Zähler

**Entscheidung**: `list_mailboxes` holt je Konto `list_mailboxes` mit `includeStatus: true` live vom Server
(Antwort des offenen Punkts „live oder Cache“, FR-008). Scheitert ein Konto, liefert das Werkzeug dessen
Cache-Zähler mit `cached: true` und dem Grund; die übrigen Konten bleiben unberührt. Rollen kommen aus
`assignRoles` (`lib/mailboxRoles.ts`), damit „Papierkorb“ derselbe Ordner ist wie in der Oberfläche.

**Begründung**: SC-003 verlangt dieselbe Zahl wie in haex-mail; die Oberfläche aktualisiert ihre Zähler per
STATUS. Der Cache kann bis zu 90 Sekunden alt sein.

## R8: Lesen einer Mail

**Entscheidung**: `get_message {id}` nutzt `fetch_message`; der Host liest mit `BODY.PEEK`
(`mail/imap.rs`), setzt also nichts auf gelesen (FR-007). Text bevorzugt `bodyText`, sonst
`htmlToText(bodyHtml)` (`lib/mailHtml.ts`). Obergrenze 12 000 Zeichen, bei Kürzung `truncated: true`.
Die Kennung ist die vorhandene `messages.id` (`accountId::mailbox::uid`), stabil und ohne neues Format.

## R9: Entwürfe und Senden

**Entscheidung**: `create_draft` baut mit `buildReplyContextAsync` (`stores/mail/reply.ts`) Antwort,
Allen-Antworten und Weiterleitung, übernimmt Anhänge der Vorlage **nicht** (Nicht im Umfang),
baut die Mail mit `buildRfc822Async` und legt sie mit `appendMessageAsync` im Ordner mit Rolle `drafts`
ab (Flags `\Draft`, `\Seen`). `APPEND` meldet keine UID; deshalb holt die Erweiterung danach die 20
neuesten Umschläge des Entwurfsordners und sucht die Message-ID. Die Entwurfs-Kennung ist wie bei
Mails `accountId::<Entwurfsordner>::uid`. `send_mail {draftId}` liest den Entwurf (`fetch_message`),
sendet per `sendMessageAsync`, legt bei Erfolg die Kopie in „Gesendet“ ab und löscht den Entwurf mit
`delete_messages`; scheitert das Senden, bleibt der Entwurf (FR-014). Die Zustimmungsabfrage zeigt nur die
Eingabe (018 FR-011). Deshalb trägt `send_mail` Empfänger, Betreff und Text als Eingabe, und der Handler
sendet nur, wenn sie dem Entwurf entsprechen (sonst Fehler mit den Werten des Entwurfs). So zeigt die Abfrage
das, was geht (FR-015), ohne dass 018 geändert wird.

**Folge**: `send_mail` braucht den neuen Löschbefehl aus E2. Ohne ihn ließe sich der Entwurf nur in den
Papierkorb schieben, was Papierkorb-Reste von Entwürfen hinterließe.

**Verworfen**: Senden direkt aus den Eingaben ohne Entwurf (keine Prüfung durch den Nutzer in der
Zustimmungsabfrage über den Entwurf hinaus); Host gibt `APPENDUID` zurück (neue Host-Fähigkeit ohne
Bedarf, Suche über Message-ID genügt).

## R9a: Zustimmung zeigt nur die Eingabe

**Entscheidung**: Die Zustimmungsabfrage nennt Erweiterung, Titel und Eingabe (018 FR-011), nicht das
Ergebnis eines früheren Aufrufs. Die zerstörenden Werkzeuge tragen deshalb in der Eingabe, was der Nutzer
sehen muss: `send_mail` Empfänger, Betreff und Text; `delete_messages` Kennung und Betreff je Mail;
`add_rule` Name, Bedingungen und Aktionen (schon strukturiert). Die Handler prüfen, dass die Angaben der
Wirklichkeit entsprechen, und handeln sonst nicht.

**Begründung**: Ein Werkzeug mit nackten Kennungen lässt die Abfrage leer wirken und macht die Zustimmung
bedeutungslos (032 FR-008). Die Prüfung verhindert, dass die Abfrage etwas anderes zeigt, als geschieht.

**Verworfen**: Eine Änderung an 018, die Abfrage um einen Werkzeug-Vorschautext zu erweitern (größerer
Umfang über beide Specs, und die Prüfung im Handler braucht ihn nicht).

## R10: Regeln, Auswertung und Zeitpunkt

**Entscheidung**: Reines Modul `app/lib/rules.ts`: Typen, Prüfung (FR-020), Klartext-Zusammenfassung,
Auswertung einer Mail, Plan der Aktionen. Bedingungen vergleichen ohne Beachtung der Schreibweise als
Teilzeichenkette; „Empfänger“ prüft `to` und `cc`; „Absender“ und „Empfänger“ vergleichen
`Name <adresse>` als Text. Alle passenden Regeln greifen; hat mehr als eine Regel die Aktion
„verschieben“, gewinnt die zuerst angelegte; Verschieben läuft zuletzt. Eine verschobene Mail durchläuft die
Regeln nicht erneut. Regeln für „alle Konten“ dürfen nur Rollen als Ziel haben, weil Ordnernamen je Konto
verschieden sind (die Prüfung verlangt das).

**Zeitpunkt (was ist „neu“)**: Je Postfach merkt sich haex-mail die höchste UID, bis zu der Regeln
gelaufen sind (`rulesUidThrough` auf `mailboxes`). Beim ersten Abruf eines Posteingangs wird der
Wert auf die höchste vorhandene UID gesetzt, ohne Regeln zu laufen (sonst würde jede neue Regel den
Bestand umsortieren). Danach laufen Regeln für UIDs oberhalb des Werts; der Wert rückt nach, und ein
`UIDVALIDITY`-Wechsel setzt ihn mit dem Cache zurück (`invalidateMailboxCacheAsync`). Aufruf an zwei
Stellen: im Watch-Handler (`stores/mail/watch.ts`) und im normalen Abruf (`refreshMessagesAsync`), jeweils
nach `persistEnvelopesAsync`, nur für INBOX.

**Begründung**: Sonst gäbe es keine Aussage, was „neu“ heißt (`persistEnvelopesAsync` upsertet ohne zu
unterscheiden). Der Wert liegt auf dem Postfach, das mit der Vault synchronisiert wird, so läuft eine Mail
nicht auf zwei Geräten doppelt durch; die Aktionen sind ohnehin idempotent.

**`applyToExisting`**: Die Regel läuft einmal über die 1000 neuesten Mails des Posteingangs (Obergrenze
`MAX_ENVELOPES`), lokal auf den geholten Umschlägen; die Antwort nennt die Zahl der betroffenen Mails und,
ob der Bestand größer war. Der Wert aus `rulesUidThrough` ändert sich dadurch nicht.

**Verworfen**: Regeln auf `persistEnvelopesAsync` selbst (läuft auch beim Anzeigen älterer Mails);
Vergleich über Regex (Nicht im Umfang).

## R11: Konto einrichten

**Entscheidung**: `list_accounts` liefert neben den Konten die übernehmbaren Passwortmanager-Einträge
(`listImportableVaultAccountsAsync`: nur Titel, Benutzername, Kennung, nie Geheimnisse; Obergrenze 20).
`import_account {passwordItemId}` ruft `importAccountAsync`, das Vollständigkeit prüft. `open_account_setup`
legt die Vorbelegung im `ui`-Store ab und navigiert zum Assistenten; das Passwortfeld bleibt leer.
Die Erweiterung läuft meist im Hintergrund; das Ergebnis des Werkzeugs sagt dem Agenten, dass der Assistent
bereitsteht und das Fenster geöffnet werden muss (der Agent hat dafür die Aktion `wm.apps.open`,
`src/lib/actions/wmActions.ts`).

**Begründung**: Der Passwortmanager bleibt die einzige Stelle mit Geheimnissen (FR-019, SC-006). Ein
eigener Holen-Weg für Einträge würde das Modell zu Kennungen führen, die es sich sonst erfinden müsste.

## R12: Manifest, Beispielsätze und Messung

**Entscheidung**: Alle 15 Werkzeuge im Manifest nach `specs/018-extension-agent-tools/contracts/manifest-tools.md`.
Das passende Angebot liefert höchstens drei Erweiterungswerkzeuge je Nachricht (018 R6), und die Auswahl
beruht auf den Beispielsätzen. Deshalb bekommt jedes Werkzeug mindestens vier deutsche und vier
englische Beispielsätze, die sich zwischen Werkzeugen mit ähnlichem Zweck unterscheiden
(`find_messages` gegen `list_mailboxes`, `move_messages` gegen `add_rule`). Die Modellprüfung bekommt
mindestens 24 Sätze (`src-tauri/src/chat/eval/tools.json`); SC-001 und SC-002 messen mit
`cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture`.

**Risiko**: 15 Werkzeuge einer Erweiterung gegen ein Angebot von drei. Zeigt die Messung, dass
Zweischritt-Aufgaben („suche und verschiebe“) scheitern, ist das ein Befund für Spec 018 (Angebot nach
Erweiterung statt nach Einzelwerkzeug), nicht für diese Spec.

## R13: Test und Prüfung

**Entscheidung**:

- holzi: neue Fälle in `extensions/mail/mail_tests.rs` gegen den erweiterten Testserver (`test_server.rs`
  lernt `SEARCH` mit Kriterien, `CREATE`, `UID STORE \Deleted`, `UID EXPUNGE`), Eintrag in
  `bridge/allowlist.rs` samt dortigem Test, Zeilen in `specs/017-extension-host/contracts/bridge.md`.
- vault-sdk: `src/api/__tests__/mail.test.ts` für die drei Methoden; Bundle- und
  Transkript-Tests bleiben.
- haex-mail (vitest): `lib/rules.test.ts` (Bedingungen, Aktionen, Konflikte, fehlendes Ziel, alle Konten),
  `tools/*.test.ts` mit gespieltem Client (Schema, Kürzung, kein Passwort in Ergebnissen), ein Test, der
  `manifest.tools` gegen die Formatregeln und gegen die registrierten Handler abgleicht (jedes erklärte
  Werkzeug hat einen Handler und umgekehrt).
- SC-005 (nie ohne Zustimmung): Test in holzi mit der Test-Erweiterung und dem Testserver in allen drei
  Modi, für `send_mail`, `delete_messages` und `add_rule` (alle drei `destructive`).

## R14: Graphify-Abfrage vor neuen Artefakten

Vor dem Entwurf neuer benannter Artefakte wurden die Graphen beider Repositories abgefragt
(`graphify query` in holzi und haextension; die Graphen sind Schnappschüsse der Hauptcheckouts).

| Neues Artefakt                                                        | Abgefragt                                  | Ergebnis                                                                                                       | Entscheidung                                                                                                                                                                                         |
| --------------------------------------------------------------------- | ------------------------------------------ | -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `imap::search`, `create_mailbox`, `delete_messages` (holzi)           | IMAP-Suche in der Mail-Erweiterung         | Kein Kandidat; `imap.rs` hat nur `uid_search` als Hilfe für `FetchRange` (`uid_set`), keinen Weg mit Kriterien | neu, in `imap.rs` neben `move_messages` (selbes Anmelden/Abmelden-Muster `with_session!`, selbe Prüfungen `check_mailbox`)                                                                           |
| `lib/rules.ts`, `stores/rules.ts`, `stores/mail/rules.ts` (haex-mail) | Regeln für eingehende Mails                | Kein Kandidat; es gibt nur Schema, Manifest und `AccountImportList.vue`                                        | neu; nutzt vorhandene `senderText`, `isMessageUnread`, `isMessageFlagged` (`lib/mail.ts`) und `assignRoles` (`lib/mailboxRoles.ts`) statt Parallelversionen                                          |
| `app/tools/*`, `plugins/agent-tools.client.ts`                        | Registrierung von Werkzeug-Handlern        | Kein Kandidat (haex-notes, haex-files haben ebenfalls keine)                                                   | neu; die Handler rufen Stores und `buildReplyContextAsync`, `importAccountAsync`, `htmlToText`, `listImportableVaultAccountsAsync` auf                                                               |
| `RulesSection.vue`, `RuleDialog.vue`                                  | Listen- und Dialogmuster der Einstellungen | Muster in `pages/settings.vue` und `AccountImportList.vue`                                                     | neu, nach dem Muster der Konten; kein gemeinsamer Baustein, weil Bedingungs- und Aktionszeilen nur hier vorkommen (Constitution: keine Abstraktion für zwei ähnliche Dinge ohne gleiche Invarianten) |

Die Zeile `Regeln anwenden, ohne die Oberfläche neu zu laden` ist **grenzwertig**: `bulkSetFlagAsync` und
`bulkMoveToMailboxAsync` (`stores/mail/bulk.ts`) tun genau das Nötige, laden aber die sichtbare Liste neu. Der
Plan verlangt, in E3 zuerst eine Variante ohne Neuladen aus diesen Funktionen herauszuziehen (Erweiterung,
kein Parallelcode) und den Vorschlag dem Betreiber mit `file:line` vorzulegen, bevor sie geändert werden.
