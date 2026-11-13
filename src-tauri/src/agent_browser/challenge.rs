//! Recognising a check in front of a page ("are you human", spec 065 FR-067, research R24): a
//! frame of a provider of such checks, or the waiting page of one. holzi only names what it sees,
//! as `challenge` with one of a few fixed kinds, so the agent knows what it is looking at: the
//! checkbox of a check is an element of a frame (FR-053) and is ticked with `browser.click`. holzi
//! does not solve images or puzzles, does not use a service that does, and does not change how the
//! browser looks to a website to pass a check; where the check does not accept the click or asks
//! for more, the agent asks the user to take over (`browser.takeover.request`).

use url::Url;

/// The kind of a check a frame at `url` belongs to, when it is one.
pub fn of_frame(url: &str) -> Option<&'static str> {
    let url = Url::parse(url).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let path = url.path();
    let on = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if on("challenges.cloudflare.com") || path.starts_with("/cdn-cgi/challenge-platform") {
        Some("cloudflare")
    } else if on("hcaptcha.com") {
        Some("hcaptcha")
    } else if on("recaptcha.net")
        || path.contains("/recaptcha/api2/")
        || path.contains("/recaptcha/enterprise/")
    {
        Some("recaptcha")
    } else if [
        "captcha-delivery.com",
        "arkoselabs.com",
        "funcaptcha.com",
        "geetest.com",
    ]
    .iter()
    .any(|domain| on(domain))
    {
        Some("other")
    } else {
        None
    }
}

/// The kind of a check the page with `title` is, when it is the waiting page of one (the page whose
/// title starts "Just a moment..." while a check runs; a title that only mentions the words, like
/// "Just a moment: ten tips for the morning", is not one).
pub fn of_page(title: &str) -> Option<&'static str> {
    let title = title.trim().to_lowercase();
    [
        "just a moment",
        "einen moment",
        "un instant",
        "un momento",
        "even geduld",
        "attention required! | cloudflare",
    ]
    .iter()
    .any(|waiting| title.starts_with(waiting))
    .then_some("cloudflare")
}

#[cfg(test)]
#[path = "challenge_tests.rs"]
mod tests;
