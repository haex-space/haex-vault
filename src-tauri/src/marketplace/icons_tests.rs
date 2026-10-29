//! The icons of the catalog (T022): what is an image, which addresses are refused, that nothing but
//! a public address is called, and the cache and the limit of parallel loads.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use wiremock::ResponseTemplate;

use super::*;
use crate::marketplace::test_support::TestMarketplace;

const PUBLIC: &str = "93.184.216.34";

fn ip(text: &str) -> IpAddr {
    text.parse().expect("ip")
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\0\x10JFIF";
const WEBP: &[u8] = b"RIFF\x24\0\0\0WEBPVP8 ";

/// A network where `table` says what a name resolves to and every connection ends at the local
/// test server, like a public address that happens to be this machine.
fn network(table: &[(&str, &[&str])]) -> (Network, Arc<Mutex<Vec<IpAddr>>>) {
    let table: HashMap<String, Vec<IpAddr>> = table
        .iter()
        .map(|(host, ips)| ((*host).to_owned(), ips.iter().map(|i| ip(i)).collect()))
        .collect();
    let connected = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&connected);
    let net = Network::with(
        move |host, _port| {
            let answer = table.get(&host).cloned().unwrap_or_default();
            Box::pin(async move { answer })
        },
        move |address, port| {
            seen.lock().expect("lock").push(address);
            SocketAddr::new(ip("127.0.0.1"), port)
        },
    );
    (net, connected)
}

fn port(market: &TestMarketplace) -> u16 {
    market.server.address().port()
}

async fn serve(market: &TestMarketplace, route: &str, body: Vec<u8>, content_type: &str) {
    market
        .reply(
            route,
            ResponseTemplate::new(200).set_body_raw(body, content_type),
        )
        .await;
}

#[tokio::test]
async fn png_jpeg_and_webp_are_accepted_by_their_bytes() {
    let market = TestMarketplace::start().await;
    serve(&market, "/a.png", PNG.to_vec(), "application/octet-stream").await;
    serve(&market, "/b.jpg", JPEG.to_vec(), "text/plain").await;
    serve(&market, "/c.webp", WEBP.to_vec(), "image/webp").await;
    let (net, _) = network(&[("icons.example", &[PUBLIC])]);
    let base = format!("http://icons.example:{}/api", port(&market));
    for (route, prefix) in [
        ("/a.png", "data:image/png;base64,"),
        ("/b.jpg", "data:image/jpeg;base64,"),
        ("/c.webp", "data:image/webp;base64,"),
    ] {
        let url = format!("http://icons.example:{}{route}", port(&market));
        let data = Icons::default().load(&net, &base, &url).await.expect(route);
        assert!(data.starts_with(prefix), "{route}: {data}");
    }
}

#[tokio::test]
async fn svg_html_and_a_lying_content_type_are_refused() {
    let market = TestMarketplace::start().await;
    serve(
        &market,
        "/x.svg",
        b"<svg xmlns='http://www.w3.org/2000/svg'><script>1</script></svg>".to_vec(),
        "image/svg+xml",
    )
    .await;
    serve(
        &market,
        "/y.png",
        b"<html><script>alert(1)</script></html>".to_vec(),
        "image/png",
    )
    .await;
    serve(&market, "/z.png", Vec::new(), "image/png").await;
    let (net, _) = network(&[("icons.example", &[PUBLIC])]);
    let base = format!("http://icons.example:{}", port(&market));
    for route in ["/x.svg", "/y.png", "/z.png"] {
        let url = format!("{base}{route}");
        let refusal = fetch(&net, &base, &url).await.expect_err(route);
        assert_eq!(refusal, IconRefusal::NotAnImage, "{route}");
    }
}

