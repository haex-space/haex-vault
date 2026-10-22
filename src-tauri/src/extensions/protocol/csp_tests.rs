use std::collections::BTreeSet;

use super::*;

#[test]
fn inline_scripts_are_hashed_and_scripts_with_src_are_not() {
    let html = concat!(
        "<head><script>window.__NUXT__={}</script>",
        "<script type=\"module\" src=\"./_nuxt/a.js\"></script>",
        "<SCRIPT data-src='x'>one()</SCRIPT>",
        "<script\nsrc=./b.js></script>",
        "<scripts>not a script</scripts>",
        "<script type=\"importmap\">{\"imports\":{}}</script></head>"
    );
    assert_eq!(
        inline_script_hashes(html),
        vec![
            source_hash("window.__NUXT__={}"),
            source_hash("one()"),
            source_hash("{\"imports\":{}}"),
        ]
    );
}

#[test]
fn a_known_script_has_the_hash_a_browser_computes() {
    // `echo -n "alert(1)" | openssl dgst -sha256 -binary | base64`
    assert_eq!(
        source_hash("alert(1)"),
        "'sha256-bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI='"
    );
}

#[test]
fn the_policy_allows_only_the_prefix_and_the_hashes() {
    let prefix = "holzi-ext://localhost/0123/";
    let policy = policy(prefix, &BTreeSet::from(["'sha256-AAA='".to_string()]));
    assert!(policy.starts_with(
        "default-src 'none'; script-src holzi-ext://localhost/0123/ 'wasm-unsafe-eval' 'sha256-AAA=';"
    ));
    for forbidden in ["'unsafe-eval'", "ipc:", "ipc.localhost", "https:", " * "] {
        assert!(!policy.contains(forbidden), "{forbidden} in {policy}");
    }
    let script_src = policy
        .split(';')
        .find(|d| d.trim_start().starts_with("script-src"))
        .unwrap();
    assert!(!script_src.contains("unsafe-inline"), "{script_src}");
    assert!(policy.contains("frame-src 'none'"));
    assert!(policy.contains("form-action 'none'"));
    assert!(policy.contains("frame-ancestors "));
}
