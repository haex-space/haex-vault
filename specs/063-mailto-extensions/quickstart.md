# Quickstart: Extensions als Öffner für E-Mail-Links

Von Hand zu prüfen (Aufgabe T025). Vorbereitung: eine Vault mit einem Kontakt, der
eine E-Mail-Adresse hat, und haex-mail (neue Version) oder die Testextension `mail-opener.xt` installiert.

1. **Ohne Öffner**: keine Extension mit `openers`. Mail-Knopf beim Kontakt tippen: das Mail-Programm des
   Systems geht auf, keine Auswahl (FR-001).
2. **Mit Öffner**: haex-mail (oder die Testextension) installieren und aktivieren. Mail-Knopf tippen: die
   Auswahl zeigt „Mit dem System öffnen“ und „Mail“ (FR-002). Vor dem Aktivieren zeigt die
   Installationsansicht „kann E-Mail-Links öffnen“ (FR-014).
3. **Öffner wählen**: „Mail“ wählen: das Fenster der Extension kommt nach vorn (oder startet) und zeigt den
   Schreib-Dialog mit der Adresse; nichts ist gesendet (FR-003, FR-015). Auswahl schließen ohne Wahl: nichts.
4. **System wählen**: „Mit dem System öffnen“: das Mail-Programm des Systems geht auf.
5. **Deaktivieren**: die Extension deaktivieren: der Knopf öffnet wieder ohne Auswahl (FR-012).
6. **Link von außen (Android)**: im Browser oder in einer anderen App auf einen `mailto:`-Link tippen, holzi
   als Programm wählen: der Dialog mit „Mail“ und „Link kopieren“, nie „Mit dem System öffnen“; „Mail“
   zeigt Empfänger, Kopie, Betreff und Text (FR-017, FR-018).
7. **Gesperrt**: dasselbe bei gesperrter Vault: nach dem Entsperren erscheint der Dialog. Zwei Links kurz
   nacheinander: es erscheint der zweite.
8. **Link von außen (Linux)**: `xdg-open 'mailto:ben@example.org?subject=Hallo%20Welt'` mit installiertem
   Paket: holzi geht auf und zeigt den Dialog.
9. **Kaputte Links**: `mailto:a@b?subject=x%0d%0aBcc:c@d` und ein Link mit 51 Empfängern: der Dialog sagt, dass
   der Link nicht gelesen werden kann, und zeigt keine Zeilen.
10. **Kein Standard**: nach dem Start von holzi ist das Standard-Mailprogramm des Systems unverändert (FR-016).
