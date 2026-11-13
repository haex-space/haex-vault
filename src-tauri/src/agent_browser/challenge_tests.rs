//! Spec 065 T136: which frames and pages are named as a check.

use super::*;

#[test]
fn the_frames_of_the_providers_are_named_by_kind() {
    for (url, kind) in [
        (
            "https://challenges.cloudflare.com/cdn-cgi/challenge-platform/h/b/turnstile/if/ov2/av0/rcv/x/0x4/light/fbE/new/normal/auto/",
            "cloudflare",
        ),
        ("https://shop.example/cdn-cgi/challenge-platform/h/g/orchestrate/chl_page/v1", "cloudflare"),
        ("https://newassets.hcaptcha.com/captcha/v1/abc/static/hcaptcha.html", "hcaptcha"),
        ("https://hcaptcha.com/1/api.js", "hcaptcha"),
        ("https://www.google.com/recaptcha/api2/anchor?ar=1&k=key", "recaptcha"),
        ("https://www.google.com/recaptcha/enterprise/anchor?k=key", "recaptcha"),
        ("https://www.recaptcha.net/recaptcha/api2/bframe", "recaptcha"),
        ("https://geo.captcha-delivery.com/captcha/?initialCid=x", "other"),
        ("https://client-api.arkoselabs.com/fc/gc/", "other"),
    ] {
        assert_eq!(of_frame(url), Some(kind), "{url}");
    }
}

#[test]
fn other_frames_are_not_checks() {
    for url in [
        "https://www.youtube.com/embed/abc",
        "https://www.google.com/maps/embed?pb=x",
        "https://shop.example/checkout",
        "https://notcloudflare.com/",
        "https://evil.example/?next=challenges.cloudflare.com",
        "about:blank",
        "not a url",
        "",
    ] {
        assert_eq!(of_frame(url), None, "{url}");
    }
}

#[test]
fn the_waiting_page_of_a_check_is_named_whatever_its_language() {
    for title in [
        "Just a moment...",
        "Einen Moment …",
        "Un instant…",
        "Un momento...",
        "Even geduld a.u.b.…",
        "Attention Required! | Cloudflare",
    ] {
        assert_eq!(of_page(title), Some("cloudflare"), "{title}");
    }
    for title in [
        "Startseite",
        "Moment mal – ein Blog",
        "Warenkorb",
        "Ten tips: just a moment for yourself",
        "",
    ] {
        assert_eq!(of_page(title), None, "{title}");
    }
}
