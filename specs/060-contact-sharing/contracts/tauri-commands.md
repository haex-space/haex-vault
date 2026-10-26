# Vertrag: Befehle, Ereignisse und Aktion (Spec 060)

Ändert `specs/051-phone-contacts/contracts/tauri-commands.md` (§ Visitenkarten) und fügt die Aktion
`contacts.share` hinzu. Alle Befehle sind nur für das eigene Fenster (`Caller::User`); sie prüfen das im
Dienst (`require_user`). Fehler sind `HolziError` mit den Codes aus `contracts/share-format.md`.

## Entfällt

`contacts_own_card`, `contacts_own_card_set`, `contacts_card_preview`, `contacts_card_accept` und ihre
Typen (`ContactsOwnCard`, `ContactsOwnCardSetArgs`, `ContactsCardTextArgs`, `ContactsCardAcceptArgs`,
`ContactsCardPreview`, `ContactsCardInto`). Die Befehle `contacts_blocked`, `contacts_block`,
`contacts_unblock` bleiben.

## Befehle

| Befehl                         | Argumente                                       | Antwort               | Zweck                                                                |
| ------------------------------ | ----------------------------------------------- | --------------------- | -------------------------------------------------------------------- |
| `contacts_share_options`       | `{ contactId }`                                 | `ContactShareOptions` | Zeilen des Teilen-Dialogs, alles vorgewählt (FR-009, FR-012, FR-013) |
| `contacts_share_build`         | `{ contactId, selection }`                      | `ContactShareOutput`  | Text, QR, Größe (FR-018, FR-019)                                     |
| `contacts_share_preview`       | `{ text, target? }`                             | `ContactSharePreview` | Prüfen und Vorschau, nichts gespeichert (FR-021, FR-021a)            |
| `contacts_share_accept`        | `{ text, target, decisions, previewRevision? }` | `Contact`             | Speichern nach den Wahlen (FR-024 bis FR-025a)                       |
| `contact_share_confirm_answer` | `{ requestId, selection? }`                     | –                     | Antwort auf die Abfrage eines Agenten (FR-030, FR-031)               |

- `target`: `{ kind: 'new' }` oder `{ kind: 'contact', id }`. Fehlt es in der Vorschau, wählt der Dienst nach
  FR-024 (Vault vorgewählt, sonst `new`). Der Ich-Kontakt als Ziel: `ShareInvalid`.
- Bei einem vorhandenen Ziel liefert die Vorschau `targetRevision`. `contacts_share_accept` MUSS genau diese
  Revision als `previewRevision` zurückbekommen. Hat sich der Kontakt seit der Vorschau geändert, bricht der
  Dienst mit `SharePreviewStale` und einer frischen Vorschau ab; es wird nichts geschrieben. Für ein neues
  Ziel ist keine Revision nötig.
- `decisions`: ein Eintrag je Posten, der eine Wahl braucht; fehlende Einträge heißen Standard. Wahlen,
  die der Posten nicht erlaubt, und `Update` auf eine Angabe, die nicht zu dem Kontakt gehört:
  `ShareInvalid`, nichts wird geschrieben.
- Die Mitgliedschaft eines `value_id` im Zielkontakt bleibt eine eigene Prüfung und wird nicht durch die
  Revisionsprüfung ersetzt.
- `contacts_share_accept` schreibt in **einer** Transaktion und rechnet Posten, Karte und Ziel aus dem
  Text neu; die Revision schützt dabei die Werte, die der Nutzer in der Vorschau gesehen hat.

## Ereignis

| Ereignis                        | Nutzlast                     | Wann                                                                                     |
| ------------------------------- | ---------------------------- | ---------------------------------------------------------------------------------------- |
| `contact-share-confirm-request` | `ContactShareConfirmRequest` | ein Agent ruft `contacts.share` auf; das Fenster zeigt den Teilen-Dialog zur Bestätigung |
| `deep-link-received`            | URL als Text                 | ein weiterer Prozess hat einen Link aus dem Posteingang abgegeben (Desktop)              |

## Aktion für Agenten

```ts
{
  id: 'contacts.share',
  titleKey: 'actions.contacts.share',
  scope: 'contacts.read',          // Voraussetzung: Freigabe „Kontakte lesen“ (FR-029)
  effect: 'destructive',           // Daten verlassen die Vault (FR-029)
  agentCallable: true,
  runner: 'native',                // eigener Ausführer, siehe unten
  input: { contactId: string, selection?: ShareSelectionInput },  // Vorschlag, darf nur einschränken
  result: { link: string, qrAvailable: boolean, size: number },
}
```

