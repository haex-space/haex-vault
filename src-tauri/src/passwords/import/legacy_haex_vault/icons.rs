//! The icon names haex-vault 2.x (Legacy) stores (spec 037, research R8) as pictures of holzi. haex-vault 2.x keeps
//! Iconify names in three spellings: `i-lucide-<name>` (its KeePass import, the defaults of folders
//! and the trash), `mdi:<name>` (its icon picker, `editor/iconPicker.vue`) and the short names of
//! the old haex-pass extension (`useIconComponents.ts`), all at haex-vault 2.x @ `8dce379`. Every
//! picture holzi may end up with is in [`TARGETS`], and each of those is a literal of
//! `src/lib/passwords/icons.ts` (checked by a test), so the build bundles it.

/// The Lucide names a haex-vault 2.x icon can become.
const TARGETS: &[&str] = &[
    "lucide:app-window",
    "lucide:apple",
    "lucide:award",
    "lucide:banknote",
    "lucide:bike",
    "lucide:bitcoin",
    "lucide:bluetooth",
    "lucide:book-open",
    "lucide:bookmark",
    "lucide:briefcase",
    "lucide:bus",
    "lucide:calculator",
    "lucide:calendar",
    "lucide:camera",
    "lucide:car",
    "lucide:circle-check",
    "lucide:circle-user",
    "lucide:clipboard",
    "lucide:clipboard-check",
    "lucide:clock",
    "lucide:cloud",
    "lucide:code",
    "lucide:contact",
    "lucide:credit-card",
    "lucide:database",
    "lucide:dollar-sign",
    "lucide:facebook",
    "lucide:feather",
    "lucide:file-archive",
    "lucide:file-check",
    "lucide:file-lock",
    "lucide:file-pen",
    "lucide:file-plus",
    "lucide:file-text",
    "lucide:files",
    "lucide:fingerprint",
    "lucide:folder",
    "lucide:folder-archive",
    "lucide:folder-check",
    "lucide:folder-key",
    "lucide:folder-lock",
    "lucide:folder-open",
    "lucide:gamepad-2",
    "lucide:gift",
    "lucide:github",
    "lucide:gitlab",
    "lucide:globe",
    "lucide:hard-drive",
    "lucide:headphones",
    "lucide:heart",
    "lucide:house",
    "lucide:id-card",
    "lucide:image",
    "lucide:instagram",
    "lucide:key",
    "lucide:key-round",
    "lucide:key-square",
    "lucide:landmark",
    "lucide:laptop",
    "lucide:lightbulb",
    "lucide:linkedin",
    "lucide:list",
    "lucide:lock",
    "lucide:lock-open",
    "lucide:mail",
    "lucide:mailbox",
    "lucide:map-pin",
    "lucide:message-circle",
    "lucide:message-square",
    "lucide:monitor",
    "lucide:music",
    "lucide:network",
    "lucide:nfc",
    "lucide:notebook",
    "lucide:notebook-text",
    "lucide:package",
    "lucide:pen",
    "lucide:phone",
    "lucide:piggy-bank",
    "lucide:plane",
    "lucide:plug",
    "lucide:printer",
    "lucide:puzzle",
    "lucide:rocket",
    "lucide:scan",
    "lucide:screen-share",
    "lucide:server",
    "lucide:settings",
    "lucide:shield",
    "lucide:shield-check",
    "lucide:shopping-bag",
    "lucide:shopping-cart",
    "lucide:smartphone",
    "lucide:star",
    "lucide:sticky-note",
    "lucide:store",
    "lucide:tablet",
    "lucide:tag",
    "lucide:terminal",
    "lucide:ticket",
    "lucide:timer",
    "lucide:train-front",
    "lucide:trash-2",
    "lucide:triangle-alert",
    "lucide:tv",
    "lucide:twitter",
    "lucide:user",
    "lucide:user-key",
    "lucide:users",
    "lucide:video",
    "lucide:wallet",
    "lucide:wifi",
    "lucide:wrench",
    "lucide:youtube",
    "lucide:zap",
];

/// Lucide names haex-vault 2.x uses that Lucide has since renamed.
const LUCIDE_RENAMED: &[(&str, &str)] = &[
    ("home", "house"),
    ("alert-triangle", "triangle-alert"),
    ("check-circle", "circle-check"),
];

