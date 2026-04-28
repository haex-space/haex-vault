// src-tauri/generator/event_names.rs
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

pub fn generate_event_names() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR ist nicht gesetzt.");
    println!("Generiere Event-Namen nach {out_dir}");
    let events_path = Path::new("../src/constants/eventNames.json");
    let dest_path = Path::new(&out_dir).join("eventNames.rs");

    let file = File::open(events_path).expect("Konnte eventNames.json nicht öffnen");
    let reader = BufReader::new(file);
    let categories: HashMap<String, HashMap<String, String>> =
        serde_json::from_reader(reader).expect("Konnte eventNames.json nicht parsen");

    let mut code = String::from(
        r#"
// ==================================================================
// HINWEIS: Diese Datei wurde automatisch von build.rs generiert.
// Manuelle Änderungen werden bei der nächsten Kompilierung überschrieben!
// ==================================================================

"#,
    );

    // Sort categories for stable output
    let mut sorted_categories: Vec<_> = categories.iter().collect();
    sorted_categories.sort_by_key(|(k, _)| k.as_str());

    for (category, events) in sorted_categories {
        let category_prefix = to_screaming_snake_case(category);
        code.push_str(&format!("// --- {category} Events ---\n"));

        let mut sorted_events: Vec<_> = events.iter().collect();
        sorted_events.sort_by_key(|(k, _)| k.as_str());

        for (key, value) in sorted_events {
            let const_name = format!(
                "EVENT_{}_{}",
                category_prefix,
                to_screaming_snake_case(key)
            );
            code.push_str(&format!("pub const {const_name}: &str = \"{value}\";\n"));
        }
        code.push('\n');
    }

    let mut f = File::create(&dest_path).expect("Konnte Zieldatei nicht erstellen");
    f.write_all(code.as_bytes())
        .expect("Konnte nicht in Zieldatei schreiben");

    println!("cargo:rerun-if-changed=../src/constants/eventNames.json");
}

/// Konvertiert einen camelCase oder PascalCase String zu SCREAMING_SNAKE_CASE
fn to_screaming_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut prev_is_lower = false;

    for (i, ch) in s.chars().enumerate() {
        if ch == '_' {
            result.push('_');
            prev_is_lower = false;
        } else if ch.is_uppercase() {
            if i > 0 && prev_is_lower {
                result.push('_');
            }
            result.push(ch);
            prev_is_lower = false;
        } else {
            result.push(ch.to_ascii_uppercase());
            prev_is_lower = true;
        }
    }

    result
}
