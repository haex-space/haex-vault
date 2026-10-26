# Datenmodell: Kontakte teilen und der Kontakt „Ich“ (Spec 060)

Alle Tabellen bleiben CRDT-getrackt (kein `_no_sync`), ohne UNIQUE (Regel am Kopf von
`identity/migrations_contacts.rs`). Rust-Typen, die die Oberfläche sieht, gehen mit `ts-rs` nach
`src/types/bindings/`. Neue `#[ts(export)]`-Typen tragen das Präfix `Contacts…`/`ContactShare…`
(ein gleicher Name überschreibt still ein fremdes Binding).

## 1. Tabellen

### `haex_contacts` (Migration `0042_contacts_self`)

Keine neue Spalte. Eine feste Zeile:

| Spalte                     | Wert                                                                              |
| -------------------------- | --------------------------------------------------------------------------------- |
| `id`                       | `SELF_CONTACT_ID` (`contacts/ids.rs`, eine einmal erzeugte UUID, **nie ändern**)  |
| `origin_kind`              | `holzi`                                                                           |
| `origin_device`            | `00000000-0000-0000-0000-000000000000` (Vault-Scope-Kennung, kein Fremdschlüssel) |
| `origin_caller`            | `internal:self`                                                                   |
| `created_at`, `updated_at` | fester Zeitpunkt im Format von `passwords::clock`                                 |
| übrige Spalten             | `NULL`                                                                            |

Der INSERT enthält keine HLC-Spalten; `haex-crdt` stempelt die Zeile beim nächsten Öffnen. Jedes Gerät
führt dieselbe Migration aus und schreibt dieselben Werte.

Ableitungen im Code, keine Spalten: `Contact.is_self` und `ContactHeader.is_self` =
`id == SELF_CONTACT_ID`.

### `haex_contacts_cards` (Migration `0043_contacts_cards_hash`, ersetzt 0033)

```sql
DROP TABLE haex_contacts_cards;
CREATE TABLE haex_contacts_cards (
  hash TEXT PRIMARY KEY NOT NULL,      -- SHA-256 (hex) des unterschriebenen Inhalts der Karte
  vault_pubkey TEXT NOT NULL,          -- hex; muss der Vault in der Karte entsprechen
  contact_id TEXT NOT NULL REFERENCES haex_contacts(id) ON DELETE CASCADE,
  card BLOB NOT NULL,                  -- len ‖ postcard ‖ Signatur, unverändert
  issued_at TEXT NOT NULL,
  received_at TEXT NOT NULL
);
CREATE INDEX haex_contacts_cards_vault ON haex_contacts_cards(vault_pubkey);
CREATE INDEX haex_contacts_cards_contact ON haex_contacts_cards(contact_id);
```

- `hash` = `sha256(payload)`, `payload` = die kanonischen Bytes der Karte ohne Signatur (`card/format.rs`).
  holzi berechnet ihn selbst und überträgt ihn nie. Dieselbe Karte ergibt auf jedem Gerät dieselbe Zeile.
- Mehrere Zeilen je Vault sind erlaubt (mehrere Karten derselben Vault). Alle Zeilen einer Vault gehören
  zu **einem** Kontakt (Spec 051 FR-013); das prüft der Dienst, nicht ein UNIQUE.
- Prüfung beim Lesen (`received::verified`): Format, Grenzen und Unterschrift (`format::from_bytes`),
  `hash == sha256(payload)`, `vault_pubkey == hex(card.vault)`. Eine Zeile, die das nicht besteht, zählt
  nicht (FR-025a).
- Keine Zeile wird kopiert (R5): Beim Update verschwinden gespeicherte bestätigte Adressen und kommen mit
  dem erneuten Einlesen zurück.
- `HOLZI_TRIGGER_VERSION` 27 → 28.

### `haex_contacts_own_card`

Unverändert. `own_card::refresh` schreibt sie weiter (FR-028).

### Vault-Einstellungen

`contacts.card.own_contact` und `contacts.card.values` entfallen; kein Löschen vorhandener Werte nötig
(sie werden nicht mehr gelesen, holzi hat keine Nutzer).

## 2. Typen der Weitergabe (Rust, `src-tauri/src/contacts/share/`)

Nicht gespeichert, nicht synchronisiert. Format: `contracts/share-format.md`.

```rust
pub struct ShareEnvelope { version: u8, name: String, name_parts: NameParts, head: ShareHead,
                           values: Vec<ShareValue>, card: Option<Vec<u8>> }

// Auswahl im Teilen-Dialog (kommt von der Oberfläche oder vom Agenten)
pub struct ShareSelection {
    pub values: Vec<String>,      // Kennungen von haex_contacts_values.id des Kontakts
    pub organization: bool, pub job_title: bool, pub note: bool,
    pub holzi: HolziSelection,
}
pub enum HolziSelection { None, Card, Own { devices: Vec<String>, relays: Vec<String> } }
```

