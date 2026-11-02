# Quickstart: Browser-Werkzeuge für Agenten prüfen

**Feature**: [spec.md](./spec.md) | **Verträge**: [contracts/](./contracts/)

Manuelle Prüfungen auf einem Desktop (Linux, macOS, Windows) mit einem installierten
Chromium-basierten Browser. Die automatischen Tests stehen in tasks.md; hier steht, was ein Mensch
einmal je Lieferung durchgeht.

## Vorbereitung

1. holzi mit einer Test-Vault starten, KI → Agenten: Zugang an, Claude Code nach Anleitung
   verbinden und registrieren (Spec 021, quickstart M1).
2. Lokalen Testserver mit den Seiten aus `src-tauri/tests/fixtures/agent_browser/site/` starten
   (`pnpm agent-browser:fixtures`, lauscht auf `https://localhost:8443` mit Testzertifikat).
3. In KI → Agenten → Claude Code: Rechtegruppe Browser an, `https://localhost:8443` mit „bedienen“.

## M1 Lesen (US1)

- Claude Code bitten: „Öffne https://localhost:8443/news und fasse die Überschriften zusammen.“
- Erwartet: Ein Browserfenster öffnet sich, der Tab zeigt die Markierung „Ein Agent steuert diesen
  Tab · Claude Code“, KI → Browser listet die Sitzung, die Antwort nennt die Überschriften.

## M2 Fremde Website (US2)

- „Folge dem Link ‚Partner‘.“ (führt auf `https://partner.localhost:8444`)
- Erwartet: Rückfrage vor dem Laden mit „Einmal erlauben“, „Immer erlauben (lesen)“, „Immer
  erlauben (bedienen)“, „Ablehnen“; „Ablehnen“ → der Agent meldet `site_not_allowed`, die Seite
  lädt nicht.

## M3 Formular und Absenden (US2)

- Freigabestufe Auto. „Fülle das Kontaktformular mit Testdaten aus und schicke es ab.“
- Erwartet: Felder werden ohne Rückfrage gefüllt; vor dem Abschicken Rückfrage mit Website,
  Schaltfläche „Absenden“ und den Feldern; „Ablehnen“ → nichts kommt beim Testserver an (Log des
  Testservers).
- Seite `/js-checkout` (Schaltfläche ohne sprechenden Namen, Skript sendet POST): Rückfrage nach
  Stufe 2 aus R5.

## M4 Eingreifen und Stoppen (US3)

- Während der Agent eine längere Aufgabe ausführt: „Eingreifen“ in der Markierung → der Agent
  meldet „pausiert“; selbst tippen; „Zurückgeben“ → der Agent macht weiter.
- „Stoppen“ in holzi → die Markierung verschwindet sofort, der Tab bleibt offen, der nächste
  Aufruf des Agenten meldet `stopped`.

## M5 Prompt Injection (US2, SC-002)

- `/injection/1` … `/injection/20` nacheinander lesen lassen.
- Erwartet: keine Rückfrage wird übersprungen, keine fremde Website ohne Rückfrage, kein Recht
  geändert; das Protokoll zeigt nur die erwarteten Aufrufe.

## M6 Anmelden (US4)

- Eintrag „Test-Login“ für `https://localhost:8443` im Passwortmanager, Rechtegruppe Passwörter für
  den Eintrag. „Melde dich auf /login an.“
- Erwartet: Rückfrage „Anmeldedaten eintragen“ mit „Für diese Website merken“; nach Erlauben ist das
  Formular gefüllt; im Protokoll und in der Antwort des Agenten steht kein Passwort; ein
  Bildschirmfoto vor dem Seitenwechsel wird mit `screenshot_blocked` abgelehnt. Zweites Mal mit
  „gemerkt“: keine Rückfrage.

## M7 Download (US5)

- „Lade die Rechnung von /invoice herunter.“ → Datei in `Downloads/Agenten-Downloads/<Vault>`;
  `/big` (150 MB) → Rückfrage „Großen Download behalten?“ mit Größe; „Ablehnen“ → keine Datei.
- „Lade ~/Dokumente/Beleg.pdf in das Formular hoch.“ ohne Recht „Dateien des Geräts“ → abgelehnt,
  gleich ob die Datei existiert; mit Recht → Rückfrage wie Absenden, die Datei ist gewählt, gesendet
  wird erst mit „Hochladen“ und eigener Rückfrage.

## M8 Profil (FR-001)

- Nach M6 holzi beenden. Unter `agent-browser/` im Datenverzeichnis liegt nur ein verschlüsselter
  Ordner (`*.hxef`), kein lesbares Cookie-Verzeichnis; `$XDG_RUNTIME_DIR/holzi-agent-browser/`
  (Linux) bzw. das Temp-Verzeichnis ist leer.
- Ein Agent meldet sich mit „Anmeldedaten eintragen“ an; der Agenten-Browser bietet nicht an, das
  Passwort zu speichern (FR-002).
- holzi starten, Vault entsperren, `/account` öffnen lassen → noch angemeldet.
- Dieselbe Vault auf einem zweiten Gerät: Agenten-Browser startet ohne Anmeldung.

## M6a Daten aus einem Eintrag (US4, FR-037)

- Eintrag „Girokonto“ mit eigenem Feld `IBAN` (Wert mit Erkennungswert), Agent mit der
  Rechtegruppe Passwörter dafür und „bedienen“ für die Fixture-Website. „Bezahle die Bestellung
  auf /checkout per Lastschrift.“
- Erwartet: Rückfrage „Daten eintragen“ mit Zweck, Eintragsauswahl und Zuordnung `IBAN ← IBAN`
  (Wert verdeckt); nach „Eintragen“ steht die IBAN im Formular; der Agent nennt nur gefüllte
  Elemente; „Bestellen“ fragt vor dem Absenden und zeigt `•••`; ein zweiter Aufruf fragt wieder.

## M9 Eigener Browser (US6, zurückgestellt am 2026-11-01)

- Erweiterung nach 049 zugelassen, Recht „Browser des Nutzers“ an. Einen Tab in der Erweiterung
  „dem Agenten übergeben“ → der Agent liest ihn; einen anderen Tab sieht er nicht
  (`browser.tabs.list`); Absenden fragt auch in Auto; „Zurücknehmen“ beendet die Steuerung.
