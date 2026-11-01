//! Spec 021 T090: holzi's OAuth server without database or network (contracts/http-auth.md).

use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::oauth::{
    parse_authorize, redirect_allowed, redirect_matches, verify_pkce, AuthorizeRequest, Collected,
    Decision, OAuthError, OAuthState, ACCESS_TTL, CODE_TTL, MAX_CLIENTS, MAX_PENDING,
};
use super::oauth_rows::slug_of;
use super::registry::Credential;

const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const REDIRECT: &str = "http://127.0.0.1:33418/callback";

fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn request(client_id: &str, redirect_uri: &str) -> AuthorizeRequest {
    AuthorizeRequest {
        client_id: client_id.to_owned(),
        redirect_uri: redirect_uri.to_owned(),
        code_challenge: challenge(VERIFIER),
        state: Some("xyz".to_owned()),
    }
}

/// A granted authorization's redirect, and the code in it.
fn granted(oauth: &OAuthState, agent: Uuid, redirect_uri: &str, now: Instant) -> (String, String) {
    let client = oauth
        .register_client(Some("Claude Code".into()), vec![REDIRECT.into()])
        .expect("registered");
    let (id, _) = oauth
        .begin(request(&client.id, redirect_uri), now)
        .expect("begun");
    assert_eq!(oauth.collect(&id, now), Ok(Collected::Waiting));
    oauth.settle(
        &id,
        Decision::Granted { agent },
        Some(Credential::generate().expect("random")),
    );
    let Ok(Collected::Redirect(location)) = oauth.collect(&id, now) else {
        panic!("no redirect");
    };
    assert_eq!(
        oauth.collect(&id, now),
        Ok(Collected::Unknown),
        "collected once"
    );
    let url = url::Url::parse(&location).expect("a url");
    let code = url
        .query_pairs()
        .find(|(name, _)| name == "code")
        .map(|(_, value)| value.into_owned())
        .expect("a code");
    (client.id, code)
}

#[test]
fn dynamic_registration_remembers_a_client_and_nothing_else() {
    let oauth = OAuthState::default();
    let client = oauth
        .register_client(Some("Claude Code".into()), vec![REDIRECT.into()])
        .expect("registered");
    assert_eq!(oauth.client(&client.id), Some(client));
    assert_eq!(oauth.pending_count(), 0);
}

#[test]
fn registration_needs_loopback_or_native_redirect_uris() {
    let oauth = OAuthState::default();
    for refused in [
        vec![],
        vec!["https://evil.example/callback".to_owned()],
        vec!["http://evil.example:8080/callback".to_owned()],
        vec!["javascript:alert(1)".to_owned()],
        vec!["http://127.0.0.1:1/cb#fragment".to_owned()],
    ] {
        assert_eq!(
            oauth.register_client(None, refused.clone()),
            Err(OAuthError::InvalidRedirectUri),
            "{refused:?}"
        );
    }
    for allowed in [
        "http://127.0.0.1:5000/cb",
        "http://localhost/cb",
        "http://[::1]:8/cb",
        "com.example.app:/oauth",
    ] {
        assert!(redirect_allowed(allowed), "{allowed}");
    }
}

#[test]
fn the_oldest_clients_go_when_too_many_register() {
    let oauth = OAuthState::default();
    let first = oauth
        .register_client(None, vec![REDIRECT.into()])
        .expect("registered");
    for _ in 0..MAX_CLIENTS {
        oauth
            .register_client(None, vec![REDIRECT.into()])
            .expect("registered");
    }
    assert_eq!(oauth.client(&first.id), None);
}

#[test]
fn a_loopback_redirect_may_differ_only_by_port() {
    let registered = [REDIRECT.to_owned(), "com.example.app:/oauth".to_owned()];
    assert!(redirect_matches(&registered, REDIRECT));
    assert!(redirect_matches(&registered, "http://127.0.0.1:1/callback"));
    assert!(redirect_matches(&registered, "com.example.app:/oauth"));
    assert!(!redirect_matches(&registered, "http://127.0.0.1:1/other"));
    assert!(!redirect_matches(
        &registered,
        "http://localhost:33418/callback"
    ));
    assert!(!redirect_matches(&registered, "com.example.app:/other"));
    assert!(!redirect_matches(
        &registered,
        "http://127.0.0.1:1/callback?x=1"
    ));
}

#[test]
fn authorize_requires_a_code_with_pkce_s256() {
    let base = "response_type=code&client_id=c&redirect_uri=http%3A%2F%2F127.0.0.1%3A1%2Fcb\
                &code_challenge=abc&state=s";
    let parsed = parse_authorize(&format!("{base}&code_challenge_method=S256")).expect("parsed");
    assert_eq!(parsed.redirect_uri, "http://127.0.0.1:1/cb");
    assert_eq!(parsed.state.as_deref(), Some("s"));
    assert_eq!(
        parse_authorize(&format!("{base}&code_challenge_method=plain")),
        Err(OAuthError::InvalidRequest)
    );
    assert_eq!(parse_authorize(base), Err(OAuthError::InvalidRequest));
    assert_eq!(
        parse_authorize("response_type=token&code_challenge_method=S256"),
        Err(OAuthError::UnsupportedResponseType)
    );
}

