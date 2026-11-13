//! The report of importing passes as plain text for a file the user saves (spec 092, FR-032): for
//! each pass its title, its place, what was not kept and how many codes are kept as text. A title
//! and names are shown, never the value of a code or any other secret.

use super::import_model::PassImportReport;
use super::pkpass::{WHY_NOT_SUPPORTED, WHY_OVER_LIMIT, WHY_UPDATE_SERVICE};

/// The reports as text, in the order they were made.
pub fn render_text(reports: &[PassImportReport]) -> String {
    let mut text = format!("Pass import report\n\nImported passes: {}\n", reports.len());
    for report in reports {
        text.push_str(&format!("\n- {}\n", report.title));
        let place = if report.folder_path.is_empty() {
            "top of Wallet".to_owned()
        } else {
            report.folder_path.join(" / ")
        };
        text.push_str(&format!("  Place: {place}\n"));
        if let Some(tag) = &report.tag_name {
            text.push_str(&format!("  Tag: {tag}\n"));
        }
        if report.flagged_codes > 0 {
            text.push_str(&format!(
                "  Codes kept as text because they did not fit their format: {}\n",
                report.flagged_codes
            ));
        }
        for item in &report.not_kept {
            let why = match item.why.as_str() {
                WHY_UPDATE_SERVICE => "the update service is never called",
                WHY_NOT_SUPPORTED => "not used by holzi",
                WHY_OVER_LIMIT => "more than holzi keeps",
                _ => "could not be read",
            };
            text.push_str(&format!("  Not kept: {} ({why})\n", item.what));
        }
    }
    text
}
