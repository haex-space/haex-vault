# Feature Specification: Allgemein mit Grundeinstellung und Erscheinungsbild

**Feature Branch**: `feat/settings-general-restructure`

**Created**: 2026-10-07

**Status**: Ready for implementation

**Input**: User description: "ich möchte die menü punkte in den einstellungen überarbeiten. 1. "Allgemein" soll die Unterpunkte "Grundeinstellung" und "Erscheinungsbild" bekommen 2. unter grundeinstellung möchte ich Sprache, Vaultname und Vaultpasswort anpassen können 3. unter erscheinungsbild soll das Theme und der Workspace Hintergrund festgelegt werden können"

Abgestimmter Entwurf: [`docs/plans/2026-10-07-settings-general-restructure-design.md`](../../docs/plans/2026-10-07-settings-general-restructure-design.md). Der Vaultname ist dort bewusst gestrichen (keine Stelle zeigt ihn an).

## User Scenarios & Testing

### User Story 1 - Allgemein in zwei Unterpunkte gegliedert (Priority: P1)

Als Nutzer finde ich unter „Allgemein“ zwei Unterpunkte, „Grundeinstellung“ und „Erscheinungsbild“, statt einer flachen Seite und einer separaten Kategorie „Darstellung“. So liegen alle grundlegenden und alle optischen Einstellungen jeweils an einem Ort.

**Why this priority**: Die neue Struktur ist der Rahmen, in den alle weiteren Stories ihre Einstellungen einhängen.

**Independent Test**: Einstellungen öffnen, „Allgemein“ wählen, beide Unterpunkte öffnen und prüfen, dass Gerätename und Sitzung wiederherstellen unter Grundeinstellung und alle bisherigen Darstellungs-Regler unter Erscheinungsbild liegen; die Sidebar zeigt keine Kategorie „Darstellung“ mehr.

**Acceptance Scenarios**:

1. **Given** die Einstellungen sind offen, **When** der Nutzer „Allgemein“ wählt, **Then** sieht er eine Übersicht mit den Einträgen „Grundeinstellung“ und „Erscheinungsbild“, jeweils mit Symbol und Kurzbeschreibung.
2. **Given** die Übersicht „Allgemein“, **When** der Nutzer „Grundeinstellung“ öffnet, **Then** sieht er Sprache, Vaultpasswort, Gerätename und Sitzung wiederherstellen.
3. **Given** die Übersicht „Allgemein“, **When** der Nutzer „Erscheinungsbild“ öffnet, **Then** sieht er Farbschema, Workspace-Hintergrund, Akzent- und Flächenfarben, Fensterhinweis sowie Import, Export und Zurücksetzen.
4. **Given** die Sidebar der Einstellungen, **When** der Nutzer die Kategorien betrachtet, **Then** gibt es keine Kategorie „Darstellung“.
5. **Given** die Einstellungssuche, **When** der Nutzer nach einer einzelnen Einstellung sucht (z. B. „Akzent“, „Sprache“, „Hintergrund“), **Then** führt der Treffer zum Unterpunkt, auf dem sie liegt.

---

### User Story 2 - Sprache wählen (Priority: P1)

Als Nutzer sehe ich holzi von Anfang an in meiner Systemsprache, kann sie schon auf dem Startbildschirm ändern und lege sie in der Vault fest, sodass sie auf allen meinen Geräten gilt.

**Why this priority**: Die Sprachwahl betrifft jeden Bildschirm und fehlt heute ganz.

**Independent Test**: App mit deutscher und mit nicht unterstützter Systemsprache starten, Sprache auf dem Startbildschirm umschalten, Vault entsperren, Sprache in Grundeinstellung ändern und auf einem verknüpften Gerät prüfen.

**Acceptance Scenarios**:

