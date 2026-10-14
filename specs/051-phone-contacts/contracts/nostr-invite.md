# Einladung aus dem Adressbuch: Nostr-Ereignisse (US7)

Wird mit Spec 027 oder 028 umgesetzt; hier festgelegt, damit deren Pläne darauf bauen. Grundlage: ADR
0014 (löst D33 des Sync-Entwurfs ab).

## Schichten (NIP-59)

| Schicht   | kind                    | unterschrieben von            | verschlüsselt an                           |
| --------- | ----------------------- | ----------------------------- | ------------------------------------------ |
| Gift Wrap | 1059 (gespeichert)      | Einmalschlüssel               | Geräteschlüssel des Empfängers (NIP-44 v2) |
| Seal      | 13                      | Geräteschlüssel des Absenders | Geräteschlüssel des Empfängers (NIP-44 v2) |
| Rumor     | 24110 (holzi-Einladung) | – (nicht unterschrieben)      | –                                          |

Tags des Gift Wraps: `["p", <Geräteschlüssel Empfänger hex>]`, `["expiration", <Ablauf der Einladung>]`
(NIP-40). `created_at` zufällig bis 48 h zurück (NIP-59). Größe ≤ 16 KiB wie `presence::MAX_EVENT`.

## Inhalt des Rumors (JSON)

```json
{
  "v": 1,
  "type": "space" | "data-share",
  "invite": "<Einladungslink-Text aus 027 FR-008 / 028>",
  "recipient": "<Vault-Identität Empfänger hex>",
  "senderCard": "<holzi:card:…>"
}
```

Das Einmalgeheimnis steckt im Einladungslink; es gilt nur für den genannten Empfänger (FR-034).

## Versand

- Je Gerät der zuletzt bekannten Karte des Empfängers ein Gift Wrap.
- An alle Nostr-Relays der Karte des Empfängers; zusätzlich an die eigenen, falls die Karte keine
  nennt.
- Stand „zugestellt“, sobald eine Annahme oder eine Empfangsbestätigung (Rumor 24111 auf demselben Weg
  zurück, an die Geräte aus `senderCard`) eintrifft; „nicht zustellbar“, wenn kein Relay das Ereignis
  annimmt oder alle Geräte der Karte laut neuerer Karte entfernt sind.

## Empfang

- Jedes Gerät abonniert `{"kinds":[1059], "#p":[<eigener Geräteschlüssel>]}` auf den eigenen
  Nostr-Relays, ab dem letzten gesehenen `created_at` minus 48 h.
- Prüfung: Seal-Schlüssel steht in `devices` der `senderCard`; `senderCard` gültig; `recipient` ist die
  eigene Vault; Einladungslink gültig (027 FR-008). Sonst still verwerfen.
- Absender blockiert → still verwerfen, keine Bestätigung (FR-039).
- Absender nicht mit bestätigter Karte im Adressbuch → Bereich „Von Unbekannten“ (FR-038).
- Die Einladung wird als Vault-Daten gespeichert, damit alle eigenen Geräte sie zeigen (027 FR-010).
