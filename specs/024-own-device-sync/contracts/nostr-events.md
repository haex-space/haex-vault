# Contract: Nostr-Ereignisse für Präsenz, Aufnahmeanfrage und Verknüpfen

Alle Ereignisse sind flüchtig (Art 21059, NIP-01: nicht gespeichert). Aufbau wie NIP-59: außen ein
Ereignis, signiert mit einem Einmalschlüssel, `p`-Tag an den Empfänger, Inhalt mit NIP-44 v2
verschlüsselt; darin ein Siegel (Art 13), signiert mit dem Geräteschlüssel des Absenders; darin das
eigentliche Ereignis (unsigniert, Art unten). Das innere Ereignis trägt den kanonischen `ts`-Wert
für Frischeprüfungen. `created_at` (das Zeitfeld jedes Nostr-Ereignisses, NIP-01) ist bei Siegel und
Gift-Wrap die aktuelle Zeit, nicht zufällig in die Vergangenheit verschoben wie bei gespeicherten
NIP-59-Nachrichten: Ein Nostr-Relay sieht die Ankunftszeit ohnehin, flüchtige Ereignisse speichert es
nicht, und verschobene Zeiten würden von Abos mit `since` weggefiltert und von Nostr-Relays mit
`created_at_lower_limit` (NIP-11) abgewiesen (research R7). Server: die eingestellten Nostr-Relays
(FR-008).

**Abos und Größen**: Abos setzen kein `since` (den Zeitfilter eines Abos, NIP-01). haex-vault sendet kein
Ereignis über 16 KiB und verwirft empfangene Ereignisse über 16 KiB, bevor es sie entschlüsselt;
Präsenzmeldungen sind unter 1 KiB. Je Gerät höchstens eine Präsenzmeldung pro Minute, außer bei einer
Adressänderung. Frisch ist eine Meldung, wenn ihr `ts` höchstens 150 s alt und höchstens 30 s in der
Zukunft liegt.

## Präsenzmeldung (inneres Ereignis Art 24100)

- Empfänger (`p`-Tag): `mb_pk` aus `mb_sk = HKDF-SHA256(ikm = inhaltsschlüssel der höchsten
Generation, info = "haex/presence/v1" ‖ tag_u32)`, `tag` = Tage seit 1970 (UTC). `tag_u32` ist
  die unsigned 32-bit Darstellung von `tag` in Big-Endian-Reihenfolge; sie wird ohne Textkodierung
  direkt an die UTF-8-Bytes des Info-Präfixes angehängt.
- Inhalt (JSON):

```json
{
  "v": 1,
  "device": "<hex device_pubkey>",
  "endpoint": "<hex endpoint_id>",
  "iroh_relay": "https://…",
  "addrs": ["203.0.113.5:4433"],
  "list_generation": 7,
  "ts": 1790000000000,
  "nonce": "<hex 16 byte>"
}
```

- Absender: nur ein Gerät, dessen Geräteliste mindestens ein weiteres Gerät nennt (FR-007). Takt:
  beim Öffnen, bei Adressänderung, alle 60 s.
- Empfänger prüft: Siegel mit einem Geräteschlüssel der geltenden Geräteliste signiert und gleich
  `device`; `ts` nicht älter als 150 s und nicht mehr als 30 s in der Zukunft; erst dann Adresse in
  den `MemoryLookup` übernehmen und, wenn noch keine besteht, eine Verbindung aufbauen. Meldungen
  unbekannter Geräte führen zu keiner Verbindung, außer zur Prüfung einer neueren Geräteliste
  (Kopie eines Hauptgeräts, FR-007): Nennt eine solche Meldung eine höhere `list_generation` als die
  eigene und ist der Absender nicht entfernt, wählt ein gelistetes Gerät ihn einmal je Meldung an
  (höchstens 16 Kandidaten zugleich); der Handshake übernimmt die Liste oder lehnt ab.