#[test]
fn pkce_checks_the_verifier() {
    assert!(verify_pkce(&challenge(VERIFIER), VERIFIER));
    assert!(!verify_pkce(
        &challenge(VERIFIER),
        &VERIFIER.replace('d', "e")
    ));
    assert!(!verify_pkce(&challenge("short"), "short"), "too short");
}

#[test]
fn an_authorization_checks_client_and_redirect_uri_before_it_opens() {
    let oauth = OAuthState::default();
    let now = Instant::now();
    assert_eq!(
        oauth.begin(request("unknown", REDIRECT), now).err(),
        Some(OAuthError::InvalidClient)
    );
    let client = oauth
        .register_client(None, vec![REDIRECT.into()])
        .expect("registered");
    assert_eq!(
        oauth
            .begin(request(&client.id, "http://127.0.0.1:1/elsewhere"), now)
            .err(),
        Some(OAuthError::InvalidRedirectUri)
    );
    assert_eq!(oauth.pending_count(), 0);
}

#[test]
fn at_most_ten_authorizations_wait() {
    let oauth = OAuthState::default();
    let now = Instant::now();
    let client = oauth
        .register_client(None, vec![REDIRECT.into()])
        .expect("registered");
    for _ in 0..MAX_PENDING {
        oauth
            .begin(request(&client.id, REDIRECT), now)
            .expect("open");
    }
    assert_eq!(
        oauth.begin(request(&client.id, REDIRECT), now).err(),
        Some(OAuthError::TooMany)
    );
}

#[test]
fn a_denied_authorization_redirects_with_access_denied() {
    let oauth = OAuthState::default();
    let now = Instant::now();
    let client = oauth
        .register_client(None, vec![REDIRECT.into()])
        .expect("registered");
    let (id, _) = oauth
        .begin(request(&client.id, REDIRECT), now)
        .expect("open");
    oauth.settle(&id, Decision::Denied, None);
    let Ok(Collected::Redirect(location)) = oauth.collect(&id, now) else {
        panic!("no redirect");
    };
    assert!(location.starts_with(REDIRECT), "{location}");
    assert!(location.contains("error=access_denied"), "{location}");
    assert!(location.contains("state=xyz"), "{location}");
    assert!(!location.contains("code="), "{location}");
}

#[test]
fn the_code_binds_client_redirect_uri_and_verifier_and_works_once() {
    let oauth = OAuthState::default();
    let now = Instant::now();
    let agent = Uuid::new_v4();
    let other_port = "http://127.0.0.1:40000/callback";

    let (client, code) = granted(&oauth, agent, other_port, now);
    assert_eq!(
        oauth
            .exchange(&code, &client, REDIRECT, VERIFIER, now)
            .err(),
        Some(OAuthError::InvalidGrant),
        "the accepted redirect URI, not the registered one"
    );
    assert_eq!(
        oauth
            .exchange(&code, &client, other_port, VERIFIER, now)
            .err(),
        Some(OAuthError::InvalidGrant),
        "a failed attempt used the code up"
    );

    let (client, code) = granted(&oauth, agent, other_port, now);
    assert_eq!(
        oauth
            .exchange(&code, "another", other_port, VERIFIER, now)
            .err(),
        Some(OAuthError::InvalidGrant)
    );
    let (client_two, code) = granted(&oauth, agent, other_port, now);
    assert_eq!(
        oauth
            .exchange(
                &code,
                &client_two,
                other_port,
                &VERIFIER.replace('d', "e"),
                now
            )
            .err(),
        Some(OAuthError::InvalidGrant)
    );
    let _ = client;

    let (client, code) = granted(&oauth, agent, other_port, now);
    let late = now + CODE_TTL + Duration::from_secs(1);
    assert_eq!(
        oauth
            .exchange(&code, &client, other_port, VERIFIER, late)
            .err(),
        Some(OAuthError::InvalidGrant)
    );

    let (client, code) = granted(&oauth, agent, other_port, now);
    let exchanged = oauth
        .exchange(&code, &client, other_port, VERIFIER, now)
        .expect("exchanged");
    assert_eq!(exchanged.agent, agent);
    assert_eq!(
        oauth
            .exchange(&code, &client, other_port, VERIFIER, now)
            .err(),
        Some(OAuthError::InvalidGrant),
        "once"
    );
}

#[test]
fn access_tokens_live_one_hour_in_memory() {
    let oauth = OAuthState::default();
    let now = Instant::now();
    let agent = Uuid::new_v4();
    let token = oauth.issue_access(agent, now).expect("issued");
    assert_eq!(oauth.agent_of(&token, now), Some(agent));
    assert_eq!(oauth.agent_of("forged", now), None);
    assert_eq!(oauth.agent_of(&token, now + ACCESS_TTL), None);
    oauth.forget_agent(agent);
    assert_eq!(oauth.agent_of(&token, now), None, "revoked");
}

#[test]
fn a_clients_name_becomes_a_slug() {
    assert_eq!(slug_of(Some("Claude Code")), "claude-code");
    assert_eq!(slug_of(Some("  My  Agent 2.0! ")), "my-agent-2-0");
    assert_eq!(slug_of(Some("!!!")), "http-agent");
    assert_eq!(slug_of(None), "http-agent");
    assert!(slug_of(Some(&"x".repeat(80))).len() <= 32);
    assert!(super::hello::valid_slug(&slug_of(Some("Ünïcödé Agent"))));
}
