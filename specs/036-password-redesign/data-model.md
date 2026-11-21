# Datenmodell: Passwortmanager-Redesign

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Aufbauend auf dem Datenmodell von [034](../034-password-manager/data-model.md): Es gibt **keine
Änderung an vorhandenen Tabellen** (keine neue Spalte, kein neuer Index auf alten Tabellen).
Verweise sind Text in vorhandenen Spalten (R3); neu ist eine CRDT-Tabelle. Sie entsteht
in Migration `0026_passwords_refs` (geplant als `0024`; Spec 017 hat `0024` und `0025` zuerst belegt) (SQL in `src-tauri/src/identity/migrations_passwords_refs.rs`,
weil `identity/migrations.rs` schon über 500 Zeilen steht), ist CRDT-synchronisiert (kein
`_no_sync`-Suffix), gehört der Vault, hat **keine UNIQUE-Constraints** (ein Konflikt hält
den Sync an; Eindeutigkeit über abgeleitete Kennungen, wie in 034 R2) und wird beim
endgültigen Löschen eines Eintrags oder Passkeys **ausdrücklich zuerst** gelöscht
(`trash::purge_item`; der Sync wendet entfernte Löschungen ohne Fremdschlüssel an).
`HAEX_TRIGGER_VERSION` 15 → 16 (wie bei 0022 und 0023, damit der CRDT-Trigger der neuen
Tabelle angelegt wird); eine Zeile im Versionsverlauf der Konstante.

## Neue Tabelle

Die WebAuthn-Signaturzähler bleiben für diesen synchronisierten Dienst immer 0. Eine
inkrementierende CRDT-Tabelle würde bei zwei offline bestätigenden Geräten denselben
Nicht-Null-Wert erzeugen; das wäre für Gegenstellen nicht von einem Clone oder einer Race
Condition zu unterscheiden. `passkeys.sign_count` bleibt als importierter Bestandswert erhalten,
wird aber für neue Bestätigungen nicht erhöht.

### `haex_passwords_passkey_links` — Passkey per Verweis (R6)

| Spalte       | Typ           | Regel                                                                          |
| ------------ | ------------- | ------------------------------------------------------------------------------ |
| `id`         | TEXT PK       | UUIDv5 (`NS_PASSKEY_LINK`) aus `item_id` + `:` + `passkey_id`                  |
| `item_id`    | TEXT NOT NULL | **Ziel**: Eintrag, der den Passkey eines anderen zeigt; FK `ON DELETE CASCADE` |
| `passkey_id` | TEXT NOT NULL | **Quelle**: der verwiesene Passkey; FK `ON DELETE CASCADE`                     |
| `created_at` | TEXT          | `CURRENT_TIMESTAMP`                                                            |

Indizes: `idx_haex_passwords_passkey_links_item_id (item_id)`,
`idx_haex_passwords_passkey_links_passkey_id (passkey_id)`.

Regeln: Ein Passkey hängt nie über eine Verbindung an dem Eintrag, dem er ohnehin gehört
(`passkeys.item_id == item_id`: abgelehnt). Verbindungen auf Verbindungen gibt es nicht
(die Quelle ist immer ein Passkey mit `item_id`). Legen zwei Geräte dieselbe Verbindung an,
ergibt die abgeleitete Kennung dieselbe Zeile; doppelte Zeilen gibt es nicht.

## Verweise im Text (keine Tabelle)

Platzhalter stehen in `item_details.username`, `.password`, `.url`, `.note` und
`item_key_values.value`; die Grammatik in [contracts/references.md](./contracts/references.md).
Kein Wert wird aufgelöst gespeichert, auch nicht in `item_snapshots.data` (FR-049).
Die Zahl der Verweise auf einen Eintrag ist abgeleitet (R12), nicht gespeichert.

## Nicht geändert, aber betroffen

- `haex_passwords_passkeys`: `item_id` bleibt nullbar (alte Daten); ein Passkey ohne Eintrag
  zählt für alle Aufrufer außer dem Nutzer als nicht vorhanden (Spec, Randfall) und wird in
  der Oberfläche nicht angezeigt. haex-vault legt keinen an; der Import legt immer einen Eintrag an
  (034 FR-023).
- `trash::purge_item` löscht zusätzlich `passkey_links` (als Quelle und als Ziel) **vor** den
  Passkeys.
- `restore` aus dem Papierkorb hebt nichts auf: Verbindungen und Platzhalter sind nicht an den
  Ort gebunden.

## Zustand im Frontend (nichts gespeichert)

| Zustand        | Ort                            | Inhalt                                                                                                                  |
| -------------- | ------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| Ablage         | `stores/passwordsClipboard.ts` | `{ targets: Target[], mode: 'cut' \| 'copy' } \| null`; nur Kennungen; leer bei `reset`, letztem Fenster, Vault-Wechsel |
| Auswahl        | `stores/passwordsSelection.ts` | unverändert (034); endet beim Ordnerwechsel und Öffnen eines Eintrags                                                   |
| Tab            | Ort des Fenster-Tabs (`?tab=`) | `details` \| `extra`; `entry/:id/history` = Verlauf                                                                     |
| Kopier-Dialog  | `CopyDialog.vue` (lokal)       | Titel/Zusatz, `history`, `usernameAsReference`, `passwordAsReference`, `passkeysAsLinks`                                |
| Vorschaubilder | `lib/passwords/thumbnails.ts`  | LRU 200, Blob-URLs nach Prüfsumme; freigegeben, wenn das letzte Passwort-Fenster schließt                               |

## Typänderungen (ts-rs nach `src/types/bindings/`)

- `PasskeyView` bekommt `itemId`, `isDiscoverable`, `signCount` (für neue Bestätigungen immer 0),
  `linkedFrom: { itemId, title } | null` (gesetzt bei Verbindung, dann ohne Umbenennen und
  Löschen, nur „Verweis lösen“).
- `ItemDetail` bekommt `passwordReferences`, `usernameReferences`, `urlReferences`,
  `noteReferences` (je `RefMark[]`) und `keyValues[].references` — nur Zustand und Quellen,
  keine Werte; `passkeys` enthält zusätzlich die per Verbindung verwiesenen.
- `RefMark { start, end, sourceItemId, sourceTitle: string | null, kind: 'username' | 'password' | 'extra', key: string | null, status: 'ok' | 'missing' | 'cycle' | 'tooDeep' }`
  (Positionen in Zeichen des **Rohtexts**).
- `ItemHeader.hasPassword` zählt einen Verweis im Passwort als Passwort.
- `ImportReport` bekommt `referencesConverted` und `referencesLeftAsText`.
- Neue Typen: `CopyOptions`, `CopyReport`, `ReferenceUsage`, `PasskeyHeader`,
  `PasskeyCreateRequest/Response`, `PasskeyConfirmRequest/Response` (die letzten vier nur
  im Dienst; ts-rs nur, wenn ein Command sie braucht — heute keiner).