#[tokio::test]
async fn the_size_limit_is_exact() {
    let market = TestMarketplace::start().await;
    let mut exact = PNG.to_vec();
    exact.resize(ICON_MAX_BYTES, 0);
    let mut over = PNG.to_vec();
    over.resize(ICON_MAX_BYTES + 1, 0);
    serve(&market, "/exact.png", exact, "image/png").await;
    serve(&market, "/over.png", over, "image/png").await;
    let (net, _) = network(&[("icons.example", &[PUBLIC])]);
    let base = format!("http://icons.example:{}", port(&market));
    assert!(fetch(&net, &base, &format!("{base}/exact.png"))
        .await
        .is_ok());
    assert_eq!(
        fetch(&net, &base, &format!("{base}/over.png"))
            .await
            .expect_err("over"),
        IconRefusal::TooLarge
    );
}

#[tokio::test]
async fn no_login_goes_with_the_request() {
    let market = TestMarketplace::start().await;
    serve(&market, "/a.png", PNG.to_vec(), "image/png").await;
    let (net, _) = network(&[("icons.example", &[PUBLIC])]);
    let base = format!("http://icons.example:{}", port(&market));
    fetch(&net, &base, &format!("{base}/a.png"))
        .await
        .expect("icon");
    let request = market.requests().await.remove(0);
    assert!(request.headers.get("authorization").is_none());
    assert!(request.headers.get("cookie").is_none());
}

#[tokio::test]
async fn a_name_that_resolves_to_the_local_network_is_not_called() {
    let market = TestMarketplace::start().await;
    serve(&market, "/a.png", PNG.to_vec(), "image/png").await;
    let (net, connected) = network(&[
        ("loop.example", &["127.0.0.1"]),
        ("lan.example", &["192.168.1.20"]),
        ("meta.example", &["169.254.169.254"]),
        ("six.example", &["::1"]),
        ("ula.example", &["fd00::1"]),
        ("mixed.example", &[PUBLIC, "10.0.0.5"]),
        ("gone.example", &[]),
    ]);
    for host in ["loop", "lan", "meta", "six", "ula", "mixed", "gone"] {
        // The marketplace itself is on the same origin and still gets no exception (R12).
        let base = format!("http://{host}.example:{}", port(&market));
        let refusal = fetch(&net, &base, &format!("{base}/a.png"))
            .await
            .expect_err(host);
        assert_eq!(refusal, IconRefusal::Destination, "{host}");
    }
    assert!(market.requests().await.is_empty(), "no request went out");
    assert!(
        connected.lock().expect("lock").is_empty(),
        "nothing was connected to"
    );
}

#[tokio::test]
async fn an_address_that_is_a_private_ip_literal_is_not_called() {
    let market = TestMarketplace::start().await;
    serve(&market, "/a.png", PNG.to_vec(), "image/png").await;
    let (net, _) = network(&[]);
    let local = format!("http://127.0.0.1:{}", port(&market));
    assert_eq!(
        fetch(&net, &local, &format!("{local}/a.png"))
            .await
            .expect_err("literal"),
        IconRefusal::Destination
    );
    assert!(market.requests().await.is_empty());
}

#[tokio::test]
async fn the_resolved_address_is_pinned_for_the_connection() {
    let market = TestMarketplace::start().await;
    serve(&market, "/a.png", PNG.to_vec(), "image/png").await;
    let (net, connected) = network(&[("icons.example", &[PUBLIC, "93.184.216.35"])]);
    let base = format!("http://icons.example:{}", port(&market));
    fetch(&net, &base, &format!("{base}/a.png"))
        .await
        .expect("icon");
    // `icons.example` does not exist in the real DNS: the request could only have worked through
    // the address that was checked and pinned.
    assert_eq!(connected.lock().expect("lock").as_slice(), [ip(PUBLIC)]);
    assert_eq!(market.hits("/a.png").await, 1);
}