- Abo: `{kinds: [21059], "#p": [mb_pk(heute), mb_pk(gestern)]}` für den Inhaltsschlüssel der
  höchsten Generation und zusätzlich für bis zu vier ältere Inhaltsschlüssel, die das Gerät hält
  (neueste zuerst), sowie für bis zu vier höhere, die es wegen eines entfernten Empfängers
  übersprungen hat (R9; mit ihnen meldet sich ein Gerät, das von der Entfernung noch nichts weiß);
  erneuert beim Tageswechsel und sobald sich der Inhaltsschlüssel ändert. In den übrigen Postfächern
  wird nur gelesen, gesendet wird immer mit dem aktuellen Schlüssel: Ein Gerät, das offline war,
  während ein Gerät entfernt und der Schlüssel erneuert wurde, kennt nur den alten Schlüssel und
  meldet sich mit ihm. Die anderen hören es dort, prüfen die Meldung wie jede andere und wählen es
  an; Handshake und Sitzung bringen ihm die neue Liste und den neuen Schlüssel (FR-027). Ein
  entferntes Gerät erfährt so nichts Neues, weil in den übrigen Postfächern niemand sendet. Weil es
  die alten Schlüssel weiter hält, zählt in jedem Postfach außer dem des aktuellen Schlüssels nur
  die Meldung eines Geräts, das die geltende Liste nennt, und nur für ein gelistetes Gerät:
  Aufnahmeanfragen und Meldungen unbekannter Absender, auch mit höherer `list_generation`, werden
  dort verworfen.

## Aufnahmeanfrage (inneres Ereignis Art 24101)

- Empfänger: `mb_pk` wie oben (die Kopie hat den Inhaltsschlüssel aus der kopierten Datei).
- Inhalt: `{v, device, endpoint, vault_device_uuid, name, requested_at, sig, iroh_relay?, addrs?}`;
  `sig` = Schnorr des Geräteschlüssels über `haex-admission/v1 ‖ lp(device) ‖ lp(endpoint) ‖
lp(vault_device_uuid) ‖ lp(name) ‖ requested_at` (`requested_at` als 8 Byte Big-Endian).
  `vault_device_uuid` ist die Knoten-ID der Kopie: Die Geräteliste nennt ein Gerät damit, und die
  Änderungen aus der Wartezeit tragen sie. `iroh_relay` und `addrs` sagen, wo die Kopie gerade
  erreichbar ist; sie sind nicht signiert (das Siegel beglaubigt sie) und werden nie gespeichert.
- Der Empfänger verwirft eine Anfrage, wenn der Siegel-Signierer nicht `device` ist, die Signatur
  nicht passt, der Name über 256 Byte hat oder `requested_at` mehr als 30 s in der Zukunft oder
  mehr als 30 Tage in der Vergangenheit liegt.
- Jedes gelistete Gerät der Vault, das sie empfängt, legt sie in `admission_requests` ab (FR-045),
  solange der Absender weder auf der Liste steht noch entfernt ist; die Kopie wiederholt sie im
  Präsenztakt, bis sie auf der Geräteliste steht, und erneuert `requested_at`, wenn die Sitzung
  älter als 7 Tage wird. Steht der Absender inzwischen auf der Liste (aufgenommen, aber noch ohne
  die neue Liste), gilt die Anfrage als Präsenzmeldung: Das Gerät wird angewählt und bekommt die
  Liste im Handshake.
- Ein Gerät, das auf keiner Liste steht, legt keine Anfragen ab.

## Treffpunkt beim Verknüpfen (inneres Ereignis Art 24102)

- Empfänger: `rv_pk` aus `rv_sk = HKDF-SHA256(ikm = code, info = "haex/link/rendezvous/v1")`.
- Absender: die neue Installation mit ihrem frisch erzeugten Geräteschlüssel.
- Inhalt: `{v, endpoint, iroh_relay, addrs, ts}`.
- Das Hauptgerät abonniert `#p = rv_pk`, solange der Code gilt, und wählt die Installation über
  `haex-link/1` an (contracts/sync-protocol.md). Mehr als eine Meldung für denselben Code: nur die
  erste wird verfolgt.

## Metadaten, die ein Nostr-Relay sieht (D9)

IP-Adressen und Zeiten der Verbindungen, Größe der Ereignisse, einen täglich wechselnden
Empfänger je Vault, einen zufälligen Absender je Ereignis. Nicht: Vault-Identität,
Geräteschlüssel, Adressen, Namen.
