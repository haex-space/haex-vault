# Feature Specification: holzi für Android

**Feature Branch**: `043-android-build`
**Created**: 2026-10-07
**Status**: Draft
**Input**: holzi läuft als Android-App auf einem echten Telefon. Die Lieferung erfolgt in
einzeln lieferbaren Stufen: zuerst die App selbst mit Tresor, Passwortmanager und Window
Manager (wm) im Kompaktmodus, dann der Sync mit dem Desktop, Erweiterungen, Chat mit
Online-Anbietern und zuletzt lokale KI und Spracheingabe. Ein CI-Job baut das APK, Release-Builds
tragen einen eigenen Signaturschlüssel. Ein Prozess hält wie in Spec 013 genau eine
Tresor-Sitzung; den Tresor schließen beendet die App. Die Build-Umgebung (Android SDK und NDK,
JDK 17, Rust mit den Android-Zielen) kommt aus der Nix-devShell. Sie verwendet das atoms-Molekül
`holzi` 0.8.0 aus [`haexmas/atoms` @ `b662c6205d6a1ebd7fac588291f82ea8e78c3d52`](https://github.com/haexmas/atoms/tree/b662c6205d6a1ebd7fac588291f82ea8e78c3d52),
insbesondere `holzi/.devshell/packages.nix`, `holzi/.devshell/rust-toolchain.toml` und
`holzi/nix-packages.json` (in holzi mit PR #308 als `.devshell/packages.nix`,
`.devshell/rust-toolchain.toml` und `.spaex/generated/nix-packages.json` ausgeliefert), und ist
nicht Teil dieser Spec.

## Beziehung zu bestehenden Specs

- [`013-vault-lifecycle-isolation`](../013-vault-lifecycle-isolation/spec.md): Die Regel „ein
  Prozess, eine Tresor-Sitzung“ gilt auf Android unverändert. Wo der Desktop nach dem Schließen
  neu startet, um die Tresorauswahl zu zeigen, beendet Android die App (FR-005 bis FR-009).
- [`014-portable-mode`](../014-portable-mode/spec.md): Gibt es auf Android nicht.
- [`015-workspace-shell`](../015-workspace-shell/spec.md) US6 und FR-028 sowie
  [`020-tab-navigation`](../020-tab-navigation/spec.md) FR-019: Der Kompaktmodus und die
  Android-Zurück-Geste sind dort beschrieben; diese Spec verlangt, dass sie auf einem Telefon
  tatsächlich so funktionieren, und prüft sie dort.
- [`036-password-redesign`](../036-password-redesign/spec.md) T081: Die Telefon-Prüfungen des
  Passwortmanagers (Wischen zwischen Tabs, Langdruck-Auswahl, Zeilenmenü, Lightbox mit Pinch
  und Wischen, 360 px Breite) sind Teil von User Story 1 dieser Spec.
- [`024-own-device-sync`](../024-own-device-sync/spec.md) und
  `025-own-device-file-sync` (entfernt, ersetzt durch [`061-sync-rules`](../061-sync-rules/spec.md)): Ein Telefon ist ein
  weiteres eigenes Gerät. Beide Specs sehen bereits vor, dass mobile Geräte nur im Vordergrund
  synchronisieren.
- [`017-extension-host`](../017-extension-host/spec.md) FR-066 und T112: Erweiterungen laufen
  auf Android mit denselben Regeln; ein Erweiterungsrahmen sieht die interne Schnittstelle von
  holzi nie. T112 (Prüfung auf Android) wird mit User Story 4 dieser Spec erledigt.
- [`007-cli-delegate`](../007-cli-delegate/spec.md): Delegates über externe Kommandozeilenprogramme
  gibt es auf Android nicht (FR-016, FR-026).
- [`008-voice-control-stt`](../008-voice-control-stt/spec.md) FR-011 und
  [ADR 0002](../../docs/adr/0002-local-model-profiles.md): Telefon-Voreinstellungen für lokale
  Modelle und eine kleinere Spracherkennung auf dem Telefon.
- [`033-multi-device-e2e`](../033-multi-device-e2e/spec.md) FR-022: Die Plattform-Beschreibung
  für Android ergänzt diese Spec um das, was auf dem Telefon geprüft wurde.
- [`044-file-browser`](../044-file-browser/spec.md) FR-039: Der eingebaute Dateibrowser bekommt
  auf Android „Zugriff auf alle Dateien“ und damit echte Pfade. Das gilt nur für den Dateibrowser:
  Wo diese Spec eine Datei wählen lässt (FR-015), bleibt es bei der Dateiauswahl des Systems, und
  Erweiterungen bekommen weiter nur gewählte Dateien (FR-016, FR-022).

## Clarifications

### Session 2026-10-07

- Q: Wird der Android-Build nur von Hand auf dem Telefon geprüft? → A: Nein. Die CI testet den
  Android-Build zusätzlich automatisch; die Prüfung von Hand auf dem Telefon bleibt für das, was
  sich nicht automatisieren lässt.
- Q: Kann man auf Android einen vorhandenen Tresor aus einer Tresordatei öffnen? → A: Ja. Der
  Hauptweg bleibt das Neuanlegen; das Öffnen einer Tresordatei kommt als zweiter Weg dazu.
- Q: Was passiert mit einer Tresordatei, die auf Android über die Dateiauswahl geöffnet wird?
  → A: holzi übernimmt eine Kopie in den eigenen Speicher; danach ist es ein gewöhnlicher Tresor,
  das Original bleibt unberührt.
- Q: Was sollen die automatischen Android-Tests in der CI abdecken? → A: Die vorhandene
  e2e-Suite des Desktops läuft möglichst vollständig auch im Android-Emulator; ausgenommen sind
  nur Fälle, deren Funktion es auf Android nicht gibt, jeweils mit Begründung.
- Q: Wann soll die Android-e2e-Suite in der CI laufen? → A: Bei jedem Pull Request, als
  Pflicht-Check vor dem Merge.
- Q: Soll holzi auf Android Bildschirmfotos und die Vorschau in der App-Übersicht verhindern?
  → A: Ja, standardmäßig an, solange ein Tresor offen ist; die Person kann es in den
  Einstellungen ausschalten.
- Q: Ist das Entsperren per Fingerabdruck oder Gesichtserkennung Teil dieser Spec? → A: Nein.
  043 entsperrt nur per Passwort; biometrisches Entsperren bekommt eine eigene Spec.
- Q: Gibt es das Übernehmen einer Tresordatei auch am Desktop? → A: Ja. FR-002a gilt auf allen
  Plattformen; am Desktop wählt die Person die Datei im Dateidialog des Systems.
- Q: Welcher Anteil der Desktop-e2e-Fälle muss auch auf Android laufen? → A: Mindestens 80 %,
  Ziel 90 %.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - holzi auf dem Telefon installieren, Tresor öffnen, Passwörter nutzen (Priority: P1)

Die Person lädt das APK aus dem CI-Lauf herunter (oder installiert es per Kabel), startet
holzi auf ihrem Android-Telefon, legt einen neuen Tresor an (der Hauptweg), öffnet eine
vorhandene Tresordatei oder entsperrt einen Tresor, den sie schon auf dem Telefon hat, und
benutzt den Passwortmanager mit dem Finger: Einträge lesen und bearbeiten, zwischen den Tabs
wischen, mehrere Einträge per Langdruck auswählen, Anhänge in der Lightbox ansehen. Der Window
Manager zeigt jedes Fenster bildschirmfüllend; die Zurück-Geste von Android führt im aktiven Tab
zurück. Funktionen, die es auf dem Telefon nicht gibt, sagen das, statt abzustürzen.

**Why this priority**: Ohne laufende App gibt es nichts anderes. Der Passwortmanager ist der
Teil von holzi, der auf dem Telefon den größten Nutzen hat, und seine Telefon-Prüfungen
(Spec 036 T081) warten nur darauf.

**Independent Test**: Auf einem Telefon ohne vorherige holzi-Installation das APK installieren,
einen Tresor anlegen, drei Einträge mit Anhang anlegen, die Prüfungen aus Spec 036 T081
durchgehen, den Tresor schließen und erneut entsperren; die Einträge sind unverändert da.

**Acceptance Scenarios**:

1. **Given** ein Android-Telefon ohne holzi, **When** die Person das APK installiert und
   holzi startet, **Then** erscheint die Tresorauswahl, und sie kann einen neuen Tresor mit
   Passwort anlegen.
2. **Given** eine Tresordatei von holzi im Download-Ordner oder einem Cloud-Speicher,
   **When** die Person in der Tresorauswahl „Tresordatei öffnen“ wählt, die Datei über die
   Dateiauswahl von Android auswählt und das Passwort eingibt, **Then** übernimmt holzi eine
   Kopie in den eigenen Speicher, der Tresor ist offen und erscheint danach in der
   Tresorauswahl; die Originaldatei bleibt unverändert.
3. **Given** ein angelegter Tresor, **When** die Person ihn schließt, **Then** endet die App;
   beim nächsten Start erscheint die Tresorauswahl, und nach dem Entsperren sind alle zuvor
   gespeicherten Daten da.
4. **Given** ein offener Tresor auf einem 360 px breiten Bildschirm, **When** die Person den
   Passwortmanager öffnet, **Then** füllt das Fenster den Bildschirm, nichts ragt seitlich
   hinaus, und keine Bedienelemente liegen unter Statusleiste, Navigationsleiste oder
   Kamera-Aussparung.
5. **Given** ein geöffneter Eintrag, **When** die Person waagrecht wischt, **Then** wechselt
   die Ansicht zwischen Details, Extra und Verlauf; ein Tippen auf einen Tab wechselt ebenso.
6. **Given** die Eintragsliste, **When** die Person eine Zeile lange drückt, **Then** ist die
   Zeile ausgewählt, die Auswahlleiste erscheint, und weitere Zeilen lassen sich per Tippen
   hinzufügen; das Zeilenmenü über den Menüknopf der Zeile bietet dieselben Aktionen wie das
   Kontextmenü am Desktop.
7. **Given** ein offener Tresor mit angezeigtem Passwort, **When** die Person die
   App-Übersicht öffnet oder ein Bildschirmfoto versucht, **Then** zeigt die Vorschau nichts
   vom Inhalt, und das Bildschirmfoto wird verweigert; nach dem Ausschalten in den
   Einstellungen sind beide wieder möglich.
8. **Given** ein Eintrag mit Bildanhängen, **When** die Person eine Anhangkarte antippt,
   **Then** öffnet die Lightbox; Zwei-Finger-Zoom vergrößert, Wischen wechselt das Bild,
   Zurück schließt die Lightbox.
9. **Given** ein Eingabefeld unten im Bildschirm, **When** die Person es antippt, **Then**
   bleibt das Feld über der Bildschirmtastatur sichtbar.
10. **Given** ein Tab mit Zurück-Eintrag, **When** die Person die Android-Zurück-Geste
    ausführt, **Then** geht der Tab einen Schritt zurück; ohne Zurück-Eintrag öffnet sich die
    Fensterübersicht; bei offener Fensterübersicht schließt sie sich; die App wird dabei nie
    verlassen (Spec 020 FR-019).
11. **Given** ein Dialog, der eine Datei erwartet (z. B. Anhang hinzufügen, Import aus
    haex-vault 2.x (Legacy), Hintergrundbild), **When** die Person ihn öffnet, **Then** erscheint die
    Dateiauswahl von Android, und die gewählte Datei wird übernommen, auch wenn sie aus einem
    Cloud-Speicher oder dem Download-Ordner stammt.
12. **Given** eine Funktion ohne Android-Gegenstück (Ordner wählen, Terminal, CLI-Delegate,
    Ordner beobachten, Grafikkarten-Erkennung), **When** die Person oder eine Erweiterung sie
    aufruft, **Then** erscheint der Hinweis „Auf diesem Gerät nicht verfügbar“, und holzi
    läuft weiter.

---

### User Story 2 - APK aus der CI bekommen (Priority: P1)

Die Person, die holzi entwickelt, bekommt zu jedem Pull Request ein installierbares, in der CI
automatisch getestetes APK und für
Releases ein signiertes APK, das sich über eine ältere Version installieren lässt, ohne Daten zu
verlieren.

**Why this priority**: Ohne gebautes APK lässt sich keine der anderen Stufen auf einem Telefon
prüfen; der CI-Job verhindert außerdem, dass Änderungen den Android-Build unbemerkt brechen.

**Independent Test**: Einen Pull Request öffnen, das APK aus dem CI-Lauf herunterladen und auf
einem Telefon installieren; danach ein Release-APK über eine ältere Release-Version
installieren, ohne dass der Tresor verloren geht.

**Acceptance Scenarios**:

1. **Given** ein Pull Request, **When** die CI läuft, **Then** schlägt sie fehl, wenn holzi für
   Android nicht mehr baut, und stellt sonst ein installierbares APK als Artefakt bereit.
2. **Given** ein Release, **When** die CI das Release-APK baut, **Then** ist es mit dem
   Signaturschlüssel von holzi signiert, und dieser Schlüssel liegt nicht im Repository.
3. **Given** ein Pull Request, **When** die CI läuft, **Then** installiert sie das APK auf
   einem Android-Emulator, führt dort die e2e-Suite des Desktops aus und wird rot, wenn ein Fall
   fehlschlägt.
4. **Given** ein installiertes Release-APK, **When** die Person ein neueres Release-APK
   installiert, **Then** ersetzt Android die App, und vorhandene Tresore bleiben erhalten.

---

### User Story 3 - Telefon mit dem Desktop synchronisieren (Priority: P2)

Die Person koppelt ihr Telefon mit ihrem Desktop-holzi als eigenes Gerät. Danach sieht sie auf
dem Telefon dieselben Passwörter, Einstellungen und Dateien wie am Desktop; Änderungen auf einer
Seite erscheinen auf der anderen, solange holzi auf dem Telefon offen ist.

**Why this priority**: Ein Passwortmanager auf dem Telefon ist erst mit den Daten vom Desktop
wirklich nützlich. Der Sync selbst ist gebaut (Specs 024, 025); hier geht es darum, dass er auf
dem Telefon funktioniert.

**Independent Test**: Desktop und Telefon koppeln, auf dem Desktop einen Eintrag anlegen, auf
dem Telefon ändern, auf dem Desktop den geänderten Stand sehen; danach holzi auf dem Telefon in
den Hintergrund schicken, am Desktop ändern, Telefon wieder öffnen: Die Änderung kommt an.

**Acceptance Scenarios**:

1. **Given** ein Desktop mit offenem Tresor und ein Telefon mit holzi, **When** die Person die
   Kopplung wie zwischen zwei Desktops durchführt, **Then** ist das Telefon als eigenes Gerät
   in der Geräteliste beider Seiten.
2. **Given** zwei gekoppelte Geräte, beide im Vordergrund, **When** auf einem ein Eintrag
   geändert wird, **Then** zeigt das andere die Änderung ohne manuelles Neuladen.
3. **Given** holzi auf dem Telefon im Hintergrund oder beendet, **When** am Desktop Daten
   geändert werden und die Person holzi auf dem Telefon wieder öffnet und entsperrt, **Then**
   holt das Telefon die Änderungen nach.
4. **Given** ein Telefon, das zwischen WLAN und Mobilfunk wechselt, **When** der Sync läuft,
   **Then** setzt er nach dem Wechsel ohne Eingriff der Person fort.

---

### User Story 4 - Erweiterungen auf dem Telefon (Priority: P3)

Die Person installiert und benutzt haextensions auf dem Telefon wie am Desktop: Erweiterung aus
einer Datei oder aus dem Netz installieren, Berechtigungen erteilen, die Erweiterung in einem
Fenster benutzen, Benachrichtigungen der Erweiterung erhalten. Eine Erweiterung bekommt auf dem
Telefon keine Rechte, die sie am Desktop nicht hätte.

**Why this priority**: Erweiterungen sind der Weg, auf dem holzi wächst (Spec 017 FR-066 verlangt
Android). Sie setzen die laufende App voraus und sind ohne Sync bereits nutzbar.

**Independent Test**: Auf dem Telefon eine Erweiterung aus einer Datei installieren, eine
Berechtigung erteilen, die Erweiterung benutzen; mit einer Prüf-Erweiterung bestätigen, dass ihr
Rahmen weder die interne Schnittstelle von holzi noch deren Schlüssel sieht.

**Acceptance Scenarios**:

1. **Given** eine Erweiterungsdatei im Download-Ordner, **When** die Person sie über die
   Installation auswählt, **Then** wird die Erweiterung wie am Desktop geprüft, die
   Berechtigungen werden angezeigt, und nach Zustimmung ist sie installiert.
2. **Given** eine installierte Erweiterung, **When** ihr Rahmen versucht, die interne
   Schnittstelle von holzi zu erreichen, **Then** ist sie dort nicht vorhanden, und ein Aufruf
   scheitert (Spec 017 FR-066, T112).
3. **Given** eine Erweiterung mit Dateiberechtigung, **When** sie eine Datei öffnen oder
   speichern lässt, **Then** wählt die Person die Datei über die Dateiauswahl von Android, und
   die Erweiterung erhält nur diese Datei.
4. **Given** eine Erweiterung, die eine Benachrichtigung sendet, **When** die Person der
   Benachrichtigungsberechtigung von Android zugestimmt hat, **Then** erscheint die
   Benachrichtigung im System; hat sie abgelehnt, erscheint die Nachricht nur innerhalb von
   holzi, und nichts stürzt ab.
5. **Given** eine Erweiterung, die Terminal oder Ordnerbeobachtung verlangt, **When** sie die
   Funktion aufruft, **Then** antwortet holzi „nicht verfügbar“ (Spec 017 FR-066).

---

### User Story 5 - Chat mit Online-Anbietern auf dem Telefon (Priority: P4)

Die Person richtet einen Online-Anbieter mit API-Schlüssel ein (oder übernimmt ihn per Sync vom
Desktop) und chattet auf dem Telefon. Verbindungen zu Anbietern sind verschlüsselt und werden
wie bei anderen Android-Apps auf gültige Zertifikate geprüft.

**Why this priority**: Chat ist ein Kernteil von holzi, auf dem Telefon aber ohne lokales
Modell nur über Online-Anbieter möglich; er braucht nur die laufende App und funktionierende
verschlüsselte Verbindungen.

**Independent Test**: Auf dem Telefon einen Anbieter mit API-Schlüssel einrichten, eine Frage
stellen, eine Antwort erhalten; einen Anbieter mit absichtlich ungültigem Zertifikat ansprechen
und eine verständliche Fehlermeldung sehen.

**Acceptance Scenarios**:

1. **Given** ein eingerichteter Online-Anbieter, **When** die Person eine Nachricht sendet,
   **Then** erscheint die Antwort gestreamt wie am Desktop.
2. **Given** ein Server mit ungültigem oder abgelaufenem Zertifikat, **When** holzi ihn
   ansprechen soll, **Then** lehnt holzi die Verbindung ab und zeigt, dass das Zertifikat nicht
   gültig ist.
3. **Given** die Modellauswahl im Chat, **When** die Person sie öffnet, **Then** sind
   CLI-Delegates (claude, codex) nicht wählbar oder als „auf diesem Gerät nicht verfügbar“
   gekennzeichnet.
4. **Given** ein laufender Chat, **When** die Verbindung abbricht, **Then** zeigt holzi den
   Abbruch und erlaubt das erneute Senden.

---

### User Story 6 - Lokale KI und Spracheingabe auf dem Telefon (Priority: P5)

Die Person lädt ein für Telefone geeignetes lokales Modell herunter und chattet ohne
Internetverbindung; sie diktiert Nachrichten per Spracheingabe. holzi schlägt ein Modell vor,
das zum Arbeitsspeicher des Telefons passt, und nennt vor jedem Download die Größe.

**Why this priority**: Lokale KI ist der aufwändigste Teil (Rechenleistung, Speicher,
Downloadgrößen) und für die übrigen Stufen nicht nötig.

**Independent Test**: Auf einem Telefon mit mindestens 6 GB Arbeitsspeicher das vorgeschlagene
Modell laden, Flugmodus einschalten, eine Frage stellen und eine Antwort erhalten; per
Spracheingabe einen Satz diktieren und ihn im Eingabefeld sehen.

**Acceptance Scenarios**:

1. **Given** die Modellauswahl auf dem Telefon, **When** die Person ein lokales Modell
   herunterladen will, **Then** sieht sie vorher die Downloadgröße und bestätigt den Download.
2. **Given** ein Telefon mit wenig Arbeitsspeicher, **When** holzi ein lokales Modell
   vorschlägt, **Then** ist es die kleinere Telefon-Voreinstellung (ADR 0002), nicht das
   Desktop-Modell.
3. **Given** ein geladenes lokales Modell und keine Netzverbindung, **When** die Person eine
   Nachricht sendet, **Then** antwortet das Modell.
4. **Given** die Spracheingabe zum ersten Mal, **When** die Person sie startet, **Then** fragt
   Android nach der Mikrofonberechtigung; bei Zustimmung wird diktiert, bei Ablehnung erklärt
   holzi, wie die Berechtigung später erteilt werden kann.
5. **Given** ein Modell, das für den freien Speicher zu groß ist, **When** die Person den
   Download startet, **Then** warnt holzi vorher und startet ihn nicht stillschweigend.

---

### Edge Cases

- **Android beendet holzi im Hintergrund**: Die Tresor-Sitzung endet; beim nächsten Öffnen
  erscheint die Tresorauswahl, und nichts, was vorher gespeichert war, fehlt. Ungespeicherte
  Eingaben in einem Formular dürfen verloren gehen.
- **Person wischt holzi aus der App-Übersicht weg**: Wie Schließen; die Sitzung endet sauber,
  der Tresor ist danach nicht als „anderswo geöffnet“ gesperrt.
- **holzi wird ein zweites Mal gestartet** (z. B. über eine Verknüpfung, während es läuft):
  Android bringt die laufende App nach vorn; ein zweiter Tresor öffnet sich nicht.
- **Bildschirm wird gedreht oder Fenstergröße ändert sich** (geteilter Bildschirm, faltbares
  Telefon): Die Sitzung bleibt bestehen, der Kompaktmodus folgt der neuen Breite, nichts wird
  neu entsperrt.
- **Gewählte Datei ist keine Tresordatei von holzi oder das Passwort ist falsch**: Die
  Tresorauswahl meldet das verständlich; an der Datei ändert sich nichts.
- **Die Tresordatei ist eine Kopie eines Tresors, den es auf diesem Gerät schon gibt**: holzi
  lehnt sie ab und sagt, dass der Tresor schon da ist; zwei Kopien desselben Tresors auf einem
  Gerät würden sich beim Sync als dasselbe Gerät ausgeben.
- **In der Tresorauswahl gibt es schon einen Tresor mit demselben Namen**: holzi schlägt einen
  anderen Namen vor; der vorhandene Tresor wird nie überschrieben.
- **Dateiauswahl abgebrochen**: Der Dialog bleibt unverändert offen, keine Fehlermeldung.
- **Datei aus der Dateiauswahl ist nicht mehr lesbar** (Cloud-Datei offline, Berechtigung
  entzogen): Verständliche Meldung, kein Absturz.
- **Telefon ohne Netz beim Start**: Tresor anlegen, entsperren und Passwortmanager funktionieren
  vollständig; Sync und Online-Chat zeigen ihren Offline-Zustand.
- **Wenig freier Speicher**: Tresor anlegen oder Anhang speichern scheitert mit verständlicher
  Meldung, ohne den Tresor zu beschädigen.
- **Ältere Android-Version unter der Mindestversion**: Android verweigert die Installation;
  holzi muss dafür nichts tun.
- **Berechtigung später in den Android-Einstellungen entzogen** (Mikrofon, Benachrichtigungen):
  Die Funktion meldet beim nächsten Gebrauch, dass die Berechtigung fehlt.

## Requirements _(mandatory)_

### Functional Requirements

#### App, Tresor und Prozessmodell

- **FR-001**: holzi MUSS als Android-App auf Telefonen mit 64-Bit-ARM-Prozessor installierbar
  und startbar sein; für Emulatoren MUSS es zusätzlich eine x86_64-Variante geben.
- **FR-002**: Nach dem Start MUSS die Tresorauswahl erscheinen; die Person MUSS dort einen
  Tresor anlegen und einen vorhandenen entsperren können. Das Anlegen ist der hervorgehobene
  Hauptweg.
- **FR-002a**: Die Person MUSS in der Tresorauswahl eine Tresordatei von holzi über die
  Dateiauswahl des Systems (auf Android die Dateiauswahl von Android, am Desktop der
  Dateidialog) wählen und mit ihrem Passwort öffnen können; das gilt auf Android und am
  Desktop. holzi MUSS dabei eine
  Kopie in den eigenen Speicher übernehmen und DARF die Originaldatei weder ändern noch löschen;
  danach MUSS der Tresor als gewöhnlicher Tresor in der Tresorauswahl erscheinen. Eine Kopie,
  die nicht vollständig übernommen oder nicht entsperrt werden konnte, DARF keinen Rest im
  Speicher von holzi hinterlassen.
- **FR-003**: Tresore MÜSSEN im privaten Speicher der App liegen; andere Apps DÜRFEN sie
  nicht lesen können.
- **FR-004**: Alles, was holzi am Desktop dauerhaft speichert (Tresor, Einstellungen,
  Anhänge, Modelle), MUSS auf Android ein Beenden der App und einen Neustart des Telefons
  überstehen.
- **FR-005**: Wie in Spec 013 MUSS ein Prozess höchstens eine Tresor-Sitzung halten.
- **FR-006**: Schließt die Person den Tresor, MUSS die App enden; beim nächsten Start MUSS die
  Tresorauswahl erscheinen.
- **FR-007**: Beendet Android die App (im Hintergrund, durch Wegwischen oder Speichermangel),
  MUSS der Tresor danach ohne Hinweis „anderswo geöffnet“ wieder entsperrbar sein.
- **FR-008**: Ein erneuter Start, während holzi läuft, MUSS die laufende App nach vorn holen
  und DARF keine zweite Tresor-Sitzung öffnen.
- **FR-009**: Drehen, geteilter Bildschirm und Größenänderungen DÜRFEN die Tresor-Sitzung
  nicht beenden.

#### Oberfläche auf dem Telefon

- **FR-010**: Unterhalb der Kompakt-Schwelle MUSS der Window Manager (wm) Fenster
  bildschirmfüllend zeigen (Spec 015 FR-028); das MUSS auf Telefonen ab 360 px Breite ohne
  waagrechtes Scrollen der Seite funktionieren.
- **FR-011**: Bedienelemente DÜRFEN NICHT unter Statusleiste, Navigationsleiste oder
  Kamera-Aussparung liegen.
- **FR-011a**: Solange ein Tresor offen ist, MUSS holzi auf Android standardmäßig Bildschirmfotos
  und Bildschirmaufnahmen verhindern und in der App-Übersicht von Android ein leeres
  Vorschaubild zeigen. Die Person MUSS das in den Einstellungen ausschalten können; die Wahl
  gilt nur für dieses Gerät und wird ohne Speichern-Knopf sofort wirksam.
- **FR-012**: Ein fokussiertes Eingabefeld MUSS sichtbar bleiben, wenn die Bildschirmtastatur
  erscheint.
- **FR-013**: Die Android-Zurück-Geste MUSS sich wie in Spec 020 FR-019 verhalten und DARF die
  App nie verlassen.
- **FR-014**: Die Telefon-Bedienung des Passwortmanagers aus Spec 036 MUSS auf einem echten
  Telefon funktionieren: Wischen zwischen den Tabs eines Eintrags, Langdruck-Auswahl in der
  Liste, Zeilenmenü über einen Knopf in der Zeile, Lightbox mit Zwei-Finger-Zoom und Wischen.
- **FR-015**: Wo die Person am Desktop eine Datei aus dem Dateisystem wählt oder speichert,
  MUSS sie auf Android die Dateiauswahl des Systems bekommen, und holzi MUSS Dateien aus allen
  dort angebotenen Quellen (lokaler Speicher, Download-Ordner, Cloud-Anbieter) lesen und dorthin
  speichern können. Das betrifft mindestens: Anhänge im Passwortmanager und im Chat, Import aus
  haex-vault 2.x, Hintergrundbild und Darstellung importieren und exportieren, Erweiterung aus Datei
  installieren.
- **FR-016**: Funktionen ohne Android-Gegenstück MÜSSEN „Auf diesem Gerät nicht verfügbar“
  melden, statt abzustürzen oder still nichts zu tun: Ordner wählen, Schreibtisch-Ordner und
  andere Desktop-Ordner, Terminal, Ordner beobachten, CLI-Delegates, Erkennung der Grafikkarte,
  freie Dateipfade für Erweiterungen. Wo sich so eine Funktion in der Oberfläche anbietet, MUSS
  sie auf Android ausgeblendet oder als nicht verfügbar gekennzeichnet sein.

#### Sync

- **FR-017**: Ein Telefon MUSS sich mit denselben Schritten wie ein Desktop als eigenes Gerät
  koppeln lassen (Spec 024) und Tresor-Daten und Dateien (Spec 025) synchronisieren.
- **FR-018**: Das Telefon MUSS synchronisieren, solange holzi im Vordergrund ist, und beim
  Zurückkehren in den Vordergrund verpasste Änderungen nachholen; im Hintergrund DARF es nicht
  synchronisieren.
- **FR-019**: Ein Wechsel des Netzes (WLAN, Mobilfunk) MUSS den Sync ohne Eingriff der Person
  fortsetzen.

#### Erweiterungen

- **FR-020**: Erweiterungen MÜSSEN sich auf Android mit denselben Prüfungen und Berechtigungen
  installieren und benutzen lassen wie am Desktop (Spec 017 FR-066).
- **FR-021**: Kein Erweiterungsrahmen DARF auf Android die interne Schnittstelle von holzi oder
  deren Schlüssel sehen oder aufrufen können; das MUSS auf einem Android-Gerät oder Emulator
  nachgewiesen sein (Spec 017 T112).
- **FR-022**: Dateizugriff einer Erweiterung MUSS auf Android über die Dateiauswahl des Systems
  laufen und DARF der Erweiterung nur die gewählten Dateien geben.
- **FR-023**: Benachrichtigungen von Erweiterungen MÜSSEN als Android-Benachrichtigungen
  erscheinen, wenn die Person der Berechtigung zugestimmt hat; sonst MÜSSEN sie innerhalb von
  holzi sichtbar bleiben.

#### Verschlüsselte Verbindungen und Chat

- **FR-024**: Jede verschlüsselte Verbindung, die holzi auf Android aufbaut (Online-Anbieter,
  Modelldownloads, Mail, Speicher-Verbindungen, Sync-Dienste), MUSS das Zertifikat der
  Gegenstelle gegen die vom System vertrauten Zertifizierungsstellen prüfen und bei ungültigem
  Zertifikat abbrechen.
- **FR-025**: Chat mit Online-Anbietern MUSS auf Android wie am Desktop funktionieren,
  einschließlich gestreamter Antworten und Werkzeugaufrufe, die auf Android verfügbar sind.
- **FR-026**: CLI-Delegates MÜSSEN auf Android als nicht verfügbar gekennzeichnet sein; per Sync
  übernommene Delegate-Einstellungen DÜRFEN auf dem Telefon keinen Fehler auslösen.

#### Lokale KI und Sprache

- **FR-027**: Lokale Modelle MÜSSEN auf dem Telefon laufen; holzi MUSS die Telefon-
  Voreinstellungen aus ADR 0002 anbieten und abhängig vom Arbeitsspeicher des Telefons ein
  passendes Modell vorschlagen.
- **FR-028**: Vor jedem Modelldownload MUSS holzi die Größe nennen und eine Bestätigung
  verlangen; reicht der freie Speicher nicht, MUSS holzi vorher warnen.
- **FR-029**: Die Spracheingabe MUSS auf dem Telefon funktionieren; die Mikrofonberechtigung
  MUSS beim ersten Gebrauch angefragt werden, und eine Ablehnung MUSS verständlich erklärt
  werden.
- **FR-030**: Auf dem Telefon MUSS standardmäßig das kleinste Spracherkennungsmodell des
  Katalogs vorgeschlagen werden (Spec 008 FR-011); größere bleiben wählbar.

#### Bauen und Verteilen

- **FR-031**: Die CI MUSS bei jedem Pull Request holzi für Android bauen und bei einem
  Fehlschlag rot werden.
- **FR-032**: Die CI MUSS ein installierbares APK als Artefakt des Laufs bereitstellen.
- **FR-033**: Release-APKs MÜSSEN mit einem eigenen Signaturschlüssel signiert sein, der nicht
  im Repository liegt; aufeinanderfolgende Releases MÜSSEN denselben Schlüssel tragen, damit
  ein Update die installierte App ersetzt.
- **FR-033a**: Die CI MUSS bei jedem Pull Request das APK auf einem Android-Emulator
  installieren und die vorhandene e2e-Suite des Desktops dort ausführen; schlägt ein Fall fehl,
  MUSS der Lauf rot werden. Dieser Lauf MUSS ein Pflicht-Check vor dem Merge sein.
- **FR-033b**: Ein e2e-Fall DARF auf Android nur ausgenommen werden, wenn seine Funktion es auf
  Android nicht gibt (FR-016) oder sein Ablauf dort anders festgelegt ist (FR-006: Schließen
  beendet die App); jede Ausnahme MUSS mit Begründung an einer Stelle aufgeführt
  sein. Für jede Ausnahme MUSS es einen Android-Fall geben, der das „nicht verfügbar“ prüft.
- **FR-033c**: Neue e2e-Fälle MÜSSEN ab dieser Spec auf Desktop und Android laufen, außer sie
  fallen unter FR-033b.
- **FR-034**: Wer holzi entwickelt, MUSS holzi im Entwicklungsmodus auf einem angeschlossenen
  Telefon oder Emulator starten können, wobei Änderungen an der Oberfläche ohne neues APK
  sichtbar werden.

### Key Entities

- **Android-Build**: Das installierbare Paket von holzi für Android, in einer Variante für
  Telefone (64-Bit-ARM) und einer für Emulatoren (x86_64); als Prüf-Build pro Pull Request oder
  als signiertes Release.
- **Signaturschlüssel**: Der Schlüssel, mit dem Release-Builds signiert werden; liegt
  außerhalb des Repositorys und ist über alle Releases gleich.
- **Telefon als eigenes Gerät**: Ein Gerät im Sinne von Spec 024 mit der Eigenschaft „nur im
  Vordergrund erreichbar“.
- **Gewählte Datei**: Eine Datei, die die Person über die Dateiauswahl von Android freigibt;
  holzi und Erweiterungen erhalten nur diese Datei, keinen Pfad im Dateisystem.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Auf einem Mittelklasse-Telefon erscheint die Tresorauswahl höchstens 3 Sekunden
  nach dem Antippen des App-Symbols.
- **SC-002**: Das Entsperren eines Tresors dauert auf dem Telefon höchstens doppelt so lang wie
  am Desktop derselben Person und nie länger als 10 Sekunden.
- **SC-003**: Alle Telefon-Prüfungen aus Spec 036 T081 und alle Akzeptanzszenarien von User
  Story 1 bestehen auf einem echten Telefon bei 360 px Breite.
- **SC-004**: In einer Stunde normaler Nutzung (Tresor, Passwortmanager, Sync, Chat) stürzt
  holzi kein einziges Mal ab, auch nicht beim Aufruf von Funktionen ohne Android-Gegenstück.
- **SC-005**: Eine Änderung am Desktop ist auf dem gekoppelten Telefon im Vordergrund innerhalb
  von 10 Sekunden sichtbar und umgekehrt.
- **SC-006**: Eine Prüf-Erweiterung findet auf Android in keinem ihrer Rahmen die interne
  Schnittstelle von holzi.
- **SC-007**: Jeder Pull Request zeigt nach dem CI-Lauf ein installierbares APK und das
  Ergebnis der e2e-Suite auf Android; mindestens 80 % der e2e-Fälle des Desktops laufen auch auf
  Android (Ziel: 90 %), der Rest steht begründet in der Ausnahmeliste; ein Bruch des Android-Builds oder seines Verhaltens
  fällt im selben Pull Request auf, nicht erst danach.
- **SC-008**: Ein Update von einem Release-APK auf das nächste behält 100 % der Tresore und
  Einstellungen.
- **SC-009**: Auf einem Telefon mit mindestens 6 GB Arbeitsspeicher beginnt das vorgeschlagene
  lokale Modell innerhalb von 15 Sekunden nach dem Senden mit der Antwort.

## Assumptions

- Mindestversion ist Android 8.0 (API 26): Die Spracheingabe nimmt über AAudio auf, das es erst ab
  Android 8.0 gibt (das Tauri-Gerüst gibt Android 7.0 vor). Geprüft wird auf dem Telefon der Person
  und auf einem aktuellen Emulator.
- Der Build läuft in der Nix-devShell mit den gepinnten atoms-Dateien
  ([`haexmas/atoms` @ `b662c6205d6a1ebd7fac588291f82ea8e78c3d52`](https://github.com/haexmas/atoms/tree/b662c6205d6a1ebd7fac588291f82ea8e78c3d52)); die CI darf eine eigene
  Android-Umgebung aufsetzen, solange sie dieselben Versionen von SDK, NDK und Rust benutzt.
- Ein vorhandener Desktop-Tresor kommt vorzugsweise per Sync auf das Telefon (User Story 3);
  das Öffnen einer Tresordatei (FR-002a) ist der zweite Weg.
- Die Prüf-Builds der Pull Requests sind mit dem Debug-Schlüssel signiert; sie lassen sich
  nicht über ein Release-APK installieren und umgekehrt. Das ist so gewollt.
- Den Signaturschlüssel erzeugt und verwahrt die Person, die holzi betreut; die CI bekommt ihn
  als Geheimnis (Constitution I).
- Was sich nicht im Emulator prüfen lässt (echte Gesten wie Zwei-Finger-Zoom, Kamera-
  Aussparung, Wechsel zwischen WLAN und Mobilfunk), prüft die Person von Hand auf dem Telefon
  nach der Quickstart-Anleitung des Plans.
- Welche Technik lokale Modelle auf Android ausführt (der Desktop-Lader oder das in ADR 0002
  geplante native Backend), entscheidet der Plan; die Spec verlangt nur, dass die
  Telefon-Voreinstellungen laufen.
- Spec 036 T081 und Spec 017 T112 werden mit dieser Spec abgehakt.

## Nicht im Umfang

- iOS (eigene Spec).
- Veröffentlichung in F-Droid oder Google Play.
- Synchronisation im Hintergrund.
- Steuerung von Downloads und Sync zum Schonen des Datenvolumens (Idee in `plans/README.md`).
- Tablet-spezifische Darstellung über den Kompaktmodus und die normale Darstellung hinaus.
- Portabler Modus (Spec 014).
- Autofill von Passwörtern in anderen Android-Apps.
- Entsperren per Fingerabdruck oder Gesichtserkennung (eigene Spec, auch für den Desktop).