#[tokio::test]
async fn a_redirect_is_resolved_and_checked_again_so_a_changing_dns_answer_does_not_slip_through() {
    let market = TestMarketplace::start().await;
    market
        .redirect(
            "/r.png",
            &format!("http://icons.example:{}/secret.png", port(&market)),
        )
        .await;
    serve(&market, "/secret.png", PNG.to_vec(), "image/png").await;
    // The first answer is public, every later one points into the local network (rebinding).
    let calls = Arc::new(Mutex::new(0u32));
    let counted = Arc::clone(&calls);
    let net = Network::with(
        move |_host, _port| {
            let mut count = counted.lock().expect("lock");
            *count += 1;
            let answer = if *count == 1 {
                ip(PUBLIC)
            } else {
                ip("10.0.0.5")
            };
            Box::pin(async move { vec![answer] })
        },
        |_address, port| SocketAddr::new(ip("127.0.0.1"), port),
    );
    let base = format!("http://icons.example:{}", port(&market));
    let refusal = fetch(&net, &base, &format!("{base}/r.png"))
        .await
        .expect_err("rebinding");
    assert_eq!(refusal, IconRefusal::Destination);
    assert_eq!(
        *calls.lock().expect("lock"),
        2,
        "the redirect was resolved again"
    );
    assert_eq!(market.hits("/r.png").await, 1);
    assert_eq!(
        market.hits("/secret.png").await,
        0,
        "the private target was never called"
    );
}

#[tokio::test]
async fn a_redirect_to_another_origin_must_use_https_and_a_redirect_loop_ends() {
    let market = TestMarketplace::start().await;
    market
        .redirect(
            "/r.png",
            &format!("http://other.example:{}/a.png", port(&market)),
        )
        .await;
    market
        .redirect(
            "/loop.png",
            &format!("http://icons.example:{}/loop.png", port(&market)),
        )
        .await;
    let (net, _) = network(&[
        ("icons.example", &[PUBLIC]),
        ("other.example", &["93.184.216.40"]),
    ]);
    let base = format!("http://icons.example:{}", port(&market));
    assert_eq!(
        fetch(&net, &base, &format!("{base}/r.png"))
            .await
            .expect_err("other origin over http"),
        IconRefusal::Address
    );
    assert_eq!(
        fetch(&net, &base, &format!("{base}/loop.png"))
            .await
            .expect_err("loop"),
        IconRefusal::Fetch
    );
}

#[test]
fn the_scheme_rules_of_one_hop() {
    let source = Url::parse("https://market.example/api")
        .expect("url")
        .origin();
    let ok = |text: &str| check_address(&source, &Url::parse(text).expect("url"));
    assert!(ok("https://img.example/a.png").is_ok());
    assert!(ok("https://market.example/a.png").is_ok());
    assert_eq!(
        ok("http://img.example/a.png"),
        Err(IconRefusal::Address),
        "external http"
    );
    assert_eq!(
        ok("http://market.example/a.png"),
        Err(IconRefusal::Address),
        "another scheme is another origin"
    );
    assert_eq!(ok("ftp://img.example/a.png"), Err(IconRefusal::Address));
    assert_eq!(
        ok("https://user:pw@img.example/a.png"),
        Err(IconRefusal::Address)
    );
    assert_eq!(ok("data:image/png;base64,AAAA"), Err(IconRefusal::Address));
    let plain = Url::parse("http://lan.example:8080").expect("url").origin();
    assert!(check_address(
        &plain,
        &Url::parse("http://lan.example:8080/a.png").expect("url")
    )
    .is_ok());
    assert_eq!(
        check_address(
            &plain,
            &Url::parse("http://lan.example:9090/a.png").expect("url")
        ),
        Err(IconRefusal::Address),
        "another port is another origin"
    );
}

#[test]
fn public_and_non_public_addresses() {
    for public in [
        "8.8.8.8",
        "93.184.216.34",
        "1.1.1.1",
        "172.15.0.1",
        "172.32.0.1",
        "100.63.0.1",
        "2606:4700:4700::1111",
        "::ffff:8.8.8.8",
        "2002:0808:0808::1",
    ] {
        assert!(is_public_ip(ip(public)), "{public}");
    }
    for local in [
        "0.0.0.0",
        "0.1.2.3",
        "10.1.1.1",
        "127.0.0.1",
        "127.255.255.254",
        "100.64.0.1",
        "100.127.255.255",
        "169.254.169.254",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.1",
        "192.0.0.1",
        "192.0.2.1",
        "198.18.0.1",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "239.255.255.250",
        "240.0.0.1",
        "255.255.255.255",
        "::",
        "::1",
        "fe80::1",
        "fc00::1",
        "fd12::1",
        "ff02::1",
        "2001:db8::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "2002:7f00:0001::1",
        "64:ff9b::1",
    ] {
        assert!(!is_public_ip(ip(local)), "{local}");
    }
}