1. **Given** die Systemsprache ist Deutsch, **When** die App startet, **Then** erscheint der Startbildschirm auf Deutsch.
2. **Given** die Systemsprache ist weder Deutsch noch Englisch, **When** die App startet, **Then** erscheint der Startbildschirm auf Englisch.
3. **Given** der Startbildschirm, **When** der Nutzer dort eine andere Sprache wählt, **Then** wechselt die Oberfläche sofort; nach einem Neustart der App gilt wieder die Systemsprache.
4. **Given** eine Vault mit gespeicherter Sprache, **When** der Nutzer sie entsperrt, **Then** wechselt die Oberfläche auf diese Sprache, unabhängig von der Wahl auf dem Startbildschirm.
5. **Given** eine neue oder bestehende Vault ohne gespeicherte Sprache, **When** der Nutzer sie anlegt oder entsperrt, **Then** speichert die Vault die gerade aktive Sprache.
6. **Given** Grundeinstellung, **When** der Nutzer die Sprache ändert, **Then** wechselt die Oberfläche sofort und die Wahl gilt nach der Synchronisierung auch auf den verknüpften Geräten.

---

### User Story 3 - Vaultpasswort ändern (Priority: P2)

Als Nutzer ändere ich das Passwort, mit dem diese Vault auf diesem Gerät entsperrt wird, nachdem ich das aktuelle Passwort bestätigt habe.

**Why this priority**: Wichtig für die Sicherheit, aber seltener gebraucht als Struktur und Sprache.

**Independent Test**: Passwort ändern, App schließen, mit altem Passwort scheitern, mit neuem Passwort entsperren.

**Acceptance Scenarios**:

1. **Given** Grundeinstellung, **When** der Nutzer „Vaultpasswort ändern“ wählt, **Then** öffnet sich eine eigene Ansicht mit den Feldern aktuelles Passwort, neues Passwort und Wiederholung sowie dem Hinweis, dass die Änderung nur auf diesem Gerät gilt.
2. **Given** die Passwort-Ansicht, **When** der Nutzer das richtige aktuelle Passwort und zweimal ein gültiges neues Passwort eingibt und bestätigt, **Then** ist die Vault ab sofort nur noch mit dem neuen Passwort zu entsperren und alle Daten bleiben erhalten.
3. **Given** die Passwort-Ansicht, **When** das aktuelle Passwort falsch ist, **Then** bleibt das Passwort unverändert und der Nutzer sieht eine Fehlermeldung.
4. **Given** die Passwort-Ansicht, **When** neues Passwort und Wiederholung nicht übereinstimmen oder das neue Passwort kürzer als die Mindestlänge beim Anlegen ist, **Then** lässt sich die Änderung nicht absenden.
5. **Given** ein Agent, **When** er das Vaultpasswort ändern soll, **Then** kann er es nicht ändern, aber die Passwort-Ansicht für den Nutzer öffnen.
6. **Given** ein mit dieser Vault verknüpftes Gerät, **When** auf diesem Gerät das Passwort geändert wurde, **Then** entsperrt das andere Gerät weiterhin mit seinem eigenen, unveränderten Passwort.

---

### User Story 4 - Workspace-Hintergrund festlegen (Priority: P3)

Als Nutzer lege ich ein Bild als Hintergrund für meine Workspaces fest oder entferne es wieder.

**Why this priority**: Rein optisch; Struktur, Sprache und Passwort haben Vorrang.

**Independent Test**: Bild wählen, alle Workspaces prüfen, verknüpftes Gerät prüfen, Bild entfernen.

**Acceptance Scenarios**:

1. **Given** Erscheinungsbild, **When** der Nutzer ein Bild auswählt, **Then** erscheint es als Hintergrund hinter allen Workspaces und Fenstern.
2. **Given** ein gesetzter Hintergrund, **When** der Nutzer „Entfernen“ wählt, **Then** erscheint wieder der bisherige Standard-Hintergrund.
3. **Given** kein gesetzter Hintergrund, **When** der Nutzer Erscheinungsbild betrachtet, **Then** wird kein „Entfernen“ angeboten.
4. **Given** ein gesetzter Hintergrund, **When** ein verknüpftes Gerät synchronisiert, **Then** zeigt es denselben Hintergrund.
5. **Given** ein sehr großes Bild, **When** der Nutzer es auswählt, **Then** wird es auf eine handliche Größe reduziert gespeichert, ohne dass es auf dem Bildschirm sichtbar unscharf wirkt.

### Edge Cases

