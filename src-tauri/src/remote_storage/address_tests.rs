//! The endpoint rules of research R8: a table of typed endpoints, the scope fixed when an endpoint
//! is set, and pinning against that scope with an injected resolver (a public name that now
//! resolves to a local address, DNS rebinding, a mixed answer).

use std::io;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use reqwest::Url;

use super::address::{
    check_endpoint, endpoint_url, is_insecure, pin, port_of, scope_of, AddressError, Resolver,
};
use super::{Addressing, EndpointScope, Location, Protocol, ProviderKind};

use EndpointScope::{Local, Public};

#[test]
fn typed_endpoints_follow_the_rules() {
    let ok = |e: &str| check_endpoint(e).map(|_| ());
    let cases: &[(&str, Result<(), AddressError>)] = &[
        ("https://s3.example.com", Ok(())),
        ("http://s3.example.com", Ok(())),
        ("http://203.0.113.7:9000", Err(AddressError::NotAllowed)),
        ("https://203.0.113.7:9000", Ok(())),
        ("http://192.168.1.5:9000", Ok(())),
        ("https://192.168.1.5:9000", Ok(())),
        ("http://127.0.0.1:9000", Ok(())),
        ("http://localhost:9000", Ok(())),
        ("http://169.254.169.254", Err(AddressError::NotAllowed)),
        ("https://[fe80::1]", Err(AddressError::NotAllowed)),
        (
            "https://[::ffff:169.254.169.254]",
            Err(AddressError::NotAllowed),
        ),
        (
            "https://[64:ff9b::a9fe:a9fe]",
            Err(AddressError::NotAllowed),
        ),
        ("http://[::ffff:10.0.0.1]:9000", Ok(())),
        ("http://[fd00::5]:9000", Ok(())),
        ("https://0.0.0.0", Err(AddressError::NotAllowed)),
        ("https://[::]", Err(AddressError::NotAllowed)),
        ("https://224.0.0.1", Err(AddressError::NotAllowed)),
        ("https://255.255.255.255", Err(AddressError::NotAllowed)),
        ("ftp://s3.example.com", Err(AddressError::Invalid)),
        (
            "https://key:secret@s3.example.com",
            Err(AddressError::Invalid),
        ),
        ("https://key@s3.example.com", Err(AddressError::Invalid)),
        ("https://s3.example.com?x=1", Err(AddressError::Invalid)),
        ("not an address", Err(AddressError::Invalid)),
    ];
    for (endpoint, expected) in cases {
        assert_eq!(&ok(endpoint), expected, "{endpoint}");
    }
}

#[test]
fn an_http_endpoint_is_marked_insecure() {
    let url = check_endpoint("http://192.168.1.5:9000").expect("allowed");
    assert!(is_insecure(&url));
    let url = check_endpoint("https://s3.example.com").expect("allowed");
    assert!(!is_insecure(&url));
}

fn aws(region: &str) -> Location {
    Location {
        protocol: Protocol::S3,
        provider_kind: ProviderKind::Aws,
        endpoint: String::new(),
        endpoint_scope: Public,
        region: region.to_owned(),
        addressing: Addressing::Virtual,
        bucket: "b".to_owned(),
        host_key: None,
    }
}

#[test]
fn aws_without_an_endpoint_uses_the_regional_address() {
    assert_eq!(
        endpoint_url(&aws("eu-central-1")).expect("url").as_str(),
        "https://s3.eu-central-1.amazonaws.com/"
    );
    assert_eq!(
        endpoint_url(&aws("eu/../x")),
        Err(AddressError::Invalid),
        "the region becomes part of a host name"
    );
}

/// Answers from a list, one answer per lookup, and counts the lookups.
struct Answers {
    answers: Mutex<Vec<Vec<IpAddr>>>,
    lookups: AtomicUsize,
}