Ablauf des Ausführers `ContactsShareAgent` (Rust, `contacts/agent_share.rs`):

1. `ContactsAgent::grants(Read)` (fragt wie bisher „Einmal erlauben / Immer erlauben / Ablehnen“, das
   betrifft nur **Lesen**), Kontakt über den Dienst laden, `hidden()` (keine Auskunft, ob er existiert).
2. `Auswahl` des Agenten gegen `contacts_share_options` einschränken (nie erweitern, FR-031).
3. `ContactShareConfirm::ask(request)` (`PromptPark`, 120 s, Abbruch des Zuges bricht ab). Jedes Mal, ohne
   Merken, in jedem Modus außer „Plan“ (dort lehnt der Chat `Risky` weiter ab).
4. Bestätigt: Weitergabe aus der **bestätigten** Auswahl bauen (`contacts_share_build`-Logik), Ergebnis
   zurück. Abgelehnt, abgebrochen oder abgelaufen: `NativeError::new("denied_by_user")` ohne Inhalt
   (FR-032).

`Tool::confirms_itself() -> bool` (neu, Vorgabe `false`; `true` für diese Aktion): `tool_round` überspringt
die allgemeine Abfrage `tool-permission-request` für Manuell und Automatisch und behält die Ablehnung im
Modus Plan. Es gibt dann genau eine Abfrage, die des Teilen-Dialogs.

Für Erweiterungen gibt es keine Schnittstelle (Spec 051 FR-029).

## Agent-Lesen: `isSelf`

`contacts_agent_search` und `contacts_agent_get` liefern `ContactHeader`/`Contact` mit dem neuen Feld
`isSelf` (FR-003a); `name` bleibt der abgeleitete Name aus den Namensteilen (ohne das Wort „Ich“). Die
Beschreibung der Aktionen in `contactsActions.ts` nennt das Feld, damit ein Agent es nutzt.

## Frontend

- `src/composables/useContactShare.ts`: dünne `invoke`-Hüllen für die vier Befehle.
- `src/components/contacts/ShareDialog.vue`: der Teilen-Dialog (Zeilen mit Checkboxen, „Alle/Keine“,
  Ergebnisbereich mit Link, QR, Kopieren). Zwei Betriebsarten: Teilen (Nutzer) und Bestätigen (Agent;
  Vorschlag vorgewählt, Buttons „Freigeben/Ablehnen“).
- `src/components/contacts/ShareImport.vue` (ersetzt `CardImport.vue`): Einfügen, Kamera, Vorschau,
  Posten mit Wahlen, Ziel.
- `src/components/contacts/ShareConfirmDialog.vue`: hört auf `contact-share-confirm-request`, hostet
  `ShareDialog` im Bestätigen-Modus, gemountet in `src/pages/workspace/[instance].vue` neben
  `FilesShareConfirmDialog`.
- `src/composables/useDeepLink.ts`, `src/stores/shareInbox.ts`: URL aus dem Betriebssystem, Zwischenspeicher
  bis zur offenen Vault, Übergabe an `/import`.
- Entfernt: `OwnCard.vue`, `ContactsCardSetting.vue`, `CardImport.vue`, Route `/card`; Toolbar und Menü
  verweisen auf „Teilen“ am Kontakt und `/import`.
- i18n: `contacts.self`, `contacts.share.*`, `contacts.import.*` in `de.json` und `en.json`; alte
  `contacts.card.*` und `settings.contacts.card.*` entfallen.

## Deep Link (Plattform)

- Schema `haex-vault://contact/<base64url>`; Konfiguration in `tauri.conf.json`
  (`plugins.deep-link`), `capabilities/default.json` und `mobile.json` (`deep-link:default`),
  `AndroidManifest.xml` (Intent-Filter), `lib.rs` (`.plugin(tauri_plugin_deep_link::init())`).
- Desktop-Weiterleitung zwischen Prozessen: Verzeichnis `<AppLocalData>/link-inbox/` (siehe `research.md` R9).
- Der Link öffnet **nur** die Vorschau: Er ruft `contacts_share_preview` auf, nie `contacts_share_accept`.
