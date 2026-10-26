# Vertrag: Die Weitergabe (`haex-vault://contact/…`)

Gilt für Spec 060 (FR-018 bis FR-020, FR-022). Ersetzt für das Teilen den Text der Visitenkarte aus
Spec 051 (`contracts/contact-card.md`); die Karte selbst bleibt unverändert und wird als Beleg
eingebettet.

## Text

```text
haex-vault://contact/<base64url(postcard(ShareEnvelope))>
```

- Schema `haex-vault://` (Spec 054: der Name der App), Host `contact`, danach der Inhalt ohne Padding.
- Ein Text für alle drei Ausgaben: Link, Zwischenablage, QR-Code (SC-009). Der QR-Code ist der Text als
  Byte-Modus, Fehlerkorrektur M, nur wenn `text.len() <= 2048` (FR-019).
- Obergrenze des Textes: `MAX_SHARE_TEXT = 16 384` Zeichen. Größer: der Dialog meldet „zu groß“ und
  nennt, was sich abwählen lässt; der Dienst gibt keinen abgeschnittenen Text aus.
- Einlesen nimmt außerdem `holzi:card:…` an (bisheriger Kartentext, FR-020). Intern wird er zu einem
  Umschlag ohne Angaben und mit der Karte; er behält Spec 051 FR-012 (siehe unten „Alte Karte“).
  Spec 054 benennt das Präfix `holzi:card:` später um; das Schema des neuen Formats trägt den neuen
  Namen von Anfang an.

## Umschlag

```rust
struct ShareEnvelope {
    version: u8,              // 1; jede andere Zahl wird abgewiesen (FR-022)
    name: String,             // FR-011: Namensteile, sonst npub, sonst ""
    name_parts: NameParts,    // vorhandene Namensteile, einzeln (für die Wahl je Feld, FR-024c)
    head: ShareHead,          // Anzeigename, Spitzname, Organisation, Position, Notiz, soweit gewählt
    values: Vec<ShareValue>,  // gewählte Angaben, in der Reihenfolge des Kontakts
    card: Option<Vec<u8>>,    // eine Karte nach Spec 051 (len ‖ postcard ‖ Signatur), unverändert
}

struct NameParts { prefix, given, middle, family, suffix: Option<String> }
struct ShareHead { display_name, nickname, organization, job_title, note: Option<String> }
struct ShareValue { kind: String, label: String, value: String }
```

Der Umschlag selbst trägt **keine** Unterschrift. Seine Angaben haben keinen Echtheitsnachweis
(Annahmen der Spec). Nur `card` ist beweisend, und nur für die holzi-Adressen.

Nicht im Umschlag: Foto, Gruppen, Abgleichsziele, Herkunft, Abgleichsstand (FR-026), Kennungen der
Angaben, Positionen.

## Grenzen (beim Bauen und beim Lesen)

| Grenze                                                      | Wert                                                                                                                                                        | Herkunft                       |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| Angaben je Umschlag                                         | 200                                                                                                                                                         | neu                            |
| Zeichen je Angabe (`value`)                                 | `kinds::MAX_VALUE_CHARS` (4 096)                                                                                                                            | `kinds.rs`                     |
| Zeichen `kind`, `label`, Namensteil, Anzeigename, Spitzname, Organisation, Position | 1 024                                                                                                                                                       | wie `card::format::MAX_STRING` |
| Zeichen Notiz                                               | 4 096                                                                                                                                                       | wie `MAX_VALUE_CHARS`          |
| Karte                                                       | Grenzen von `card::format::check_limits` (32 Geräte, 10 Relays, 1 KiB je Text); die 2 048 Zeichen von `MAX_TEXT` gelten für die Karte im Umschlag **nicht** | `format.rs`                    |
| Gesamttext                                                  | 16 384                                                                                                                                                      | neu                            |

Beim Lesen: Angaben bekannter Art laufen durch `kinds::validate`; eine ungültige wird mit einem Hinweis
übergangen (nicht abgewiesen) und zählt in der Vorschau als „übergangen“. Eine Angabe unbekannter Art
bleibt unverändert und kommt schreibgeschützt an (Spec 051 FR-007). Der Umschlag als Ganzes wird nur
bei Formfehlern abgewiesen: falsches Schema oder Präfix, kein Base64url, übrige Bytes nach dem
Umschlag, unbekannte Version, Grenzen verletzt, ungültige Karte.

## Karte im Umschlag

- Lesen: `format::from_bytes` prüft Format, Grenzen und Unterschrift gegen `card.vault`. Scheitert das,
  ist der **ganze** Umschlag ungültig (FR-022, FR-025a).
- Die eigene Vault: Karte mit `vault` = eigene Identität wird abgewiesen (`CardOwn`).
- Aus der Karte werden nur `vault`, `devices` und `nostr_relays` verwendet. `card.values` und
  `card.name` bleiben unbenutzt (FR-023).
- Eigener Kontakt (Bauen): holzi unterschreibt pro Teilen eine frische Karte mit nur den gewählten
  Geräten und Relays, `values` leer, `name` wie im Umschlag; `issued_at` = jetzt, `list_generation` wie
  die wirksame Geräteliste. Kein Speichern der frischen Karte.
- Fremder Kontakt (Bauen): die neueste gespeicherte Karte der Vault des Kontakts, Bytes unverändert.
  Der Dialog zeigt dazu die Zeile „holzi-Adressen (bestätigt)“ mit dem Hinweis aus FR-013.

## Alte Karte (`holzi:card:`)

Wird als Umschlag mit `values = []`, `name = card.name`, `card = Some(bytes)` gelesen. Die Vorschau
bietet für die holzi-Adressen eines vorhandenen Kontakts zusätzlich „Ersetzen“ an, das Spec 051 FR-012
umsetzt: eine neuere Karte ersetzt die gespeicherten Karten dieser Vault, eine ältere ändert nichts
(`CardOlder`). Bei einer Weitergabe im neuen Format gibt es „Ersetzen“ nie (FR-025). Die Angaben
(`card.values`) einer alten Karte übernimmt die Vorschau weiter als Angaben.

## Namensregel (FR-011)

1. Namensteile (Präfix, Vorname, weitere Namen, Nachname, Suffix), durch Leerzeichen verbunden;
2. sonst `npub1…` der Vault-Identität (eigener Kontakt: die eigene; fremder Kontakt: die der
   gespeicherten Karte);
3. sonst `""`.

Anzeigename, Spitzname, Organisation, Telefonnummer und E-Mail-Adresse sind kein Ersatz. Anzeigename und
Spitzname sind trotzdem eigenständige, einzeln auswählbare Angaben im Umschlag; sie werden nicht in `name`
eingebaut.

## Fehler

Alle Fehler des Einlesens sind `HolziError`-Varianten mit stabilen Codes für die Oberfläche:
`ShareInvalid { reason }` (Form, Version, Grenzen), `CardInvalid { reason }` (Unterschrift, Format der
Karte), `CardOwn`, `CardOlder` (nur alte Karte), `ShareTooLarge` (Bauen) und `SharePreviewStale` (das
Ziel hat sich seit der Vorschau geändert). Die Oberfläche zeigt für jede einen Hinweistext; bei
`SharePreviewStale` lädt sie die frische Vorschau, und es wird nichts geschrieben.
