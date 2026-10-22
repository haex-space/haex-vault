# Vertrag: Passkey-Dienst (anlegen, bestätigen, auflisten)

**Spec**: [../spec.md](../spec.md) (FR-023 bis FR-036) | **Research**: [../research.md](../research.md) R5, R6, R8, R9

Drei Methoden von `PasswordsService` (`service/passkeys.rs`), Muster wie `read_secret_item`:
`pub async fn m(&self, caller: &Caller, grants: &[Grant], …)`. Es gibt **keine Tauri-Commands**
dafür (FR-023). Der Aufrufer ergibt sich aus dem Eingang, nie aus einem Argument. Die
Oberfläche des Nutzers benutzt weiter `passwords_passkey_rename` und `passwords_passkey_delete`
(nur `Caller::User`, FR-033, beide unverändert aus 034) und den neuen Command
`passwords_passkey_unlink` für Verbindungen.

Alle Binärfelder sind Base64URL ohne Auffüllung (WebAuthn). Fehler tragen eine Art (`kind`),
nie lokalisierten Text, nie einen Schlüssel.

## `passkey_create`

```text
request:  { itemId, rpId, rpName, origin, userHandle, userName, userDisplayName?,
            challenge, excludeCredentials: credentialId[], pubKeyCredParams: int[]   // COSE-Algorithmen
            discoverable: bool }
response: { credentialId, attestationObject, clientDataJson, publicKeySpki, algorithm, itemId }
errors:   Forbidden | NotFound | InvalidInput{field} | OriginMismatch | UnsupportedAlgorithm
          | ExcludedCredentialExists
```

- **Berechtigung** (FR-031): `authorize_update` für `itemId` mit Art „Lesen und Schreiben“, der
  Eintrag muss im Bereich liegen. Ein `itemId` außerhalb des Bereichs (oder ohne Eintrag):
  `NotFound` für Aufrufer mit Freigabe, `Forbidden` ohne. Der eingebaute Agent hat nie Zugriff
  (`reach()` → `Forbidden`).
- **Pflicht**: `itemId`, `rpId`, `rpName`, `userHandle`, `userName`, `challenge`, `origin`
  (sonst `InvalidInput{field}`, FR-024). `userHandle` darf nach Base64URL-Dekodierung
  höchstens 64 Byte lang sein; andernfalls gilt `InvalidInput{field:"userHandle"}`.
- **Herkunft**: `origin_matches(origin, rpId)` (R9) sonst `OriginMismatch`.
- **Ausgeschlossene**: existiert ein Passkey der Gegenstelle mit einer Kennung aus
  `excludeCredentials` in einem für den Aufrufer **sichtbaren** Eintrag →
  `ExcludedCredentialExists` (es entsteht nichts).
- **Algorithmus**: der erste von ES256 (−7), EdDSA (−8) in `pubKeyCredParams` in dieser
  Rangfolge; keiner → `UnsupportedAlgorithm`.
- **Schlüssel**: neues Paar, PKCS8 (Base64) und SPKI (Base64) wie bisher in `passkeys`;
  `credentialId` = 32 Zufallsbytes; Zeile mit `sign_count` 0, `created_at`, `last_used_at`
  leer, `is_discoverable` aus der Anfrage; Kennung = UUIDv5 der Credential-ID (034 R2).
- **Antwort**: `clientDataJson` = `{"type":"webauthn.create","challenge":…,"origin":…,"crossOrigin":false}`
  (Reihenfolge fest); `attestationObject` = CBOR `{fmt:"none", attStmt:{}, authData}`;
  `authData` = `SHA-256(rpId)` ‖ Flags `0x58` (BE, BS, AT; kein UP ohne Presence-Nachweis) ‖ Zähler 0 (4 Byte) ‖
  AAGUID (16 Nullbytes) ‖ Länge der Credential-ID (2 Byte) ‖ Credential-ID ‖ COSE-Schlüssel;
  `publicKeySpki` für `getPublicKey()`.

## `passkey_confirm`

```text
request:  { rpId, origin, challenge, allowCredentials: credentialId[], itemId? }
response: { credentialId, authenticatorData, clientDataJson, signature, userHandle?, itemId }
errors:   Forbidden | NotFound | OriginMismatch | UnsupportedAlgorithm | ChoiceRequired{candidates: PasskeyHeader[]}
```

- **Kandidaten**: Passkeys mit `rpId`, in einem für den Aufrufer lesbaren Eintrag
  (`authorize_read`; Papierkorb und fremde Tags: lautlos nicht vorhanden); bei
  `allowCredentials` nur diese Kennungen, sonst nur `is_discoverable`; bei `itemId` nur dieser
  Eintrag; **Passkeys per Verbindung** zählen über das Ziel mit, wenn Ziel **und** Quelle
  lesbar sind (FR-047). Passkeys ohne Eintrag zählen nie.
- **Ein Passkey ist ein Kandidat**, auch wenn er über mehrere Wege lesbar ist: Die Kandidaten
  werden nach Passkey (`id`) zusammengefasst, bevor gezählt wird; als `itemId` der Antwort gilt
  der eigene Eintrag, wenn er lesbar ist, sonst das erste lesbare Ziel. Ohne das gäbe derselbe
  Passkey über seinen Eintrag und eine Verbindung zwei Kandidaten, und `ChoiceRequired` käme
  auch mit seiner Kennung in `allowCredentials` immer wieder.
