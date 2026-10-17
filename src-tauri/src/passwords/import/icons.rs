//! The KeePass standard icons 0–68 as icon names of holzi (spec 034, US7, research R12 point 7).
//! KeePass stores only the index; the pictures belong to the application, so this table is the icon
//! set. Ported from `KEEPASS_ICONS` of haex-vault 2.x (Legacy) (`keepass.vue`, SHA
//! `8dce379d94e18fcd42c3b73686a06f984ca3f574`) to literal `lucide:` names; the brand icons that
//! haex-vault 2.x takes from another set (Linux, Wikipedia, Debian, Firefox, Chrome, Internet Explorer,
//! Windows) map to the nearest Lucide picture. Every target name is also a literal in
//! `src/lib/passwords/icons.ts`, so the icon scan of the build bundles it (the test checks that).

/// The name for a KeePass standard icon index; `None` for an index outside 0–68.
pub fn standard_icon(index: usize) -> Option<&'static str> {
    STANDARD_ICONS.get(index).copied()
}

/// Every target name, for the check against the list of the interface.
pub fn target_names() -> impl Iterator<Item = &'static str> {
    STANDARD_ICONS.iter().copied()
}

const STANDARD_ICONS: [&str; 69] = [
    "lucide:key",             // 0  Key
    "lucide:globe",           // 1  World / Network
    "lucide:triangle-alert",  // 2  Warning
    "lucide:server",          // 3  Network Server
    "lucide:folder-check",    // 4  Marked Directory
    "lucide:message-circle",  // 5  User Communication
    "lucide:puzzle",          // 6  Parts
    "lucide:notebook",        // 7  Notepad
    "lucide:network",         // 8  World Socket
    "lucide:contact",         // 9  Identity
    "lucide:file-check",      // 10 Paper Ready
    "lucide:camera",          // 11 Digicam
    "lucide:bluetooth",       // 12 IR Communication
    "lucide:key-round",       // 13 Multi Keys
    "lucide:zap",             // 14 Energy
    "lucide:scan",            // 15 Scanner
    "lucide:wifi",            // 16 World Star
    "lucide:mailbox",         // 17 Envelope Box
    "lucide:hard-drive",      // 18 Disk
    "lucide:monitor",         // 19 Monitor
    "lucide:mail",            // 20 EMail
    "lucide:settings",        // 21 Configuration
    "lucide:clipboard",       // 22 Clipboard Ready
    "lucide:file-plus",       // 23 Paper New
    "lucide:terminal",        // 24 Screen / Terminal
    "lucide:plug",            // 25 Energy Careful
    "lucide:wallet",          // 26 E-Wallet
    "lucide:key-square",      // 27 Keys
    "lucide:notebook-text",   // 28 Notepad 2
    "lucide:id-card",         // 29 ID Card
    "lucide:nfc",             // 30 Smart Card
    "lucide:calculator",      // 31 Calculator
    "lucide:file-pen",        // 32 Notepad 3
    "lucide:package",         // 33 Card Package
    "lucide:folder",          // 34 Folder
    "lucide:folder-open",     // 35 Folder Open
    "lucide:folder-archive",  // 36 Folder Package
    "lucide:lock-open",       // 37 Lock Open
    "lucide:file-lock",       // 38 Paper Locked
    "lucide:circle-check",    // 39 Checked
    "lucide:pen",             // 40 Pen
    "lucide:image",           // 41 Thumbnail
    "lucide:book-open",       // 42 Book
    "lucide:list",            // 43 List
    "lucide:user-key",        // 44 User Key
    "lucide:wrench",          // 45 Tool
    "lucide:house",           // 46 Home
    "lucide:star",            // 47 Star
    "lucide:terminal",        // 48 Tux (Linux)
    "lucide:feather",         // 49 Feather
    "lucide:apple",           // 50 Apple
    "lucide:book-open",       // 51 Wikipedia
    "lucide:banknote",        // 52 Money
    "lucide:award",           // 53 Certificate
    "lucide:smartphone",      // 54 Phone
    "lucide:tablet",          // 55 PDA
    "lucide:files",           // 56 Files
    "lucide:clipboard-check", // 57 Clipboard Check
    "lucide:file-archive",    // 58 Zip Archive
    "lucide:terminal",        // 59 Debian (Linux)
    "lucide:globe",           // 60 Firefox
    "lucide:globe",           // 61 Chrome
    "lucide:globe",           // 62 Internet Explorer
    "lucide:monitor",         // 63 Windows
    "lucide:screen-share",    // 64 Remote Desktop
    "lucide:timer",           // 65 Stopwatch
    "lucide:printer",         // 66 Printer
    "lucide:shield",          // 67 Emblem
    "lucide:camera",          // 68 Camera
];

#[cfg(test)]
#[path = "icons_tests.rs"]
mod tests;
