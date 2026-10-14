# Erweiterungen: Kontakte lesen

## Berechtigung

- Art `contacts` (`PermissionKind::Contacts`), Aktion nur `read`.
- Ziel `*` (alle Kontakte) oder ein Gruppenname (`Target::Tag`).
- Manifest:

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

- `ContactSummary`: `id`, `displayName`, `organization`, `phones[]`, `emails[]`, `groups[]`.
- `ContactDetail`: alle Kopf-Felder, alle Angaben (`kind`, `label`, `value`) einschließlich unbekannter
  Arten, bestätigte holzi-Adressen (`vault`, `devices[]`, `nostrRelays[]`), Gruppen; ohne Herkunft,
  Abgleichsziele und Zuordnungen.
- Fotos: `photo` als Hash; Bytes über `extension_contacts_photo { hash }` mit derselben Prüfung.

## Fehler

| Code | Bedeutung                                                                   |
| ---- | --------------------------------------------------------------------------- |
| 1002 | verweigert oder nicht vorhanden (nicht unterscheidbar, FR-031)              |
| 1004 | `PermissionPromptRequired` mit `{resourceType: "contacts", action, target}` |

## vault-sdk

Neue Datei `src/api/contacts.ts` und `src/commands/contacts.ts` im Repository
`https://github.com/haex-space/vault-sdk`; Export in `src/index.ts`. Der Plan der Umsetzung nennt den
Commit mit vollem SHA, sobald der PR dort gemergt ist.