#[test]
fn the_kind_of_an_image_is_read_from_its_bytes_only() {
    assert_eq!(image_mime(PNG), Some("image/png"));
    assert_eq!(image_mime(JPEG), Some("image/jpeg"));
    assert_eq!(image_mime(WEBP), Some("image/webp"));
    assert_eq!(image_mime(b"GIF89a"), None);
    assert_eq!(image_mime(b"<svg"), None);
    assert_eq!(image_mime(b"RIFF\0\0\0\0WAVE"), None);
    assert_eq!(image_mime(b""), None);
}

#[tokio::test]
async fn the_cache_serves_the_second_call_without_a_request() {
    let market = TestMarketplace::start().await;
    serve(&market, "/a.png", PNG.to_vec(), "image/png").await;
    let (net, _) = network(&[("icons.example", &[PUBLIC])]);
    let base = format!("http://icons.example:{}", port(&market));
    let url = format!("{base}/a.png");
    let icons = Icons::default();
    let first = icons.load(&net, &base, &url).await.expect("first");
    let second = icons.load(&net, &base, &url).await.expect("second");
    assert_eq!(first, second);
    assert_eq!(market.hits("/a.png").await, 1);
}

#[tokio::test]
async fn the_cache_forgets_the_oldest_first() {
    let market = TestMarketplace::start().await;
    for route in ["/1.png", "/2.png", "/3.png"] {
        serve(&market, route, PNG.to_vec(), "image/png").await;
    }
    let (net, _) = network(&[("icons.example", &[PUBLIC])]);
    let base = format!("http://icons.example:{}", port(&market));
    let icons = Icons::new(4, 2);
    for route in ["/1.png", "/2.png", "/3.png"] {
        icons
            .load(&net, &base, &format!("{base}{route}"))
            .await
            .expect(route);
    }
    icons
        .load(&net, &base, &format!("{base}/3.png"))
        .await
        .expect("kept");
    icons
        .load(&net, &base, &format!("{base}/1.png"))
        .await
        .expect("loaded again");
    assert_eq!(market.hits("/3.png").await, 1, "the newest was kept");
    assert_eq!(market.hits("/1.png").await, 2, "the oldest was forgotten");
}

#[tokio::test]
async fn a_load_waits_when_all_permits_are_taken() {
    let market = TestMarketplace::start().await;
    serve(&market, "/a.png", PNG.to_vec(), "image/png").await;
    let (net, _) = network(&[("icons.example", &[PUBLIC])]);
    let base = format!("http://icons.example:{}", port(&market));
    let url = format!("{base}/a.png");
    let icons = Icons::new(1, 10);
    let held = icons.limit().acquire_owned().await.expect("permit");
    let waiting =
        tokio::time::timeout(Duration::from_millis(100), icons.load(&net, &base, &url)).await;
    assert!(waiting.is_err(), "the load waits for a permit");
    assert!(market.requests().await.is_empty());
    drop(held);
    icons.load(&net, &base, &url).await.expect("now it loads");
}

#[tokio::test]
async fn every_refusal_reads_as_unavailable() {
    let (net, _) = network(&[]);
    let error = Icons::default()
        .load(&net, "https://market.example", "http://127.0.0.1/a.png")
        .await
        .expect_err("refused");
    assert!(matches!(
        error,
        HolziError::Marketplace { ref reason } if reason == crate::marketplace::kinds::ICON_UNAVAILABLE
    ));
}