- Kein Kandidat: `NotFound` (mit Freigabe) beziehungsweise `Forbidden` (ohne), nie ein Hinweis
  auf Passkeys außerhalb des Bereichs. Mehr als einer: `ChoiceRequired` mit den Kopfdaten; der
  Aufrufer wiederholt mit der Kennung in `allowCredentials`.
- **Herkunft**: wie oben, **bevor** irgendetwas geschrieben oder signiert wird.
- **Berechtigung**: mindestens „Lesen“ im Bereich des Eintrags (FR-031).
- **Zähler**: immer 0 (R5), weil getrennte Offline-Geräte keinen global monotonen Wert
  garantieren können; `last_used_at` wird in **derselben** Transaktion wie das Signieren
  geschrieben. Schlägt das Schreiben fehl, wird nichts zurückgegeben.
- **Signatur**: über `authenticatorData ‖ SHA-256(clientDataJson)`; ES256 als DER-ECDSA (P-256,
  SHA-256), EdDSA als 64 Byte. `authenticatorData` = `SHA-256(rpId)` ‖ Flags `0x18` (BE, BS;
  kein UP ohne Presence-Nachweis) ‖ Zähler 0 (4 Byte, Big Endian). UV (`0x04`) steht weder
  hier noch beim Anlegen: kein Mensch bestätigt die Anfrage (R8). Damit sind die Antworten
  für Gegenstellen, die UP verlangen, bewusst nicht verwendbar, bis ein vertrauenswürdiger,
  zeremoniegebundener Presence-Nachweis ergänzt ist. `clientDataJson` =
  `{"type":"webauthn.get","challenge":…,"origin":…,"crossOrigin":false}`.
- **Seit Spec 049 (ADR-0015)**: Anlegen und Bestätigen nehmen `presence: UserPresence`
  (`None`, `Present`, `Verified`, nicht deserialisierbar). `Present` setzt UP, `Verified` UP und
  UV; `None` lässt die Flags wie oben. Nur die External Bridge erzeugt `Present` oder `Verified`,
  nach einer Anwesenheitsbestätigung für genau eine Anfrage. Anlegen nimmt statt `itemId` ein
  Ziel `target: Existing(itemId) | New { title, url, username, tags }`; `New` legt Eintrag und
  Passkey in einer Transaktion an.
- RS256 (importiert) → `UnsupportedAlgorithm` (FR-034); es wird dabei kein Nutzungszustand
  geschrieben.

## `passkey_list`

```text
request:  { rpId?, itemId?, discoverableOnly?: bool }
response: PasskeyHeader[] // { id, credentialId, rpId, rpName, userName, userDisplayName, nickname,
                          //   algorithm, isDiscoverable, createdAt, lastUsedAt, itemId, linkedFromItemId? }
errors:   Forbidden
```

Nur Passkeys an **lesbaren** Einträgen (Ziele per Verbindung zählen mit Ziel und Quelle),
sonst gar nicht (kein Hinweis, FR-029); Passkeys im Papierkorb und ohne Eintrag nie. Die
Oberfläche des Nutzers liest Passkeys nicht hier, sondern je Eintrag über `get_item` (dort
erscheinen Passkeys im Papierkorb mit ihrem Eintrag; solche ohne Eintrag nirgends). **Nie**
ein Schlüssel.

## Zusätzliche Funktion für die Oberfläche

`passwords_passkey_unlink` (`{ itemId, passkeyId }`, nur Nutzer): löscht eine Verbindung
(„Verweis lösen“), nicht den Passkey. `passwords_passkey_delete` bleibt wie in 034
(`{ passkeyId }`) und löscht immer den Passkey selbst samt seinen Verbindungen; die
Oberfläche bietet es nur am eigenen Eintrag des Passkeys an. Eine Verbindung über
`passkey_delete` zu lösen ginge nicht: `passkeyId` allein sagt nicht, welches Ziel gemeint ist.

## Tests (Rust, `passkeys_ops_tests.rs`, `webauthn_tests.rs`, `src-tauri/tests/passwords_passkeys.rs`)

- Anlegen → Bestätigen: Signatur mit dem öffentlichen Schlüssel prüfen (ES256 und EdDSA),
  Zähler bleibt bei 0; UP und UV sind nicht gesetzt.
- Zwei Geräte: je ein Bestätigen, Sync, danach bleibt der Zähler auf beiden Geräten 0
  (`passwords_sync.rs`).
- Herkunft: `evil.com` für `example.com`, `com` als `rpId`, `co.uk`, Unterdomäne, `localhost`
  mit `http`, eine IP-Adresse als `rpId` (abgelehnt), IDNA, Port; jede Ablehnung ohne Signatur
  und ohne Zähleränderung.
- Verbindung: ein Passkey, der über seinen Eintrag und eine Verbindung lesbar ist, ist **ein**
  Kandidat (kein `ChoiceRequired`).
- Bereich: Tag A sieht und bestätigt nur Passkeys an Einträgen mit Tag A; Papierkorb,
  Verbindung (Quelle außerhalb → nicht vorhanden), Passkey ohne Eintrag, eingebauter Agent.
