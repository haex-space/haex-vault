# Vertrag: Block `openers` im Manifest

Gilt für `haextension/manifest.json` und für die Prüfung in `haex-bundle` (Repo `vault-sdk`,
`crates/haex-bundle/src/openers.rs`, neben `tools.rs`). holzi liest den Block in
`extensions/bundle/manifest.rs`.

## Form

```json
{
  "openers": ["mail.compose"]
}
```

- Optional. Fehlt der Block, ist die Extension kein Öffner.
- Ein Array von Zeichenketten, höchstens **8** Einträge, keine doppelten.
- Jeder Eintrag ist eine **Art** von Link: `^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$`, höchstens 32 Zeichen.
- Bekannte Art dieser Version: `mail.compose` („E-Mail schreiben“).

## Prüfung im `haex-bundle` (`check_openers`)

| Fall                                                       | Ergebnis                                    |
| ---------------------------------------------------------- | ------------------------------------------- |
| kein Array, ein Eintrag keine Zeichenkette                 | Fehler `openers is not an array of strings` |
| mehr als 8 Einträge                                        | Fehler mit der Zahl                         |
| Eintrag passt nicht zur Form oder ist über 32 Zeichen lang | Fehler mit Index und Eintrag                |
| ein Eintrag doppelt                                        | Fehler mit Index                            |
| Art unbekannt, aber von richtiger Form (`tel.dial`)        | gültig (FR-013)                             |

Die Prüfung steht in `haex-bundle::format` an derselben Stelle wie `check_tools` und läuft beim Signieren,
Verifizieren und im WebAssembly-Bau. Testvektoren in `test-vectors/` für gültig, zu lang, doppelt und falsche Form.

## Wie holzi es liest

- `Manifest.openers: Vec<String>` in Reihenfolge des Manifests. Ein Block falschen Typs zählt in holzi als
  abwesend (wie die anderen optionalen Felder), bevor das neue `haex-bundle` ihn ablehnt.
- Bekannte Arten (`OpenerKind`): `mail.compose`. Unbekannte Einträge bleiben im Manifest, werden nirgends
  angeboten und nicht in `ExtensionSummary.openers` genannt.
- Die Installationsansicht zeigt bei `mail.compose` die Zeile „kann E-Mail-Links öffnen“ (FR-014) wie die
  anderen deklarierten Angaben (`registry/declared_view.rs`); es ist keine Berechtigung und wird nicht als
  solche abgefragt.

## Beispiel haex-mail

```json
{ "openers": ["mail.compose"] }
```

Die Zeile kommt in `apps/haex-mail/haextension/manifest.json` neben `tools`.