- Eine Bilddatei, die sich nicht lesen oder dekodieren lässt, ändert den Hintergrund nicht und zeigt eine Fehlermeldung.
- Bricht die Passwortänderung mitten im Vorgang ab (z. B. Absturz), lässt sich die Vault mit genau einem der beiden Passwörter weiter öffnen; es gehen keine Daten verloren.
- Während der Passwortänderung laufen keine Schreibzugriffe (Sync, Agent) parallel auf die Vault.
- Ein neues Passwort, das dem aktuellen gleicht, wird abgelehnt.
- Ändert ein anderes Gerät die Sprache, während dieses Gerät die Vault offen hat, wechselt die Oberfläche nach der Synchronisierung.
- Auf dem Startbildschirm ohne Vault gibt es nichts zu speichern; die Wahl lebt nur bis zum Neustart.

## Requirements

### Functional Requirements

- **FR-001**: The settings sidebar MUST NOT contain a category "Darstellung".
- **FR-002**: The category "Allgemein" MUST open an overview with exactly two entries, "Grundeinstellung" and "Erscheinungsbild", each with icon and one-line description.
- **FR-003**: "Grundeinstellung" MUST contain language, a row leading to the vault-password view, device name and session restore.
- **FR-004**: "Erscheinungsbild" MUST contain color scheme, workspace background and every appearance control that "Darstellung" contained before (accent, window, container, text and component tints, window hint, import, export, reset).
- **FR-005**: The settings search MUST lead each single setting of FR-003 and FR-004 to the sub-view it is on.
- **FR-006**: Before a vault is open, the interface language MUST be German when the system language is German and English otherwise; the device MUST NOT persist a language choice of its own.
- **FR-007**: The start screen MUST offer a language choice that switches the interface at once and lasts until the app restarts.
- **FR-008**: The vault MUST store one language for all its devices and synchronize it.
- **FR-009**: On unlock, a stored vault language MUST take effect; a vault without one (new or existing) MUST store the language active at that moment.
- **FR-010**: Changing the language in "Grundeinstellung" MUST switch the interface at once and store it in the vault; a language change synchronized from another device MUST take effect on this device.
- **FR-011**: The vault-password view MUST be its own settings location that requires current password, new password and repetition and states that the change applies to this device only.
- **FR-012**: A password change MUST only happen when the current password is correct, the new password meets the minimum length of vault creation, matches its repetition and differs from the current one.
- **FR-013**: After a successful change, the vault on this device MUST open with the new password only and keep all its data; other devices' passwords MUST stay unchanged.
- **FR-014**: No agent MUST be able to change the vault password; an agent MUST be able to open the vault-password view.
- **FR-015**: While the password changes, no other access MUST write to the vault.
- **FR-016**: The user MUST be able to choose an image file as workspace background and remove it again; "Entfernen" MUST only be offered while a background is set.
- **FR-017**: A set background MUST show behind every workspace; without one, the current default background MUST show.
- **FR-018**: The background MUST be stored in the vault and synchronized to its devices, reduced to at most 2560 px on its long edge.
- **FR-019**: An agent MUST be able to set the language and remove the background; to set a background it MUST be able to open "Erscheinungsbild" for the user, who then chooses the image (research R8).

### Key Entities

- **Vault language**: one of German or English; stored once per vault, synchronized; absent until first set.
- **Workspace background**: one image or none per vault; stored reduced in size, synchronized.
- **Vault password**: per device; protects this device's copy of the vault; never synchronized.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Every setting that was reachable under "Allgemein" or "Darstellung" before is reachable in at most two clicks from the sidebar entry "Allgemein".
- **SC-002**: A user with a German system sees the start screen in German, a user with any other system language in English, in 100 % of starts.
- **SC-003**: A language change in "Grundeinstellung" is visible on a linked device after its next synchronization without any action there.
- **SC-004**: After a password change, unlocking with the old password fails and unlocking with the new one succeeds, with every item of the vault still present.
- **SC-005**: A background chosen on one device shows on a linked device after its next synchronization.
- **SC-006**: No agent tool exists through which the vault password can be changed.

## Assumptions

- The vault name from the original request is out of scope: no screen would show it (operator decision 2026-10-07). It can be added later as one more vault setting.
- Supported languages stay German and English.
- The password change follows haex-vault 2.x's (Legacy) proven sequence for re-encrypting a vault; holzi does not keep the password in memory, so the current one is verified against the vault file.
- The password plays no part in synchronization, so a change needs no step on sync servers.
- A background of a few hundred kilobytes fits the existing synchronized preference/page budget; T037
  covers that boundary with a two-device regression check.
- No backward compatibility: holzi has no users yet.
