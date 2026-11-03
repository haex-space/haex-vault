# Quickstart: Browser-Werkzeuge für Agenten prüfen

**Feature**: [spec.md](./spec.md) | **Verträge**: [contracts/](./contracts/)

Manuelle Prüfungen auf einem Desktop (Linux, macOS, Windows) mit einem installierten
Chromium-basierten Browser. Die automatischen Tests stehen in tasks.md; hier steht, was ein Mensch
einmal je Lieferung durchgeht.

## Vorbereitung

1. holzi mit einer Test-Vault starten, KI → Agenten: Zugang an, Claude Code nach Anleitung
   verbinden und registrieren (Spec 021, quickstart M1).
2. Lokalen Testserver mit den Seiten aus `src-tauri/tests/fixtures/agent_browser/site/` starten:
   `pnpm agent-browser:fixtures` in der nix-Umgebung (anderer Port: `pnpm agent-browser:fixtures
9443`). Derselbe Server wie in den Integrationstests, über HTTPS mit dem Testzertifikat:
   `https://127.0.0.1:8443` ist die Website, `https://localhost:8443` eine zweite (der Partner);
   jede Anfrage steht mit Host und Methode im Terminal.
3. Das Testzertifikat ist nicht vertrauenswürdig. Beim ersten Aufruf jeder der beiden Websites
   zeigt der Agenten-Browser eine Warnung: mit „Eingreifen“ den Tab übernehmen, „Erweitert → Weiter
   zu …“ wählen und zurückgeben. Das Zertifikat nicht als Zertifizierungsstelle importieren: sein
   Schlüssel liegt im Repository.
4. In KI → Agenten → Claude Code: Rechtegruppe Browser an, `https://127.0.0.1:8443` mit „bedienen“.

## M1 Lesen (US1)

- Claude Code bitten: „Öffne https://127.0.0.1:8443/ und fasse die Nachrichten zusammen.“
- Erwartet: Ein Browserfenster öffnet sich, der Tab zeigt die Markierung „Ein Agent steuert diesen
  Tab · Claude Code“, KI → Browser listet die Sitzung, die Antwort nennt die Überschriften.

## M2 Fremde Website (US2)

- „Öffne https://localhost:8443/partner.html.“ (eine zweite Website) oder `/framed.html`, das einen
  Rahmen dieser Website enthält
- Erwartet: Rückfrage vor dem Laden mit „Einmal erlauben“, „Immer erlauben (lesen)“, „Immer
  erlauben (bedienen)“, „Ablehnen“; „Ablehnen“ → der Agent meldet `site_not_allowed`, die Seite
  lädt nicht.

## M3 Formular und Absenden (US2)

- Freigabestufe Auto. „Fülle auf /form.html die Bestellung mit Testdaten aus und schicke sie ab.“
- Erwartet: Felder werden ohne Rückfrage gefüllt; vor dem Abschicken Rückfrage mit Website,
  Schaltfläche „Absenden“ und den Feldern; „Ablehnen“ → nichts kommt beim Testserver an (Log des
  Testservers).
- Auf `/form.html` sendet ein Skript ohne Formular `POST /api`: Rückfrage nach Stufe 2 aus R5.

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

- Eintrag „Test-Login“ für `https://127.0.0.1:8443` im Passwortmanager, Rechtegruppe Passwörter
  für den Eintrag. „Melde dich auf /login.html an.“ Die Seite zeigt im Text, was eingetragen wurde.
- Erwartet: Rückfrage „Anmeldedaten eintragen“ mit „Für diese Website merken“; nach Erlauben ist das
  Formular gefüllt; im Protokoll und in der Antwort des Agenten steht kein Passwort; ein
  Bildschirmfoto vor dem Seitenwechsel wird mit `screenshot_blocked` abgelehnt. Zweites Mal mit
  „gemerkt“: keine Rückfrage.

## M7 Download (US5)

- Auf `/files.html`: „Lade die Rechnung herunter.“ (`/invoice.txt`) → Datei in
  `Downloads/HaexVault/<Vault>`; „Lade die große Datei herunter.“ (`/big.bin`, 150 MB) → Rückfrage
  „Großen Download behalten?“ mit Größe; „Ablehnen“ → keine Datei.
- „Lade ~/Dokumente/Beleg.pdf in das Formular hoch.“ ohne Recht „Dateien des Geräts“ → abgelehnt,
  gleich ob die Datei existiert; mit Recht → Rückfrage wie Absenden, die Datei ist gewählt, gesendet
  wird erst mit „Hochladen“ und eigener Rückfrage.

## M8 Profil (FR-001)

- Nach M6 holzi beenden. Unter `agent-browser/` im Datenverzeichnis liegt nur ein verschlüsselter
  Ordner (`*.hxef`), kein lesbares Cookie-Verzeichnis; `$XDG_RUNTIME_DIR/holzi-agent-browser/`
  (Linux) bzw. das Temp-Verzeichnis ist leer.
- Ein Agent meldet sich mit „Anmeldedaten eintragen“ an; der Agenten-Browser bietet nicht an, das
  Passwort zu speichern (FR-002).