`devices` sind `device_pubkey` (hex); `relays` die Nostr-Relay-URLs der Karte. Der Dienst prüft jede
Kennung gegen den Kontakt und jede Gerätekennung gegen die wirksame Geräteliste.

### Optionen für den Dialog (`ContactShareOptions`)

```text
rows:  [ShareRow { id, group, kind, label, value }]   // alle vorgewählt
head:  { organization?, jobTitle?, note? }             // Zeilen, wenn belegt
holzi: None
     | Card { npub, devices: n, relays: n, issuedAt }  // fremder Kontakt mit Karte
     | Own  { canSign: bool, npub, devices: [ShareDevice], relays: [string] }
name:  string                                          // wie ihn der Empfänger sähe (FR-011)
canShare: bool                                         // false: nichts Weiterzugebendes (FR-017)
```

### Ausgabe (`ContactShareOutput`)

```text
link: string           // der Text (gleich für Link, Zwischenablage, QR)
qrSvg?: string         // nur wenn link.len() <= 2048
size: number
```

Fehler: `ShareTooLarge` (über 16 384) statt eines abgeschnittenen Textes.

### Vorschau und Entscheidungen (`ContactSharePreview`)

```text
name, npub?, nameIsNpub: bool, vaultName: string        // Vault, in die gespeichert würde
target: New | Contact { id, name }                      // vorgewählt oder gewählt
targets: [ShareTarget { id, name, reason: Vault | Hint }]  // Hinweise nach Telefon/E-Mail
identity: None | New | Matches | Mismatch { existingNpub }
items: [ShareItem { id: u32, kind, label, value, state, choices: [Choice], default: Choice,
                    existing: [ExistingValue] }]
reachable: bool                                         // false: ohne holzi-Adressen (FR-017a)
skipped: u32                                            // übergangene ungültige Angaben
blocked: bool
```

- `ShareItem.state`: `New`, `Same` (schon da: keine Wahl), `Conflict` (einfeldig belegt, Wert
  weicht ab).
- Posten-Arten: `NamePart(field)`, `Organization`, `JobTitle`, `Note`, `Value`, `HolziAddresses`.
- `Choice`: `Add`, `Update`, `Skip`, `Keep`, `Replace` (nur alte Karte). Je Art nur erlaubte:
  `Value`: Add/Update/Skip; einfeldig: Keep/Update; `HolziAddresses`: Add/Skip (+ Replace).
- `Decision { item: u32, choice: Choice, value_id?: String }`; `value_id` nur bei `Update` einer Angabe.
- `ExistingValue { id, kind, label, value }`: die Werte derselben Art am Kontakt, aus denen der Nutzer bei
  `Update` einen wählt.

### Aufruf der Aktion für Agenten (`ContactShareConfirmRequest`)

```text
requestId, agentId, contactId, contactName
proposed: ShareSelection      // Vorschlag des Agenten; Standard: alles
options: ContactShareOptions
```

Antwort: `Option<ShareSelection>`; `None` = abgelehnt. Der Agent bekommt `ContactShareOutput` nur zur
bestätigten Auswahl; sonst `denied_by_user`.

## 3. Zustände und Regeln

| Regel                                                                | Quelle            | Stelle                                     |
| -------------------------------------------------------------------- | ----------------- | ------------------------------------------ |
| Ich-Kontakt hat feste Kennung, nicht löschbar                        | FR-001, FR-002    | `service/contacts.rs`                      |
| `display_name` des Ich-Kontakts gesperrt                             | FR-003            | `service/contacts.rs`                      |
| Ich-Kontakt leer erlaubt, ohne Abgleichsziele                        | FR-003, FR-006    | `check_identity`, `service/targets.rs`     |
| Name beim Teilen: Namensteile → npub → leer                          | FR-011            | `share::name_for`                          |
| Eigene Karte: frisch unterschrieben, nur gewählte Geräte/Relays      | FR-012            | `own_card::build_for_share`                |
| Fremde Karte: neueste gespeicherte, unverändert, ganz oder gar nicht | FR-013            | `share::build`                             |
| Weitergabe entfernt/ersetzt nichts                                   | FR-024a, FR-025   | `share::merge`                             |
| Identität nie ersetzt                                                | FR-025            | `share::merge`                             |
| Karte wird geprüft (Unterschrift, Hash, Vault)                       | FR-025a           | `received::verified`, `format::from_bytes` |
| Agent: jeder Aufruf bestätigt                                        | FR-029 bis FR-032 | `ContactsShareAgent`                       |

## 4. Ordnung der „neuesten“ Karte

`received::stored(vault)`: größte `list_generation`, bei Gleichstand größtes `issued_at`, bei
Gleichstand kleinster `hash` (stabil auf allen Geräten). Diese Karte bestimmt den Schwellwert in
`foreign_trust` und den Namen (`ContactCardInfo.name`). `received::addresses(vault)`: Vereinigung der
Geräte (nach `endpoint_id`, mit dem `iroh_relay` der Karte mit der größten Generation) und Relays
(Menge) aller geprüften Karten.
