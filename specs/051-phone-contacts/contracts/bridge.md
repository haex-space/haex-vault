# Erweiterungen: Kontakte lesen

## Berechtigung

- Art `contacts` (`PermissionKind::Contacts`), Aktion nur `read`.
- Ziel `*` (alle Kontakte) oder ein Gruppenname (`Target::Tag`).
- Manifest (`readWrite` oder eine andere Aktion wird als unlesbare Erklärung gelistet und nie zur
  Berechtigung):

```json
{ "permissions": [{ "kind": "contacts", "action": "read", "target": "*" }] }
```

- Zustände und Rückfrage wie Spec 017 FR-016; eine verweigerte `*` entzieht alles, eine verweigerte
  Gruppe entzieht diese Gruppe (wie `extensions/passwords.rs::grants_of`).

## Brückenmethoden

| Methode                     | Parameter                     | Antwort                                 |
| --------------------------- | ----------------------------- | --------------------------------------- |
| `extension_contacts_list`   | `{ group?, offset?, limit? }` | `{ contacts: ContactSummary[], total }` |
| `extension_contacts_read`   | `{ id }`                      | `ContactDetail`                         |
| `extension_contacts_search` | `{ query, limit? }`           | `{ contacts: ContactSummary[] }`        |
| `extension_contacts_photo`  | `{ hash }`                    | JPEG als Base64                         |

- `ContactSummary`: `id`, `displayName`, `organization`, `phones[]`, `emails[]`, `groups[]`
  (Gruppennamen), `photo`.
- `ContactDetail`: `id`, `displayName`, alle Kopf-Felder, alle Angaben (`id`, `kind`, `label`, `value`)
  einschließlich unbekannter Arten, Gruppennamen, `photo`, bestätigte holzi-Adressen als
  `holziAddresses` (`vault` hex, `npub`, `devices[]` mit `devicePubkey`, `endpointId`, `irohRelay`,
  `nostrRelays[]`), `updatedAt`; ohne Herkunft, Abgleichsziele, Probleme und Zuordnungen.
- Ein Feld ohne Wert fehlt, nie `null` (die Aktionen der Agenten prüfen ihre Ergebnisse gegen ein
  Schema ohne `null`).
- Fotos: `photo` als Hash; Bytes über `extension_contacts_photo { hash }` mit derselben Prüfung.
- `list` gibt ohne `limit` 100 Kontakte, höchstens 1.000; `total` zählt alle im Bereich (der Gruppe).
  `search` gibt ohne `limit` 20 Treffer, höchstens 200, mit derselben Suche wie das Fenster.
- `group` und Gruppenziele vergleichen den Namen ohne Groß- und Kleinschreibung; tragen mehrere Gruppen
  denselben Namen, gelten alle. Eine Rückfrage nennt die Gruppe so, wie das Adressbuch sie schreibt.

## Fehler

| Code | Bedeutung                                                                   |
| ---- | --------------------------------------------------------------------------- |
| 1002 | verweigert oder nicht vorhanden (nicht unterscheidbar, FR-031)              |
| 3001 | Parameter unlesbar (`id`, `hash` leer, `query` keine Zeichenkette, …)       |
| 1004 | `PermissionPromptRequired` mit `{resourceType: "contacts", action, target}` |

## vault-sdk

Neue Datei `src/api/contacts.ts` und `src/commands/contacts.ts` im Repository
`https://github.com/haex-space/vault-sdk`; Export in `src/index.ts`. Gemergt mit haex-space/vault-sdk#71
als Commit `04a9cd614d68a13a94d0b28247208afeddfe5cca` (`sdk.contacts.listAsync`, `readAsync`,
`searchAsync`, `photoAsync`).
