# Quickstart: Dateizugriff und Dateifreigaben von Hand prüfen

Voraussetzung: zwei Rechner (oder zwei Instanzen nach Spec 030) im selben Netz.

- **Anna**: Vault A auf Gerät A1 (Laptop) und A2 (zweites Gerät, verknüpft nach Spec 024).
- **Ben**: Vault B auf Gerät B1.
- Anna und Ben haben die Visitenkarte des anderen ins Adressbuch übernommen (Spec 051).

Automatische Prüfungen vor der Handprüfung:

```bash
nix develop --command scripts/with-nix-host-bridge.sh cargo test --lib files::share
nix develop --command scripts/with-nix-host-bridge.sh cargo test --lib sync::files
nix develop --command pnpm typecheck && nix develop --command pnpm lint
nix develop --command pnpm format:check
```

## 1. Eigene Geräte (US1, US5)

1. A2: Dateibrowser → „Meine Geräte“ zeigt A1 „erreichbar“.
2. A2: A1 öffnen, `Dokumente` auflisten, ein PDF ansehen, ein Video öffnen und springen (SC-002: Start
   < 3 s).
3. A2: ein Bild nach A1 kopieren, mittendrin abbrechen → auf A1 keine halbe Datei (SC-008).
4. A2: auf A1 eine Datei löschen → liegt im Papierkorb von A1.
5. A2: das Datenverzeichnis von holzi auf A1 öffnen → gesperrt (FR-007).
6. A1: Einstellungen → Föderation → „Geräte-Dateizugriff“ aus. A2 zeigt „Dateizugriff auf diesem Gerät
   ausgeschaltet“. Wieder an.

## 2. Freigabe an einen Kontakt (US2)

1. A1: Ordner `Fotos/Urlaub` → „Freigeben …“ → Ben, „Lesen“ (SC-003: < 30 s).
2. B1: Dateibrowser → „Von Kontakten“ → Anna aufklappen → „Urlaub“ erscheint mit „Lesen“.
3. B1: Pfadleiste endet an „Urlaub“; keine Aktionen zum Schreiben; Foto in eigenen Ordner kopieren.
4. A1 beenden → B1 zeigt „Urlaub“ ausgegraut, „nicht erreichbar“.
5. A1: einen Ordner in einem S3-Speicher und einen verschlüsselten Ordner freigeben → B1 sieht Klartext.

## 3. Schreiben und Konflikt (US3)

1. A1: Freigabe auf „Lesen und Schreiben“ stellen.
2. B1: Datei anlegen, umbenennen, löschen → wirkt auf A1; Wurzel umbenennen → abgelehnt.
3. Große Datei von B1 in die Freigabe kopieren und währenddessen auf A1 dieselbe Datei ändern → B1
   fragt „Ersetzen“ oder „Beide behalten“.

## 4. Entziehen (US4)

1. B1 kopiert eine große Datei aus der Freigabe; A1 entzieht Ben → Transfer bricht ab (SC-004: < 2 s).
2. A1 beenden; auf A2 Ben ein Recht entziehen → „Änderung wartet auf ‹A1›“; A1 starten → Zustand
   verschwindet.
3. A1: Quellordner umbenennen → „Quelle fehlt“.
4. A1: Ben im Adressbuch löschen → Dialog nennt die Freigaben; danach kein Zugriff mehr.

## 5. Agents (US6)

1. A2: Chat-Agent ohne Berechtigung für „A1“ → fragt bzw. lehnt ab.
2. Mit „Lesen“: Zusammenfassung einer Datei auf A1 gelingt; Schreiben wird abgelehnt.
3. Agent bitten, einen Ordner an Ben freizugeben → nicht möglich (FR-045).
