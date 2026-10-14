use super::extension_offer::{matched_offer, MatchEntry, MAX_MATCHED};

/// (extension title, [(tool, German title, description, German example)]).
type Fake = (
    &'static str,
    &'static [(&'static str, &'static str, &'static str, &'static str)],
);

/// 30 invented extensions with two to four tools each.
const FAKES: [Fake; 30] = [
    (
        "Haex Mail",
        &[
            (
                "count_unread",
                "Ungelesene Mails zählen",
                "Counts the unread mails in the inbox",
                "Wie viele ungelesene Mails habe ich?",
            ),
            (
                "search_mail",
                "Mails suchen",
                "Searches mails by sender or subject",
                "Suche die Mail von Anna über die Rechnung",
            ),
            (
                "archive_mail",
                "Mail archivieren",
                "Moves a mail to the archive",
                "Archiviere die letzte Mail von Tom",
            ),
        ],
    ),
    (
        "Kalender",
        &[
            (
                "list_events",
                "Termine anzeigen",
                "Lists the events of a day",
                "Welche Termine habe ich morgen?",
            ),
            (
                "add_event",
                "Termin eintragen",
                "Adds an event to the calendar",
                "Trag einen Termin beim Zahnarzt am Freitag ein",
            ),
        ],
    ),
    (
        "Notizen",
        &[
            (
                "add_note",
                "Notiz anlegen",
                "Creates a note",
                "Notiere dir, dass ich Brot kaufen muss",
            ),
            (
                "find_note",
                "Notiz finden",
                "Finds notes by text",
                "Finde meine Notiz zum Urlaub",
            ),
        ],
    ),
    (
        "Wetter",
        &[
            (
                "forecast",
                "Wettervorhersage",
                "Weather forecast for a place",
                "Wird es morgen in Berlin regnen?",
            ),
            (
                "current_weather",
                "Aktuelles Wetter",
                "Current weather at a place",
                "Wie warm ist es gerade draußen?",
            ),
        ],
    ),
    (
        "Aufgaben",
        &[
            (
                "add_task",
                "Aufgabe hinzufügen",
                "Adds a task to a list",
                "Setz Rasen mähen auf meine Aufgabenliste",
            ),
            (
                "open_tasks",
                "Offene Aufgaben",
                "Lists open tasks",
                "Was steht auf meiner Aufgabenliste?",
            ),
            (
                "complete_task",
                "Aufgabe erledigen",
                "Marks a task as done",
                "Hake die Aufgabe Steuererklärung ab",
            ),
        ],
    ),
    (
        "Kontakte",
        &[
            (
                "find_contact",
                "Kontakt finden",
                "Finds a contact by name",
                "Wie ist die Telefonnummer von Paul?",
            ),
            (
                "add_contact",
                "Kontakt anlegen",
                "Adds a contact",
                "Speichere Lisa mit der Nummer 0176 als Kontakt",
            ),
        ],
    ),
    (
        "Fotos",
        &[
            (
                "search_photos",
                "Fotos suchen",
                "Searches photos by place or date",
                "Zeig mir die Fotos vom Strand",
            ),
            (
                "count_photos",
                "Fotos zählen",
                "Counts photos in an album",
                "Wie viele Fotos sind im Album Hochzeit?",
            ),
        ],
    ),
    (
        "Musik",
        &[
            (
                "play_song",
                "Lied abspielen",
                "Plays a song",
                "Spiel das Lied Bohemian Rhapsody",
            ),
            (
                "pause_music",
                "Musik pausieren",
                "Pauses the music",
                "Halt die Musik an",
            ),
            (
                "next_song",
                "Nächstes Lied",
                "Skips to the next song",
                "Spring zum nächsten Lied",
            ),
        ],
    ),
    (
        "Rezepte",
        &[
            (
                "find_recipe",
                "Rezept finden",
                "Finds recipes by ingredient",
                "Was kann ich mit Zucchini kochen?",
            ),
            (
                "shopping_list",
                "Einkaufsliste",
                "Ingredients of a recipe as a shopping list",
                "Was muss ich für Lasagne einkaufen?",
            ),
        ],
    ),
    (
        "Finanzen",
        &[
            (
                "account_balance",
                "Kontostand",
                "Balance of a bank account",
                "Wie hoch ist mein Kontostand?",
            ),
            (
                "monthly_spending",
                "Ausgaben des Monats",
                "Spending of the current month by category",
                "Wie viel Geld habe ich diesen Monat für Essen ausgegeben?",
            ),
        ],
    ),
    (
        "Fitness",
        &[
            (
                "steps_today",
                "Schritte heute",
                "Steps walked today",
                "Wie viele Schritte bin ich heute gegangen?",
            ),
            (
                "log_workout",
                "Training eintragen",
                "Logs a workout",
                "Trag 30 Minuten Joggen als Training ein",
            ),
        ],
    ),
    (
        "Lesezeichen",
        &[
            (
                "add_bookmark",
                "Lesezeichen setzen",
                "Saves a web page as bookmark",
                "Speichere diese Seite als Lesezeichen",
            ),
            (
                "find_bookmark",
                "Lesezeichen finden",
                "Finds bookmarks",
                "Finde mein Lesezeichen zu Rust",
            ),
        ],
    ),
    (
        "Reisen",
        &[
            (
                "find_train",
                "Zugverbindung",
                "Finds train connections",
                "Wann fährt der nächste Zug nach Hamburg?",
            ),
            (
                "trip_plan",
                "Reiseplan",
                "Shows the plan of a trip",
                "Was steht für meine Reise nach Rom an?",
            ),
        ],
    ),
    (
        "Haushalt",
        &[
            (
                "cleaning_plan",
                "Putzplan",
                "Shows who cleans what this week",
                "Wer ist diese Woche mit Bad putzen dran?",
            ),
            (
                "add_chore",
                "Hausarbeit eintragen",
                "Adds a chore to the plan",
                "Trag Fenster putzen für Samstag ein",
            ),
        ],
    ),
    (
        "Pflanzen",
        &[
            (
                "water_plants",
                "Pflanzen gießen",
                "Which plants need water today",
                "Welche Pflanzen muss ich heute gießen?",
            ),
            (
                "add_plant",
                "Pflanze hinzufügen",
                "Adds a plant",
                "Füge meine neue Monstera hinzu",
            ),
        ],
    ),
    (
        "Bücher",
        &[
            (
                "reading_list",
                "Leseliste",
                "Lists books to read",
                "Welche Bücher stehen auf meiner Leseliste?",
            ),
            (
                "add_book",
                "Buch hinzufügen",
                "Adds a book to the reading list",
                "Setz Der Name des Windes auf meine Leseliste",
            ),
        ],
    ),
    (
        "Filme",
        &[
            (
                "watchlist",
                "Merkliste",
                "Movies on the watchlist",
                "Welche Filme wollte ich noch sehen?",
            ),
            (
                "rate_movie",
                "Film bewerten",
                "Rates a movie",
                "Gib dem Film Dune fünf Sterne",
            ),
        ],
    ),
    (
        "Paketverfolgung",
        &[
            (
                "track_parcel",
                "Paket verfolgen",
                "Shows where a parcel is",
                "Wo ist mein Paket von Zalando?",
            ),
            (
                "parcels",
                "Pakete",
                "Lists expected parcels",
                "Welche Pakete kommen diese Woche?",
            ),
        ],
    ),
    (
        "Passwörter",
        &[
            (
                "weak_passwords",
                "Schwache Passwörter",
                "Finds weak passwords",
                "Welche meiner Passwörter sind unsicher?",
            ),
            (
                "password_age",
                "Alter der Passwörter",
                "Passwords older than a year",
                "Welche Passwörter habe ich lange nicht geändert?",
            ),
        ],
    ),
    (
        "Smart Home",
        &[
            (
                "light_on",
                "Licht einschalten",
                "Switches a light on",
                "Mach das Licht im Wohnzimmer an",
            ),
            (
                "thermostat",
                "Heizung einstellen",
                "Sets the heating",
                "Stell die Heizung im Bad auf 22 Grad",
            ),
            (
                "door_status",
                "Türstatus",
                "Whether doors are locked",
                "Ist die Haustür abgeschlossen?",
            ),
        ],
    ),
    (
        "Nachrichten",
        &[
            (
                "headlines",
                "Schlagzeilen",
                "Today's headlines",
                "Was sind die Schlagzeilen heute?",
            ),
            (
                "topic_news",
                "Nachrichten zu einem Thema",
                "News about a topic",
                "Gibt es Neuigkeiten zur Bundestagswahl?",
            ),
        ],
    ),
    (
        "Zeiterfassung",
        &[
            (
                "start_timer",
                "Zeit starten",
                "Starts tracking time for a project",
                "Starte die Zeiterfassung für Projekt Holzi",
            ),
            (
                "hours_week",
                "Stunden der Woche",
                "Hours tracked this week",
                "Wie viele Stunden habe ich diese Woche gearbeitet?",
            ),
        ],
    ),
    (
        "Vokabeln",
        &[
            (
                "practice",
                "Vokabeln üben",
                "Starts a vocabulary practice",
                "Lass uns Spanisch Vokabeln üben",
            ),
            (
                "add_word",
                "Vokabel hinzufügen",
                "Adds a word to learn",
                "Füge das Wort perro mit Bedeutung Hund hinzu",
            ),
        ],
    ),
    (
        "Medikamente",
        &[
            (
                "next_dose",
                "Nächste Einnahme",
                "When the next dose is due",
                "Wann muss ich die nächste Tablette nehmen?",
            ),
            (
                "add_medication",
                "Medikament eintragen",
                "Adds a medication schedule",
                "Trag Ibuprofen dreimal täglich ein",
            ),
        ],
    ),
    (
        "Geburtstage",
        &[
            (
                "upcoming_birthdays",
                "Nächste Geburtstage",
                "Upcoming birthdays",
                "Wer hat als Nächstes Geburtstag?",
            ),
            (
                "gift_ideas",
                "Geschenkideen",
                "Gift ideas noted for a person",
                "Was wollte ich Mama zum Geburtstag schenken?",
            ),
        ],
    ),
    (
        "Auto",
        &[
            (
                "fuel_log",
                "Tankbuch",
                "Logs a refuel",
                "Ich habe für 60 Euro getankt",
            ),
            (
                "service_due",
                "Nächste Inspektion",
                "When the car service is due",
                "Wann muss das Auto zur Inspektion?",
            ),
        ],
    ),
    (
        "Abos",
        &[
            (
                "subscriptions",
                "Abos anzeigen",
                "Lists paid subscriptions",
                "Welche Abos bezahle ich gerade?",
            ),
            (
                "cancel_hint",
                "Kündigungsfrist",
                "Notice period of a subscription",
                "Bis wann kann ich Netflix kündigen?",
            ),
        ],
    ),
    (
        "Dokumente",
        &[
            (
                "find_document",
                "Dokument finden",
                "Finds scanned documents",
                "Wo ist mein Mietvertrag?",
            ),
            (
                "expiring_documents",
                "Ablaufende Dokumente",
                "Documents that expire soon",
                "Wann läuft mein Reisepass ab?",
            ),
        ],
    ),
    (
        "Spiele",
        &[
            (
                "play_time",
                "Spielzeit",
                "Time played this week",
                "Wie lange habe ich diese Woche gespielt?",
            ),
            (
                "game_backlog",
                "Spieleliste",
                "Games still to play",
                "Welche Spiele wollte ich noch spielen?",
            ),
        ],
    ),
    (
        "Test-Erweiterung",
        &[
            (
                "count_entries",
                "Einträge zählen",
                "Counts the entries",
                "Wie viele Einträge habe ich?",
            ),
            (
                "add_entry",
                "Eintrag hinzufügen",
                "Adds an entry",
                "Füge einen Eintrag Milch hinzu",
            ),
            (
                "slow_echo",
                "Langsames Echo",
                "Echoes text slowly",
                "Gib ein langsames Echo von hallo",
            ),
        ],
    ),
];

