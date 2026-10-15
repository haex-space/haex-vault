# Visitenkarte: Format

## Struktur (postcard, Feldreihenfolge ist Teil des Formats)

```rust
struct ContactCard {
    version: u8,                 // 1
    vault: [u8; 32],             // x-only Public Key der Vault-Identität
    issued_at: u64,              // Unix-Sekunden
    list_generation: u64,        // Generation der Geräteliste, aus der die Geräte stammen
    devices: Vec<CardDevice>,    // Hauptgeräte und verknüpfte Geräte der wirksamen Liste
    nostr_relays: Vec<String>,   // effective_nostr_relays() beim Erstellen
    name: String,                // Anzeigename der eigenen Vault
    values: Vec<CardValue>,      // vom Nutzer gewählte Angaben des eigenen Kontakts
}
struct CardDevice { device_pubkey: [u8; 32], endpoint_id: [u8; 32], iroh_relay: Option<String> }
struct CardValue { kind: String, label: String, value: String }
```

## Unterschrift und Text

- `sig = signing::sign(Domain::ContactCard, canonical(card), vault_secret)`, BIP-340 Schnorr über
  `SHA-256("holzi-contact-card/v1" ‖ bytes)`.
- Text: `holzi:card:` + base64url ohne Auffüllung von `len(u16 BE) ‖ postcard ‖ sig(64)`.
- QR-Code: derselbe Text, Fehlerkorrektur M.
- Höchstens 2.048 Bytes Text. Passt die Karte nicht, lässt holzi Angaben von hinten weg und meldet sie
  in `omitted`; Geräte und Nostr-Relays werden nie weggelassen (bei mehr als 2.048 Bytes nur mit
  Angaben ist die Karte nicht erstellbar, Fehler `CardTooLarge`).

## Prüfung beim Einlesen (FR-011)

1. Präfix, base64url, Länge, `version == 1`, postcard ohne Restbytes.
2. Unterschrift mit `vault` als Schlüssel; falsch → `CardInvalid`.
3. `vault` gleich der eigenen Vault-Identität → `CardOwn`.
4. Jede Relay-URL `wss://` oder `ws://`, höchstens 10 Relays, höchstens 32 Geräte, Strings ≤ 1 KiB.
5. Gibt es schon eine Karte zu `vault`: `issued_at` muss größer sein, sonst `CardOlder` (keine
   Änderung, FR-012).

Ein Hauptgerät erstellt eine neue Karte immer mit `issued_at` größer als dem der vorigen, auch wenn
zwei Karten in derselben Sekunde entstehen.

## Verwendung

- `haex_contacts_cards.card` und `haex_contacts_own_card.card` speichern den Text ohne Präfix dekodiert
  (`len ‖ postcard ‖ sig`), sodass dieselbe Prüfung für Text und gespeicherte Karte gilt.
- Für US7 nimmt der Absender die eigene Karte in die Einladung auf; der Empfänger prüft sie nach
  denselben Regeln und den Seal-Schlüssel gegen `devices`.
