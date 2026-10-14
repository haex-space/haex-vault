//! The kinds of numbers, addresses and other values on the phone (`TYPE` and `LABEL` of a data row)
//! and holzi's labels (data-model.md). A phone type without its own holzi label becomes a label of
//! the user's own with the type's English name; written back, such a label is the phone's custom
//! type. The phone's type is written only when holzi's label changed (FR-022), so a type without a
//! holzi label survives every other change.

use crate::contacts::kinds::{ADDRESS, EMAIL, EVENT, IM, PHONE, RELATION, SIP, WEBSITE};

/// Android's `BaseTypes.TYPE_CUSTOM`.
pub const CUSTOM_TYPE: i64 = 0;

/// The types of one kind with their holzi label; a label with `custom:` has no label of its own.
fn table(kind: &str) -> &'static [(i64, &'static str)] {
    match kind {
        PHONE => &[
            (1, "home"),
            (2, "mobile"),
            (3, "work"),
            (4, "fax_work"),
            (5, "fax_home"),
            (6, "pager"),
            (7, "other"),
            (8, "custom:Callback"),
            (9, "car"),
            (10, "company_main"),
            (11, "custom:ISDN"),
            (12, "main"),
            (13, "custom:Other fax"),
            (14, "custom:Radio"),
            (15, "custom:Telex"),
            (16, "custom:TTY/TDD"),
            (17, "custom:Work mobile"),
            (18, "custom:Work pager"),
            (19, "custom:Assistant"),
            (20, "custom:MMS"),
        ],
        EMAIL => &[(1, "home"), (2, "work"), (3, "other"), (4, "mobile")],
        ADDRESS | SIP | IM => &[(1, "home"), (2, "work"), (3, "other")],
        WEBSITE => &[
            (1, "custom:Homepage"),
            (2, "custom:Blog"),
            (3, "custom:Profile"),
            (4, "home"),
            (5, "work"),
            (6, "custom:FTP"),
            (7, "other"),
        ],
        EVENT => &[(1, "anniversary"), (2, "other"), (3, "birthday")],
        RELATION => &[
            (1, "assistant"),
            (2, "brother"),
            (3, "child"),
            (4, "domestic_partner"),
            (5, "father"),
            (6, "friend"),
            (7, "manager"),
            (8, "mother"),
            (9, "parent"),
            (10, "partner"),
            (11, "referred_by"),
            (12, "relative"),
            (13, "sister"),
            (14, "spouse"),
        ],
        _ => &[],
    }
}

/// holzi's label for a row's `TYPE` and `LABEL`.
pub fn from_phone(kind: &str, phone_type: Option<&str>, label: Option<&str>) -> String {
    let phone_type = phone_type.and_then(|text| text.trim().parse::<i64>().ok());
    let own = label.map(str::trim).filter(|text| !text.is_empty());
    match phone_type {
        Some(CUSTOM_TYPE) => match own {
            Some(own) => format!("custom:{own}"),
            None => "other".to_owned(),
        },
        Some(code) => table(kind)
            .iter()
            .find(|(candidate, _)| *candidate == code)
            .map_or_else(|| "other".to_owned(), |(_, label)| (*label).to_owned()),
        None => "other".to_owned(),
    }
}

/// The `TYPE` and `LABEL` a holzi label is written as.
pub fn to_phone(kind: &str, label: &str) -> (i64, Option<String>) {
    let rows = table(kind);
    if let Some((code, _)) = rows.iter().find(|(_, candidate)| *candidate == label) {
        return (*code, None);
    }
    if let Some(own) = label.strip_prefix("custom:") {
        return (CUSTOM_TYPE, Some(own.to_owned()));
    }
    let other = rows
        .iter()
        .find(|(_, candidate)| *candidate == "other")
        .map_or(CUSTOM_TYPE, |(code, _)| *code);
    if other == CUSTOM_TYPE {
        (CUSTOM_TYPE, Some(label.to_owned()))
    } else {
        (other, None)
    }
}

/// The names of Android's messenger protocols (`Im.PROTOCOL_*`), by code.
const PROTOCOLS: [(i64, &str); 9] = [
    (0, "AIM"),
    (1, "MSN"),
    (2, "Yahoo"),
    (3, "Skype"),
    (4, "QQ"),
    (5, "Google Talk"),
    (6, "ICQ"),
    (7, "Jabber"),
    (8, "NetMeeting"),
];

/// Android's `Im.PROTOCOL_CUSTOM`; holzi writes every service with it.
pub const CUSTOM_PROTOCOL: i64 = -1;

/// The name of a messenger service from `PROTOCOL` and `CUSTOM_PROTOCOL`.
pub fn service(protocol: Option<&str>, custom: Option<&str>) -> Option<String> {
    let custom = custom.map(str::trim).filter(|text| !text.is_empty());
    match protocol.and_then(|text| text.trim().parse::<i64>().ok()) {
        Some(CUSTOM_PROTOCOL) | None => custom.map(str::to_owned),
        Some(code) => PROTOCOLS
            .iter()
            .find(|(candidate, _)| *candidate == code)
            .map(|(_, name)| (*name).to_owned())
            .or_else(|| custom.map(str::to_owned)),
    }
}

#[cfg(test)]
#[path = "labels_tests.rs"]
mod tests;