fn entries() -> Vec<MatchEntry> {
    FAKES
        .iter()
        .flat_map(|(title, tools)| {
            tools
                .iter()
                .map(move |(name, title_de, description, example)| MatchEntry {
                    tool_name: format!("x_{}_{name}", title.to_lowercase().replace([' ', '-'], "")),
                    extension_title: (*title).to_owned(),
                    texts: vec![
                        (*title_de).to_owned(),
                        (*description).to_owned(),
                        (*example).to_owned(),
                    ],
                })
        })
        .collect()
}

#[test]
fn a_question_without_the_extensions_name_finds_its_tool() {
    let offer = matched_offer("Wie viele ungelesene Mails habe ich?", &entries());
    assert_eq!(
        offer.first().map(String::as_str),
        Some("x_haexmail_count_unread")
    );
    assert!(offer.len() <= MAX_MATCHED);
}

#[test]
fn the_test_extensions_question_finds_its_count_tool() {
    let offer = matched_offer("Wie viele Einträge habe ich?", &entries());
    assert_eq!(
        offer.first().map(String::as_str),
        Some("x_testerweiterung_count_entries")
    );
}

#[test]
fn smalltalk_finds_nothing() {
    for message in [
        "Hallo, wie geht es dir?",
        "Danke dir!",
        "Erzähl mir einen Witz",
    ] {
        assert!(matched_offer(message, &entries()).is_empty(), "{message}");
    }
}

#[test]
fn stop_words_alone_never_hit() {
    let offer = matched_offer("Wie habe ich das? Was ist es denn?", &entries());
    assert!(offer.is_empty(), "{offer:?}");
}

#[test]
fn naming_the_extension_offers_its_tools() {
    let offer = matched_offer("Was gibt es Neues in der Test-Erweiterung?", &entries());
    assert!(!offer.is_empty());
    assert!(
        offer
            .iter()
            .all(|name| name.starts_with("x_testerweiterung_")),
        "{offer:?}"
    );
}

#[test]
fn the_order_is_the_same_every_time() {
    let message = "Welche Termine und Aufgaben habe ich morgen?";
    let first = matched_offer(message, &entries());
    let mut reversed = entries();
    reversed.reverse();
    assert_eq!(first, matched_offer(message, &reversed));
}

#[test]
fn umlauts_are_part_of_words() {
    let offer = matched_offer("Welche Pflanzen muss ich gießen?", &entries());
    assert_eq!(
        offer.first().map(String::as_str),
        Some("x_pflanzen_water_plants")
    );
}

#[test]
fn no_tools_no_offer() {
    assert!(matched_offer("Wie viele ungelesene Mails habe ich?", &[]).is_empty());
}
