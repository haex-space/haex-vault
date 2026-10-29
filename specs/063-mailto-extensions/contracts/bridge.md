# Vertrag: Bridge-Methode und Ereignisse für Öffner

Eingeordnet in den Vertrag der Bridge (Spec 017, `contracts/bridge.md`). Der Name folgt der Regel
`extension_<bereich>_<verb>`.

## Methode `extension_opener_take`

Eine Extension holt die Anfrage ab, die der Nutzer ihr gegeben hat.

| Eingabe | Ausgabe                                                                                                                                    |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| keine   | `null` oder `{ "action": "mail.compose", "mail": { "to": string[], "cc": string[], "bcc": string[], "subject": string, "body": string } }` |

- Gibt nur die Anfrage der **aufrufenden** Extension zurück und löscht sie (genau einmal).
- Gibt `null` zurück, wenn keine wartet oder sie verfallen ist.
- Prüft, dass die Extension installiert, aktiviert und für `mail.compose` angemeldet ist; sonst `null`, kein
  Fehler (eine Extension erfährt nichts über Anfragen, die nicht für sie sind).
- Braucht **keine Berechtigung**: Die Anmeldung gibt keinen Zugriff (FR-010), und die Anfrage gibt es nur,
  weil der Nutzer gewählt hat (FR-005). Das Modul steht in der Allowlist wie `extension_get_info`.
- Der Link kommt nie roh zur Extension, nur die Felder als Text (FR-007).

## Ereignis `opener-request` (an die Frames der Extension)

`extension-frame-event {frame, type: "opener-request", data: {}, timestamp}` an jeden offenen Frame der
Extension, sobald eine Anfrage geparkt wurde. Der Frame ruft daraufhin `extension_opener_take`. Das Ereignis
trägt keinen Inhalt, damit nichts an einen Frame geht, der nicht abholt.

Eine Extension ruft `extension_opener_take` auch einmal beim Start des Frames, weil ein Frame, der erst durch
die Wahl entsteht, das Ereignis nicht mehr bekommt.

## SDK (`vault-sdk`)

`client.openers.take(): Promise<OpenRequest | null>` ruft die Methode auf;
`client.openers.onRequest(handler)` hört auf `opener-request` und ruft `take` selbst auf. Typ
`OpenRequest = { action: 'mail.compose'; mail: MailFields }`.

## Fehler

`extension_opener_take` gibt keine eigenen Fehlercodes; ein Fehler der Bridge (Frame unbekannt) bleibt der
bekannte.