impl Answers {
    fn new(answers: &[&[&str]]) -> Self {
        Self {
            answers: Mutex::new(
                answers
                    .iter()
                    .rev()
                    .map(|a| a.iter().map(|ip| ip.parse().expect("ip")).collect())
                    .collect(),
            ),
            lookups: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Resolver for Answers {
    async fn lookup(&self, _host: &str, _port: u16) -> io::Result<Vec<IpAddr>> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        self.answers
            .lock()
            .expect("lock")
            .pop()
            .ok_or_else(|| io::Error::other("no answer"))
    }
}

fn url(text: &str) -> Url {
    Url::parse(text).expect("url")
}

#[tokio::test]
async fn the_scope_of_an_endpoint_follows_its_addresses() {
    let cases: &[(&str, &[&str], Result<EndpointScope, AddressError>)] = &[
        ("https://s3.example.com", &["203.0.113.7"], Ok(Public)),
        (
            "https://s3.example.com",
            &["203.0.113.7", "2001:db8::7"],
            Ok(Public),
        ),
        (
            "http://s3.example.com",
            &["203.0.113.7"],
            Err(AddressError::NotAllowed),
        ),
        ("http://rustfs.lan:9000", &["192.168.1.5"], Ok(Local)),
        ("https://rustfs.lan", &["10.0.0.5", "fd00::5"], Ok(Local)),
        ("http://localhost:9000", &["127.0.0.1", "::1"], Ok(Local)),
        (
            "https://s3.example.com",
            &["203.0.113.7", "192.168.1.5"],
            Err(AddressError::NotAllowed),
        ),
        (
            "https://s3.example.com",
            &["203.0.113.7", "169.254.169.254"],
            Err(AddressError::NotAllowed),
        ),
        (
            "http://rustfs.lan",
            &["::ffff:169.254.169.254"],
            Err(AddressError::NotAllowed),
        ),
        (
            "https://nowhere.example.com",
            &[],
            Err(AddressError::Unresolved),
        ),
    ];
    for (endpoint, answer, expected) in cases {
        let resolver = Answers::new(&[answer]);
        assert_eq!(
            &scope_of(&url(endpoint), &resolver).await,
            expected,
            "{endpoint} at {answer:?}"
        );
    }
    let resolver = Answers::new(&[]);
    assert_eq!(
        scope_of(&url("http://192.168.1.5:9000"), &resolver).await,
        Ok(Local),
        "an IP address needs no lookup"
    );
    assert_eq!(resolver.lookups.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_public_endpoint_that_now_resolves_to_a_local_address_is_refused() {
    let resolver = Answers::new(&[&["127.0.0.1"], &["127.0.0.1"]]);
    let target = url("https://storage.example.com");
    assert_eq!(
        pin(&target, Public, &resolver).await,
        Err(AddressError::NotAllowed),
        "DNS rebinding after the endpoint was set"
    );
    let pinned = pin(&target, Local, &resolver)
        .await
        .expect("a local endpoint");
    assert_eq!(pinned.host.as_deref(), Some("storage.example.com"));
    assert_eq!(pinned.addrs, vec!["127.0.0.1:443".parse().expect("addr")]);
}

#[tokio::test]
async fn a_local_endpoint_that_now_resolves_to_a_public_address_is_refused() {
    let resolver = Answers::new(&[&["203.0.113.7"]]);
    assert_eq!(
        pin(&url("http://rustfs.lan:9000"), Local, &resolver).await,
        Err(AddressError::NotAllowed)
    );
}

#[tokio::test]
async fn the_request_goes_to_the_address_checked_once_not_to_a_later_answer() {
    let resolver = Answers::new(&[&["203.0.113.7"], &["169.254.169.254"]]);
    let pinned = pin(&url("https://s3.example.com:9000"), Public, &resolver)
        .await
        .expect("a public address");
    assert_eq!(
        pinned.addrs,
        vec!["203.0.113.7:9000".parse().expect("addr")]
    );
    assert_eq!(
        resolver.lookups.load(Ordering::SeqCst),
        1,
        "resolved once; the second answer (DNS rebinding) is never asked for"
    );
}

#[tokio::test]
async fn one_disallowed_address_in_an_answer_refuses_the_request() {
    for scope in [Public, Local] {
        let resolver = Answers::new(&[&["203.0.113.7", "192.168.1.5", "169.254.169.254"]]);
        assert_eq!(
            pin(&url("https://s3.example.com"), scope, &resolver).await,
            Err(AddressError::NotAllowed),
            "{scope:?}"
        );
    }
}

#[tokio::test]
async fn http_needs_a_local_scope() {
    let resolver = Answers::new(&[&["203.0.113.7"]]);
    assert_eq!(
        pin(&url("http://s3.example.com"), Public, &resolver).await,
        Err(AddressError::NotAllowed)
    );
    assert_eq!(
        resolver.lookups.load(Ordering::SeqCst),
        0,
        "refused before the lookup"
    );
}

#[tokio::test]
async fn an_ip_address_is_checked_without_a_lookup() {
    let resolver = Answers::new(&[]);
    let pinned = pin(&url("http://192.168.1.5:9000"), Local, &resolver)
        .await
        .expect("a local endpoint");
    assert_eq!(pinned.host, None);
    assert_eq!(resolver.lookups.load(Ordering::SeqCst), 0);
    for scope in [Public, Local] {
        assert_eq!(
            pin(&url("https://[::ffff:169.254.169.254]"), scope, &resolver).await,
            Err(AddressError::NotAllowed)
        );
    }
}

#[tokio::test]
async fn a_name_that_does_not_resolve_is_refused() {
    let resolver = Answers::new(&[&[]]);
    assert_eq!(
        pin(&url("https://nowhere.example.com"), Public, &resolver).await,
        Err(AddressError::Unresolved)
    );
}

/// Spec 050 (T069): `sftp://` is encrypted like `https`, its port is 22 unless given, and its
/// addresses follow the same rules.
#[tokio::test]
async fn sftp_is_encrypted_and_listens_on_22_unless_told() {
    assert!(check_endpoint("sftp://storage.example.com").is_ok());
    assert!(
        check_endpoint("sftp://203.0.113.5:23").is_ok(),
        "public, encrypted"
    );
    assert!(check_endpoint("sftp://anna@storage.example.com").is_err());
    assert!(check_endpoint("sftp://169.254.169.254").is_err());
    assert!(
        check_endpoint("sftp://[fe80::1]:22").is_err(),
        "an IP address in an sftp URL is checked like one in https"
    );
    assert!(!is_insecure(&url("sftp://storage.example.com")));
    assert_eq!(port_of(&url("sftp://storage.example.com")), Some(22));
    assert_eq!(port_of(&url("sftp://storage.example.com:23")), Some(23));
    let names = Answers::new(&[&["203.0.113.5"], &["203.0.113.5"]]);
    let target = url("sftp://storage.example.com:23");
    assert_eq!(scope_of(&target, &names).await, Ok(EndpointScope::Public));
    let pinned = pin(&target, EndpointScope::Public, &names)
        .await
        .expect("pinned");
    assert_eq!(pinned.addrs, ["203.0.113.5:23".parse().unwrap()]);
}
