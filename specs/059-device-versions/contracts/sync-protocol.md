# Contract: Änderung des Sync-Protokolls (`holzi-sync/1`, Protokollversion 2)

Ändert [Spec 024 contracts/sync-protocol.md](../../024-own-device-sync/contracts/sync-protocol.md), Abschnitt
„1. Handshake“. Alles, was hier nicht steht, bleibt.

## Challenge

```text
A → D  Challenge   { v: 2, nonce_a: [u8;32], endpoint_a: [u8;32], list: ListRef,
                     schema: SchemaVersion }
```

`schema` ist der Stand des annehmenden Geräts und das **letzte** Feld. `Response.schema` bleibt unverändert.
`SchemaVersion = { protocol: 2, holzi_migration: u32, crdt_trigger: u32 }`. `v` und `SchemaVersion.protocol`
sind beide `PROTOCOL_VERSION`.

## Regeln

- `v` ist das erste Feld von `Challenge` und `Response`, also das Byte direkt nach dem Variantenkennzeichen
  (`u8`, postcard; `Challenge` ist Variante 0). Das wählende Gerät liest die `Challenge`
  als ganzen Rahmen, prüft `bytes[1] == PROTOCOL_VERSION` und dekodiert danach. Bei Abweichung endet es mit
  `Refused(Incompatible)`, ohne `Response` zu senden; das Gerät wird über den Endpunkt benannt (research R3).
- Beide Seiten halten den Stand, den sie sehen, für das Gerät fest, das die geltende Liste für den Endpunkt der
  Verbindung nennt: das wählende Gerät aus `Challenge.schema`, das annehmende aus `Response.schema`. Das
  geschieht, bevor entschieden wird, also auch bei `Reject`. Fremde Vault und ungültige Signatur halten nichts
  fest.
- Verträglichkeit unverändert: gleiches `v` und gleiches `schema`, sonst `Incompatible`.
- Vor `Accept` fließen damit die Geräteliste (FR-009) und die drei Zahlen des Versionsstands. Die Zahlen sagen
  nichts über Inhalte; der Rahmen bleibt unter dem Limit von 64 KiB.
- Die ALPN bleibt `holzi-sync/1`. Sie ändert sich nur bei einem Bruch, der den Handshake selbst unlesbar
  macht; die Versionsprüfung ist `v` direkt nach dem Variantenkennzeichen (ersetzt „Die Protokollversion steckt im ALPN“ in
  research R14 von Spec 024).

## Ältere Stände (`v = 1`)

- Ein älteres wählendes Gerät liest die neue `Challenge` bis zu seinem letzten bekannten Feld, sieht `v = 2` und
  lehnt sich selbst mit `Incompatible` ab.
- Ein neues wählendes Gerät sieht in einer alten `Challenge` `v = 1`, lehnt sich mit `Incompatible` ab und
  zeigt „Stand noch nicht bekannt“ für das Gerät.
- Ein neues annehmendes Gerät dekodiert die alte `Response` (Layout unverändert), hält ihr `schema` fest und
  antwortet `Reject { Incompatible }`.
