//! The ids of the browser tools by what they do (spec 065, contracts/browser-tools.md).

/// The tools that only read.
pub const READING: &[&str] = &[
    "browser.page.read",
    "browser.page.screenshot",
    "browser.scroll",
    "browser.hover",
    "browser.tabs.list",
    "browser.wait",
    "browser.takeover.request",
];

/// Asks the user to take over and waits for the browser back; reading, since it changes nothing on
/// a website.
pub(super) const TAKEOVER: &str = "browser.takeover.request";

/// The tools that change something (every navigation counts, contracts/browser-tools.md); they ask
/// the user themselves.
pub const CHANGING: &[&str] = &[
    "browser.open",
    "browser.search",
    "browser.tabs.open",
    "browser.navigate",
    "browser.follow_link",
    "browser.click",
    "browser.click_at",
    "browser.drag",
    "browser.type",
    "browser.select",
    "browser.press_key",
    "browser.media",
    "browser.tabs.close",
    "browser.download",
    "browser.upload",
    "browser.credentials.fill",
    "browser.entry.fill",
    "browser.credentials.create",
];

/// The changing tools that operate an element: they ask with it, not with their input, and the
/// page is read again after them.
pub(super) const OPERATING: &[&str] = &[
    "browser.click",
    "browser.click_at",
    "browser.drag",
    "browser.type",
    "browser.select",
    "browser.upload",
    "browser.press_key",
    "browser.hover",
    "browser.scroll",
    "browser.media",
];

/// The tools that ask themselves, with the element they operate or the entries of a fill; a
/// download and a fill reply with their result, not with the page.
pub(super) const ASKS_WITH_ELEMENT: &[&str] = &[
    "browser.click",
    "browser.click_at",
    "browser.drag",
    "browser.type",
    "browser.select",
    "browser.press_key",
    "browser.media",
    "browser.upload",
    "browser.download",
    "browser.credentials.fill",
    "browser.entry.fill",
    "browser.credentials.create",
];

pub const BROWSER_ACTION_IDS: &[&str] = &[
    "browser.search",
    "browser.page.read",
    "browser.page.screenshot",
    "browser.scroll",
    "browser.tabs.list",
    "browser.wait",
    "browser.open",
    "browser.tabs.open",
    "browser.navigate",
    "browser.follow_link",
    "browser.click",
    "browser.click_at",
    "browser.drag",
    "browser.type",
    "browser.select",
    "browser.press_key",
    "browser.hover",
    "browser.media",
    "browser.tabs.close",
    "browser.takeover.request",
    "browser.download",
    "browser.upload",
    "browser.credentials.fill",
    "browser.entry.fill",
    "browser.credentials.create",
];
