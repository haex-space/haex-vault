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
4. In KI → Agenten → Claude Code: Rechtegruppe Browser an, „Alle Websites“ auf „Keine“ (M2 braucht
   die Rückfrage, FR-043), `https://127.0.0.1:8443` mit „bedienen“.

## M1 Lesen (US1)

- Claude Code bitten: „Öffne https://127.0.0.1:8443/ und fasse die Nachrichten zusammen.“
- Erwartet: Ein Browserfenster öffnet sich, der Tab zeigt die Markierung „Ein Agent steuert diesen
  Tab · Claude Code“, KI → Browser listet die Sitzung, die Antwort nennt die Überschriften.

## M2 Fremde Website (US2)

- „Öffne https://localhost:8443/partner.html.“ (eine zweite Website) oder `/framed.html`, das einen
  Rahmen dieser Website enthält
- Erwartet: Rückfrage vor dem Laden mit „Erlauben“, „Ablehnen“ und dem Häkchen „Für diese
  Website merken“ (mit Wahl „Lesen“ oder „Bedienen“); „Ablehnen“ → der Agent meldet `site_not_allowed`, die Seite
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

## M10 Dialogs stay open (FR-042)

- Open a question of the agent browser (M3), send holzi to the background, bring it back with a
  click into the window, click next to the dialog, press Escape: the dialog stays and nothing is
  declined. There is no X; only "Erlauben" and "Ablehnen" answer. The same with a permission
  question in the chat, the registration of a new agent (Spec 021, quickstart M1) and an approval
  of the browser extension (049).

## M11 All websites and page requests (FR-043, FR-044)

- Switch the browser rights group on anew: "Alle Websites: Bedienen" is set, any website opens
  without a question. A bank with "Lesen" as an exception: no clicks there.
- A page that sends something on its own ("Merken" on `/form.html` sends `POST /api`): the
  question "Anfrage der Seite senden?" with the website, the request and its content (nested JSON
  as text, secrets as `•••`). "Ablehnen" with
  "Für diese Website merken": the same request is no longer asked but refused (`request_denied`);
  the rule is listed in "KI → Browser" and can be removed.

## M12 Trackers (FR-045)

- First start with an empty cache: the browser waits at most 10 seconds for the lists. A page with
  a known tracker (for example `https://bam.nr-data.net`) loads, the tracker is blocked and never
  asks. In "KI → Browser": the default lists of uBlock Origin with the time of their download or the error of the last one; add an
  own list by address, switch it off, remove it. A list whose address is not `https` is refused.

## M13 Viewing the profile (FR-046)

- "KI → Browser → Profil" after a login on a test page: the website with its cookies, the value
  only after a click. Change value and expiry, delete one cookie and then a whole website, then
  let the page reload: the change holds. The agent sees none of it.

## M14 Block list (FR-047)

- In the rights list of an agent: Browser at "Read and change", add a website under the
  exceptions: it shows as blocked. The agent opens another website, then the blocked one: it
  answers `site_blocked`, also by a link and a redirect. Set the website to "Read": it opens.
  In "KI → Browser" the same for the built-in agent; changing the dropdown saves without an error.

## M15 DNS over HTTPS (FR-048)

- "KI → Browser → Sicheres DNS": the default is "Automatisch" with Quad9. Choose "Streng" with an
  own address that does not answer (`https://127.0.0.1:9/dns-query`), stop and start the agent
  browser (the next browser tool starts it): no page loads, the agent sees the load error. Set it
  back to "Automatisch" with the same address: pages load through the system's resolver. Choose
  "Streng" with Quad9, then Cloudflare: pages load. An `http` address is refused.
- The change applies at the next start; a running browser keeps what it started with, and the page
  says so. With Chrome, Edge or Brave as the chosen browser the same two steps (unreachable
  provider in "Streng" fails, in "Automatisch" loads) give the same result.

## M16 Keys, hover, the wheel and players (US7, FR-049 to FR-055)

- Start the fixtures (`pnpm agent-browser:fixtures`), an agent with "operate" for
  `https://127.0.0.1:8443`. "Open /hover.html, open the menu 'Konto' and choose 'Abmelden'": the
  agent hovers, the entries get numbers, the click works (the same menu inside a shadow root comes
  with N2). "/wheel.html: load the whole list": the last wheel call says the page did not move.
- "/keys.html: select all text of the note and replace it": `Ctrl+A` then typing in `keys` mode works. "Press
  Ctrl+V", "press F12", "press Alt+Left": each is refused with `key_not_allowed`; no developer tools
  open and the page does not go back. Focus the submit button and press Enter: the question of a
  click on it appears.
- "/canvas-keys.html: move the square to the right edge" with the arrow keys. "/player.html: play
  the sound, seek to 5 s, pause": the element list shows the state each time; nothing is heard.
- "/fullscreen.html: click the video's fullscreen button": the window stays as it is, the marker
  stays, the agent's next result says `fullscreen_refused`.
- With "read" for the fixture website: hover and the wheel work; a key, typing and the player are refused.

## M17 Seeing and pointing (US8, FR-056 to FR-063)

- With a model that accepts images: "/canvas.html: click the red circle, then drag the blue square
  onto the grey target, then press 'Jetzt kaufen'". The agent takes a screenshot with `marks`,
  clicks the circle by position (the result says no element was hit), drags the square, and the
  button asks as before. Scroll the page by hand between the screenshot and a click: the click is
  refused with `view_changed` and nothing happens in the page.
- Ask for a region of the small chart: the image is enlarged and the result names region and zoom.
- With a local model without images the screenshot and the position tools are not offered; with
  Claude Code through MCP they are.

## M18 Stop and take over during input (FR-062)

- While a drag runs (a fixture that makes it slow), press "Stoppen" in holzi: within a second the
  call ends, no mouse button stays held (the page shows `mouseup`). Move the mouse in the page
  while the agent presses keys: the agent pauses as for a click.

## M9 Eigener Browser (US6, zurückgestellt am 2026-11-01)

- Erweiterung nach 049 zugelassen, Recht „Browser des Nutzers“ an. Einen Tab in der Erweiterung
  „dem Agenten übergeben“ → der Agent liest ihn; einen anderen Tab sieht er nicht
  (`browser.tabs.list`); Absenden fragt auch in Auto; „Zurücknehmen“ beendet die Steuerung.