/// The names of the haex-vault 2.x icon picker and its KeePass import.
const MDI: &[(&str, &str)] = &[
    ("account", "lucide:user"),
    ("account-circle", "lucide:circle-user"),
    ("account-group", "lucide:users"),
    ("airplane", "lucide:plane"),
    ("apple", "lucide:apple"),
    ("application", "lucide:app-window"),
    ("bank", "lucide:landmark"),
    ("bike", "lucide:bike"),
    ("bitcoin", "lucide:bitcoin"),
    ("bookmark", "lucide:bookmark"),
    ("briefcase", "lucide:briefcase"),
    ("bus", "lucide:bus"),
    ("calendar", "lucide:calendar"),
    ("camera", "lucide:camera"),
    ("car", "lucide:car"),
    ("cart", "lucide:shopping-cart"),
    ("cash", "lucide:banknote"),
    ("cellphone", "lucide:smartphone"),
    ("chat", "lucide:message-circle"),
    ("clock", "lucide:clock"),
    ("cloud", "lucide:cloud"),
    ("code-tags", "lucide:code"),
    ("console", "lucide:terminal"),
    ("controller", "lucide:gamepad-2"),
    ("credit-card", "lucide:credit-card"),
    ("currency-usd", "lucide:dollar-sign"),
    ("database", "lucide:database"),
    ("debian", "lucide:terminal"),
    ("desktop-tower", "lucide:monitor"),
    ("email", "lucide:mail"),
    ("email-outline", "lucide:mail"),
    ("file-document", "lucide:file-text"),
    ("fingerprint", "lucide:fingerprint"),
    ("firefox", "lucide:globe"),
    ("folder", "lucide:folder"),
    ("folder-key", "lucide:folder-key"),
    ("folder-lock", "lucide:folder-lock"),
    ("gift", "lucide:gift"),
    ("github", "lucide:github"),
    ("gitlab", "lucide:gitlab"),
    ("google-chrome", "lucide:globe"),
    ("harddisk", "lucide:hard-drive"),
    ("headphones", "lucide:headphones"),
    ("heart", "lucide:heart"),
    ("home", "lucide:house"),
    ("internet-explorer", "lucide:globe"),
    ("key", "lucide:key"),
    ("key-variant", "lucide:key-round"),
    ("laptop", "lucide:laptop"),
    ("lightbulb", "lucide:lightbulb"),
    ("linux", "lucide:terminal"),
    ("lock", "lucide:lock"),
    ("lock-open", "lucide:lock-open"),
    ("map-marker", "lucide:map-pin"),
    ("message", "lucide:message-square"),
    ("microsoft-windows", "lucide:monitor"),
    ("music", "lucide:music"),
    ("note", "lucide:sticky-note"),
    ("notebook", "lucide:notebook"),
    ("phone", "lucide:phone"),
    ("piggy-bank", "lucide:piggy-bank"),
    ("security", "lucide:shield"),
    ("server", "lucide:server"),
    ("shield", "lucide:shield"),
    ("shield-check", "lucide:shield-check"),
    ("shopping", "lucide:shopping-bag"),
    ("star", "lucide:star"),
    ("store", "lucide:store"),
    ("tag", "lucide:tag"),
    ("television", "lucide:tv"),
    ("ticket", "lucide:ticket"),
    ("train", "lucide:train-front"),
    ("video", "lucide:video"),
    ("web", "lucide:globe"),
    ("wifi", "lucide:wifi"),
    ("wikipedia", "lucide:book-open"),
    ("wrench", "lucide:wrench"),
];

/// The short names of the old haex-pass extension that are not a Lucide name as they stand.
const LEGACY: &[(&str, &str)] = &[
    ("file", "lucide:file-text"),
    ("home", "lucide:house"),
    ("message", "lucide:message-square"),
];

/// The picture of holzi for a haex-vault 2.x icon name; `None` when there is none (the caller reports
/// it). `binary:` values are pictures of their own and not handled here.
pub(super) fn map_icon(name: &str) -> Option<&'static str> {
    let name = name.trim();
    if let Some(mdi) = name.strip_prefix("mdi:") {
        return lookup(MDI, mdi);
    }
    if let Some(lucide) = name.strip_prefix("i-lucide-") {
        return known(lucide);
    }
    if let Some(lucide) = name.strip_prefix("lucide:") {
        return known(lucide);
    }
    if name.contains(':') {
        return None;
    }
    lookup(LEGACY, name).or_else(|| known(name))
}

/// The target for a Lucide name, with haex-vault 2.x's older spellings renamed.
fn known(lucide: &str) -> Option<&'static str> {
    let lucide = lookup(LUCIDE_RENAMED, lucide).unwrap_or(lucide);
    let wanted = format!("lucide:{lucide}");
    TARGETS.iter().copied().find(|target| *target == wanted)
}

fn lookup(table: &[(&str, &'static str)], key: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(from, _)| *from == key)
        .map(|(_, to)| *to)
}

/// Every picture [`map_icon`] can return, for the test against the interface list.
#[cfg(test)]
fn target_names() -> impl Iterator<Item = &'static str> {
    TARGETS.iter().copied()
}

#[cfg(test)]
#[path = "icons_tests.rs"]
mod tests;