- holzi starten, Vault entsperren, die Seite erneut öffnen lassen → noch angemeldet (mit einer
  echten Website mit Anmeldung; die Testseiten führen keine Sitzung).
- Dieselbe Vault auf einem zweiten Gerät: Agenten-Browser startet ohne Anmeldung.

## M6b Neue Anmeldedaten (US4, FR-036)

- Rechtegruppe Passwörter mit „Lesen und Schreiben“; mit Tags muss „Tag für neue Einträge“ gewählt
  sein. „Lege auf /signup.html ein Konto für anna@example.org an.“
- Erwartet: Rückfrage „Neue Anmeldedaten speichern?“ mit dem Benutzernamen; „Neues Passwort“ und
  „Passwort wiederholen“ sind gleich gefüllt („gleich“ im Text der Seite, das Passwort selbst als
  `•••` für den Agenten); „Speichern“ → Eintrag mit der Website im Passwortmanager; „Ablehnen“ →
  beide Felder leer („leer“). Nur mit „Lesen“: `no_write_rights`, keine Rückfrage.
- Dasselbe mit dem eingebauten Agenten über KI → Browser → Passwort-Rechte.

## M6a Daten aus einem Eintrag (US4, FR-037)

- Eintrag „Girokonto“ mit eigenen Feldern `IBAN` und `Kartennummer` (Werte mit Erkennungswert),
  Agent mit der Rechtegruppe Passwörter dafür und „bedienen“ für `https://127.0.0.1:8443`,
  „lesen“ für den Partner `https://localhost:8443`. „Bezahle die Bestellung auf /checkout.html per
  Lastschrift.“
- Erwartet: Rückfrage „Daten eintragen“ mit Zweck, Eintragsauswahl und Zuordnung `IBAN ← IBAN`
  (Wert verdeckt); nach „Eintragen“ steht die IBAN im Formular; der Agent nennt nur gefüllte
  Elemente; „Bestellen“ fragt vor dem Absenden und zeigt `•••`; ein zweiter Aufruf fragt wieder.
- Die Kartennummer liegt in einem Rahmen des Partners: solange er nur gelesen werden darf, fehlt
  ihre Zeile. Mit „bedienen“ für den Partner erscheint sie mit „(Rahmen von https://localhost:8443)“;
  nach „Eintragen“ meldet die Seite „Karte: gefüllt“, der Agent bekommt `in_frames: 1`.

## M10 Dialoge bleiben offen (FR-042)

- Eine Rückfrage des Agenten-Browsers öffnen (M3), holzi in den Hintergrund legen, zurückholen mit
  einem Klick ins Fenster, neben den Dialog klicken, Escape drücken: der Dialog bleibt, nichts wird
  abgelehnt. Es gibt kein X; nur „Erlauben“ und „Ablehnen“ antworten. Dasselbe mit einer
  Berechtigungsfrage im Chat.

## M11 Alle Websites und Anfragen der Seite (FR-043, FR-044)

- Browser-Recht neu einschalten: „Alle Websites: Bedienen“ steht, eine beliebige Website öffnet
  ohne Rückfrage. Eine Bank mit „Lesen“ als Ausnahme: dort kein Klicken.
- Eine Seite, die selbst etwas sendet (`/xhr.html` der Testseiten, Anfrage an die Partnerseite):
  Rückfrage „Anfrage der Seite“ mit dem Inhalt (verschachteltes JSON als Text, `password` als
  `•••`). „Ablehnen“ mit „Für diese Website merken“: dieselbe Anfrage wird nicht mehr gefragt,
  sondern abgelehnt; die Regel steht in „KI → Browser“ und lässt sich entfernen.

## M12 Tracker (FR-045)

- Erster Start mit leerem Cache: der Browser wartet höchstens 10 Sekunden auf die Listen. Eine
  Seite mit bekanntem Tracker (zum Beispiel `https://bam.nr-data.net`) lädt, der Tracker wird
  geblockt und fragt nie. In „KI → Browser“: die Standardlisten von uBlock Origin mit Zeitpunkt und
  Zahl der Regeln; eine eigene Liste per Adresse hinzufügen, abschalten, entfernen. Eine Liste mit
  Adresse ohne `https` wird abgelehnt.

## M13 Profil ansehen (FR-046)

- „KI → Browser → Profil“ nach einer Anmeldung auf einer Testseite: die Website mit ihren Cookies,
  der Wert erst nach Klick. Wert und Ablauf ändern, ein Cookie und dann eine ganze Website löschen,
  dann die Seite neu laden lassen: die Änderung gilt. Der Agent sieht davon nichts.

## M9 Eigener Browser (US6, zurückgestellt am 2026-11-01)

- Erweiterung nach 049 zugelassen, Recht „Browser des Nutzers“ an. Einen Tab in der Erweiterung
  „dem Agenten übergeben“ → der Agent liest ihn; einen anderen Tab sieht er nicht
  (`browser.tabs.list`); Absenden fragt auch in Auto; „Zurücknehmen“ beendet die Steuerung.
