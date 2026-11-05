use serde_json::json;

use super::cookies::{check_expiry, parse, site_of, Cookie, CookieRef};

fn reply() -> serde_json::Value {
    json!({ "cookies": [
        { "name": "sid", "value": "s3cr3t", "domain": ".shop.example", "path": "/",
          "expires": 1893456000.0, "size": 9, "httpOnly": true, "secure": true,
          "session": false, "sameSite": "Lax", "priority": "Medium", "sameParty": true,
          "sourceScheme": "Secure", "sourcePort": 443,
          "partitionKey": { "topLevelSite": "https://shop.example", "hasCrossSiteAncestor": false } },
        { "name": "theme", "value": "dark", "domain": "shop.example", "path": "/app",
          "expires": -1.0, "size": 9, "httpOnly": false, "secure": false, "session": true },
        { "broken": true }
    ]})
}

#[test]
fn the_browsers_reply_is_read_and_a_cookie_that_cannot_be_read_is_left_out() {
    let cookies = parse(&reply());
    assert_eq!(cookies.len(), 2);
    assert_eq!(cookies[0].name, "sid");
    assert!(cookies[0].http_only && cookies[0].secure);
    assert_eq!(cookies[0].same_site.as_deref(), Some("Lax"));
    assert_eq!(cookies[1].same_site, None);
    assert!(parse(&json!({})).is_empty());
}

#[test]
fn the_list_shows_no_value_and_groups_by_the_website_without_the_dot() {
    let cookies = parse(&reply());
    let views: Vec<_> = cookies.iter().map(Cookie::view).collect();
    assert_eq!(views[0].site, "shop.example");
    assert_eq!(
        views[0].domain, ".shop.example",
        "the exact domain names the cookie"
    );
    assert_eq!(
        views[1].site, "shop.example",
        "a host cookie belongs to the same website"
    );
    assert_eq!(views[0].expires, Some(1_893_456_000));
    assert_eq!(views[1].expires, None, "a session cookie has no expiry");
    let text = serde_json::to_string(&views).unwrap();
    assert!(!text.contains("s3cr3t") && !text.contains("dark"), "{text}");
    assert_eq!(site_of(".a.example"), "a.example");
    assert_eq!(site_of("a.example"), "a.example");
}

#[test]
fn a_cookie_is_written_again_with_every_attribute_kept() {
    let cookies = parse(&reply());
    let domain = cookies[0].set_params("new", Some(1_900_000_000));
    assert_eq!(domain["value"], "new");
    assert_eq!(
        domain["domain"], ".shop.example",
        "a cookie of the domain stays one"
    );
    assert!(domain.get("url").is_none());
    assert_eq!(domain["expires"], 1_900_000_000);
    assert_eq!(domain["secure"], true);
    assert_eq!(domain["httpOnly"], true);
    assert_eq!(domain["sameSite"], "Lax");
    assert_eq!(domain["priority"], "Medium");
    assert_eq!(domain["sameParty"], true);
    assert_eq!(domain["sourceScheme"], "Secure");
    assert_eq!(domain["sourcePort"], 443);
    assert_eq!(
        domain["partitionKey"],
        json!({ "topLevelSite": "https://shop.example", "hasCrossSiteAncestor": false })
    );
    assert_eq!(domain["path"], "/");

    let host = cookies[1].set_params("light", None);
    assert!(
        host.get("domain").is_none(),
        "a host cookie is set by its address"
    );
    assert_eq!(host["url"], "http://shop.example/app");
    assert!(
        host.get("expires").is_none(),
        "no expiry is a session cookie"
    );
    let secure_host = Cookie {
        secure: true,
        ..cookies[1].clone()
    };
    assert_eq!(
        secure_host.set_params("x", None)["url"],
        "https://shop.example/app"
    );
}

#[test]
fn a_new_expiry_must_lie_ahead() {
    assert!(check_expiry(None, 1_000).is_ok(), "a session cookie");
    assert!(check_expiry(Some(2_000), 1_000).is_ok());
    assert!(check_expiry(Some(1_000), 1_000).is_err());
    assert!(check_expiry(Some(5), 1_000).is_err());
}

#[test]
fn a_cookie_is_named_by_its_name_exact_domain_and_path() {
    let cookies = parse(&reply());
    let key = CookieRef {
        name: "sid".to_owned(),
        domain: ".shop.example".to_owned(),
        path: "/".to_owned(),
    };
    assert!(cookies[0].matches(&key));
    assert!(!cookies[1].matches(&key));
    let host_only = CookieRef {
        domain: "shop.example".to_owned(),
        ..key
    };
    assert!(
        !cookies[0].matches(&host_only),
        "the dot makes another cookie"
    );
}
