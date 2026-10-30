use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wiremock::ResponseTemplate;

use super::*;
use crate::marketplace::test_support::{entry, TestMarketplace};

const SLUG: &str = "tetris";
const VERSION: &str = "1.0.0";

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn reason(error: HolziError) -> String {
    match error {
        HolziError::Marketplace { reason } => reason,
        other => panic!("not a marketplace error: {other:?}"),
    }
}

fn limits(size: u64, hash: &str) -> Limits {
    Limits {
        max: ARCHIVE_BYTES,
        size,
        hash: hash.to_owned(),
    }
}

/// A version as the details list it, with the checksum and size of `bytes` unless overridden.
fn listed(bytes: &[u8], min_app: Option<&str>) -> Value {
    json!({
        "version": VERSION,
        "bundleSize": bytes.len(),
        "bundleHash": sha256(bytes),
        "minAppVersion": min_app,
        "maxAppVersion": null,
    })
}

/// A marketplace and a second server for the files, as with a signed address on another host.
struct Fixture {
    market: TestMarketplace,
    files: TestMarketplace,
}

impl Fixture {
    async fn new(listed: Value, answer: Value) -> Self {
        let market = TestMarketplace::start().await;
        let files = TestMarketplace::start().await;
        market
            .json(
                &format!("/extensions/{SLUG}"),
                200,
                entry(SLUG, 1, vec![listed]),
            )
            .await;
        let mut answer = answer;
        answer["downloadUrl"] = json!(format!("{}/bundle.xt", files.origin()));
        market
            .json(&format!("/extensions/{SLUG}/download"), 200, answer)
            .await;
        Self { market, files }
    }

    /// Serves `served` while the marketplace promises `promised`.
    async fn serving(served: &[u8], promised: &[u8]) -> Self {
        let fixture = Self::new(listed(promised, None), answer_for(promised)).await;
        fixture
            .files
            .reply(
                "/bundle.xt",
                ResponseTemplate::new(200).set_body_bytes(served.to_vec()),
            )
            .await;
        fixture
    }

    fn source(&self) -> Source {
        Source {
            id: "market".into(),
            name: "Market".into(),
            base_url: self.market.base_url(),
            auth: None,
            unavailable: None,
        }
    }

    async fn download(&self) -> (Result<Vec<u8>>, Vec<DownloadProgress>) {
        let mut seen = Vec::new();
        let result = download_bundle(
            Http::default(),
            &self.source(),
            SLUG,
            VERSION,
            &app_version(),
            &CancellationToken::new(),
            &mut |progress| seen.push(progress),
        )
        .await;
        (result, seen)
    }
}

fn answer_for(bytes: &[u8]) -> Value {
    json!({ "version": VERSION, "bundleSize": bytes.len(), "bundleHash": sha256(bytes) })
}

#[tokio::test]
async fn a_bundle_arrives_whole_and_its_progress_only_grows() {
    let bytes = b"a bundle of some bytes".to_vec();
    let fixture = Fixture::serving(&bytes, &bytes).await;

    let (result, progress) = fixture.download().await;

    assert_eq!(result.unwrap(), bytes);
    let last = progress.last().expect("at least the final report");
    assert_eq!(
        (last.loaded, last.total),
        (bytes.len() as u64, bytes.len() as u64)
    );
    assert!(progress
        .windows(2)
        .all(|pair| pair[0].loaded <= pair[1].loaded));
}

#[tokio::test]
async fn the_checksum_is_compared_without_regard_to_case() {
    let bytes = b"bytes".to_vec();
    let mut answer = answer_for(&bytes);
    answer["bundleHash"] = json!(sha256(&bytes).to_ascii_uppercase());
    let fixture = Fixture::new(listed(&bytes, None), answer).await;
    fixture
        .files
        .reply(
            "/bundle.xt",
            ResponseTemplate::new(200).set_body_bytes(bytes.clone()),
        )
        .await;

    assert_eq!(fixture.download().await.0.unwrap(), bytes);
}

#[tokio::test]
async fn other_bytes_than_the_checksum_names_are_refused() {
    let promised = b"the bundle the marketplace describes".to_vec();
    let mut served = promised.clone();
    served[4] ^= 0x01;
    let fixture = Fixture::serving(&served, &promised).await;

    let (result, _) = fixture.download().await;

    assert_eq!(reason(result.unwrap_err()), kinds::HASH_MISMATCH);
}

#[tokio::test]
async fn fewer_bytes_than_promised_are_refused() {
    let promised = vec![7u8; 40];
    let fixture = Fixture::serving(&promised[..25], &promised).await;

    assert_eq!(
        reason(fixture.download().await.0.unwrap_err()),
        kinds::SIZE_MISMATCH
    );
}

#[tokio::test]
async fn more_bytes_than_promised_are_refused() {
    let promised = vec![7u8; 40];
    let mut served = promised.clone();
    served.extend_from_slice(&[7u8; 10]);
    let fixture = Fixture::serving(&served, &promised).await;

    assert_eq!(
        reason(fixture.download().await.0.unwrap_err()),
        kinds::SIZE_MISMATCH
    );
}

#[tokio::test]
async fn a_version_without_checksum_is_not_downloaded() {
    let bytes = b"bytes".to_vec();
    let mut without = listed(&bytes, None);
    without["bundleHash"] = Value::Null;
    let fixture = Fixture::new(without, answer_for(&bytes)).await;

    let (result, _) = fixture.download().await;

    assert_eq!(reason(result.unwrap_err()), kinds::NO_HASH);
    assert_eq!(fixture.market.hits("/extensions/tetris/download").await, 0);
    assert!(fixture.files.requests().await.is_empty());
}

#[tokio::test]
async fn an_incompatible_version_is_not_downloaded() {
    let bytes = b"bytes".to_vec();
    let fixture = Fixture::new(listed(&bytes, Some("99.0.0")), answer_for(&bytes)).await;

    let (result, _) = fixture.download().await;

    assert_eq!(reason(result.unwrap_err()), kinds::INCOMPATIBLE_VERSION);
    assert!(fixture.files.requests().await.is_empty());
}

#[tokio::test]
async fn a_version_the_marketplace_does_not_list_is_not_found() {
    let bytes = b"bytes".to_vec();
    let fixture = Fixture::serving(&bytes, &bytes).await;

    let result = download_bundle(
        Http::default(),
        &fixture.source(),
        SLUG,
        "9.9.9",
        &app_version(),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::NOT_FOUND);
}

#[tokio::test]
async fn a_slug_that_is_no_path_segment_never_leaves_the_host() {
    let bytes = b"bytes".to_vec();
    let fixture = Fixture::serving(&bytes, &bytes).await;

    let result = download_bundle(
        Http::default(),
        &fixture.source(),
        "../admin",
        VERSION,
        &app_version(),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::NOT_FOUND);
    assert!(fixture.market.requests().await.is_empty());
}

#[tokio::test]
async fn an_address_beyond_the_limit_is_refused_before_a_byte_is_loaded() {
    let bytes = b"bytes".to_vec();
    let mut answer = answer_for(&bytes);
    answer["bundleSize"] = json!(ARCHIVE_BYTES + 1);
    let mut huge = listed(&bytes, None);
    huge["bundleSize"] = json!(ARCHIVE_BYTES + 1);
    let fixture = Fixture::new(huge, answer).await;

    let (result, _) = fixture.download().await;

    assert_eq!(reason(result.unwrap_err()), kinds::TOO_LARGE);
    assert!(fixture.files.requests().await.is_empty());
}

#[tokio::test]
async fn the_file_host_sees_no_login_and_no_marketplace_header() {
    let bytes = b"bytes".to_vec();
    let fixture = Fixture::serving(&bytes, &bytes).await;
    fixture.download().await.0.unwrap();

    let marketplace = fixture.market.requests().await;
    assert!(
        marketplace
            .iter()
            .all(|r| r.headers.contains_key("x-platform")),
        "the marketplace is asked the way the contract says"
    );
    let files = fixture.files.requests().await;
    assert_eq!(files.len(), 1);
    for name in ["authorization", "x-platform", "x-app-version", "cookie"] {
        assert!(
            !files[0].headers.contains_key(name),
            "the signed address must not get `{name}`"
        );
    }
}

#[tokio::test]
async fn a_marketplace_that_requires_a_login_over_http_is_not_asked() {
    let bytes = b"bytes".to_vec();
    let fixture = Fixture::serving(&bytes, &bytes).await;
    let mut source = fixture.source();
    source.auth = Some(crate::marketplace::client::AuthHeader::bearer("secret"));
    // A loopback address would be allowed in a debug build (research R15); this one is not.
    source.base_url = "http://marketplace.invalid".to_owned();

    let result = download_bundle(
        Http::default(),
        &source,
        SLUG,
        VERSION,
        &app_version(),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::INSECURE_CREDENTIALS);
    assert!(fixture.market.requests().await.is_empty());
}

/// A server that sends `total` bytes as fast as the client takes them and counts what it wrote.
async fn flood(total: u64) -> (String, Arc<AtomicU64>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let written = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&written);
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut buffer = [0u8; 4096];
        while !head.ends_with(b"\r\n\r\n") {
            match socket.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(n) => head.extend_from_slice(&buffer[..n]),
            }
        }
        let header =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n");
        if socket.write_all(header.as_bytes()).await.is_err() {
            return;
        }
        let chunk = vec![0u8; 64 * 1024];
        while counter.load(Ordering::SeqCst) < total {
            if socket.write_all(&chunk).await.is_err() {
                break;
            }
            counter.fetch_add(chunk.len() as u64, Ordering::SeqCst);
        }
    });
    (origin, written, task)
}

#[tokio::test]
async fn a_body_much_larger_than_promised_is_not_read_to_its_end() {
    let total = 200 * 1024 * 1024;
    let (origin, written, server) = flood(total).await;
    let promised = vec![0u8; 1000];
    let market = TestMarketplace::start().await;
    market
        .json(
            &format!("/extensions/{SLUG}"),
            200,
            entry(SLUG, 1, vec![listed(&promised, None)]),
        )
        .await;
    let mut answer = answer_for(&promised);
    answer["downloadUrl"] = json!(format!("{origin}/bundle.xt"));
    market
        .json(&format!("/extensions/{SLUG}/download"), 200, answer)
        .await;
    let source = Source {
        id: "market".into(),
        name: "Market".into(),
        base_url: market.base_url(),
        auth: None,
        unavailable: None,
    };

    let result = download_bundle(
        Http::default(),
        &source,
        SLUG,
        VERSION,
        &app_version(),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::SIZE_MISMATCH);
    tokio::time::timeout(Duration::from_secs(20), server)
        .await
        .expect("the server notices that the client left")
        .unwrap();
    assert!(
        written.load(Ordering::SeqCst) < total / 2,
        "the host must stop reading, the server wrote {} of {total}",
        written.load(Ordering::SeqCst)
    );
}

#[tokio::test]
async fn cancelling_ends_a_download_that_is_waiting_for_its_bytes() {
    let bytes = b"bytes".to_vec();
    let fixture = Fixture::new(listed(&bytes, None), answer_for(&bytes)).await;
    fixture
        .files
        .reply(
            "/bundle.xt",
            ResponseTemplate::new(200)
                .set_body_bytes(bytes)
                .set_delay(Duration::from_secs(30)),
        )
        .await;
    let downloads = Downloads::default();
    let closing = CancellationToken::new();
    let id = Uuid::new_v4();
    let (cancel, _running) = downloads.begin(id, &closing).unwrap();

    let source = fixture.source();
    let task = tokio::spawn(async move {
        download_bundle(
            Http::default(),
            &source,
            SLUG,
            VERSION,
            &app_version(),
            &cancel,
            &mut |_| {},
        )
        .await
    });
    while fixture.files.requests().await.is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    downloads.cancel(id);

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("cancelled at once")
        .unwrap();
    assert_eq!(reason(result.unwrap_err()), kinds::DOWNLOAD_CANCELLED);
}

#[tokio::test]
async fn a_closing_vault_ends_a_running_download() {
    let bytes = b"bytes".to_vec();
    let fixture = Fixture::new(listed(&bytes, None), answer_for(&bytes)).await;
    fixture
        .files
        .reply(
            "/bundle.xt",
            ResponseTemplate::new(200)
                .set_body_bytes(bytes)
                .set_delay(Duration::from_secs(30)),
        )
        .await;
    let downloads = Downloads::default();
    let closing = CancellationToken::new();
    let (cancel, _running) = downloads.begin(Uuid::new_v4(), &closing).unwrap();

    let source = fixture.source();
    let task = tokio::spawn(async move {
        download_bundle(
            Http::default(),
            &source,
            SLUG,
            VERSION,
            &app_version(),
            &cancel,
            &mut |_| {},
        )
        .await
    });
    while fixture.files.requests().await.is_empty() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    closing.cancel();

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("ended at once")
        .unwrap();
    assert_eq!(reason(result.unwrap_err()), kinds::DOWNLOAD_CANCELLED);
}

#[test]
fn a_download_id_runs_once_and_is_free_again_when_it_ends() {
    let downloads = Downloads::default();
    let closing = CancellationToken::new();
    let id = Uuid::new_v4();

    let (token, guard) = downloads.begin(id, &closing).unwrap();
    assert!(downloads.begin(id, &closing).is_err());
    assert!(!token.is_cancelled());
    downloads.cancel(id);
    assert!(token.is_cancelled());
    downloads.cancel(Uuid::new_v4());

    drop(guard);
    assert!(downloads.begin(id, &closing).is_ok());
}

#[test]
fn only_https_or_the_scheme_of_the_marketplace_is_a_destination() {
    let https = "https://market.example";
    let http = "http://192.168.1.5:3000";
    let allowed = [
        (https, "https://files.example/a.xt?sig=1"),
        (http, "https://files.example/a.xt"),
        (http, "http://192.168.1.5:9000/a.xt"),
    ];
    for (base, url) in allowed {
        assert!(check_destination(base, url).is_ok(), "{base} -> {url}");
    }
    let refused = [
        (https, "http://files.example/a.xt"),
        (https, "file:///etc/passwd"),
        (http, "ftp://files.example/a.xt"),
        (https, "data:text/plain,hi"),
        (https, "not a url"),
        (https, "https://"),
    ];
    for (base, url) in refused {
        let error = check_destination(base, url).unwrap_err();
        assert_eq!(reason(error), kinds::DESTINATION_REFUSED, "{base} -> {url}");
    }
}

fn wire(size: Option<u64>, hash: Option<&str>, version: &str, url: &str) -> WireDownload {
    WireDownload {
        download_url: url.into(),
        version: version.into(),
        bundle_size: size,
        bundle_hash: hash.map(str::to_owned),
    }
}

#[test]
fn an_answer_without_size_or_checksum_names_no_target() {
    let hash = "ab".repeat(32);
    let base = "https://market.example";
    let url = "https://files.example/a.xt";
    for answer in [
        wire(None, Some(&hash), "1.0.0", url),
        wire(Some(10), None, "1.0.0", url),
        wire(Some(10), Some("abc"), "1.0.0", url),
        wire(Some(10), Some(&"zz".repeat(32)), "1.0.0", url),
    ] {
        let error = target_of(base, "1.0.0", answer).unwrap_err();
        assert_eq!(reason(error), kinds::NO_HASH);
    }
}

#[test]
fn an_answer_for_another_version_or_a_huge_bundle_names_no_target() {
    let hash = "AB".repeat(32);
    let base = "https://market.example";
    let url = "https://files.example/a.xt";

    let other = target_of(base, "1.0.0", wire(Some(10), Some(&hash), "1.0.1", url));
    assert_eq!(reason(other.unwrap_err()), kinds::SOURCE_BAD_RESPONSE);

    let huge = wire(Some(ARCHIVE_BYTES + 1), Some(&hash), "1.0.0", url);
    assert_eq!(
        reason(target_of(base, "1.0.0", huge).unwrap_err()),
        kinds::TOO_LARGE
    );

    let target = target_of(base, "1.0.0", wire(Some(10), Some(&hash), "1.0.0", url)).unwrap();
    assert_eq!(target.hash, hash.to_ascii_lowercase());
    assert_eq!(target.size, 10);
}

fn chunks(parts: Vec<Vec<u8>>) -> impl Stream<Item = std::result::Result<Vec<u8>, std::io::Error>> {
    futures::stream::iter(parts.into_iter().map(Ok))
}

#[tokio::test]
async fn collecting_stops_at_the_first_byte_over_the_limit_without_reading_on() {
    let pulled = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&pulled);
    let stream = futures::stream::iter(0..1000u32).map(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>(vec![0u8; 10])
    });
    let limits = Limits {
        max: 55,
        size: 55,
        hash: "00".repeat(32),
    };

    let result = collect(
        stream,
        limits,
        Duration::from_secs(20),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::TOO_LARGE);
    assert_eq!(
        pulled.load(Ordering::SeqCst),
        6,
        "the sixth chunk crosses 55"
    );
}

#[tokio::test(start_paused = true)]
async fn a_silent_server_ends_the_download_after_the_idle_time() {
    let stream = futures::stream::once(async { Ok::<_, std::io::Error>(vec![1u8]) })
        .chain(futures::stream::pending());

    let result = collect(
        stream,
        limits(10, &"00".repeat(32)),
        Duration::from_secs(20),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::SOURCE_TIMEOUT);
}

#[tokio::test(start_paused = true)]
async fn a_slow_but_steady_server_has_no_total_limit() {
    let stream = futures::stream::unfold(0u32, |n| async move {
        if n == 30 {
            return None;
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
        Some((Ok::<_, std::io::Error>(vec![n as u8]), n + 1))
    });
    let bytes: Vec<u8> = (0..30u8).collect();

    let result = collect(
        stream,
        limits(30, &sha256(&bytes)),
        Duration::from_secs(20),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(result.unwrap(), bytes);
}

#[tokio::test(start_paused = true)]
async fn cancelling_in_the_middle_returns_nothing() {
    let cancel = CancellationToken::new();
    let stream = futures::stream::once(async { Ok::<_, std::io::Error>(vec![1u8]) })
        .chain(futures::stream::pending());
    let canceller = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        canceller.cancel();
    });

    let result = collect(
        stream,
        limits(10, &"00".repeat(32)),
        Duration::from_secs(20),
        &cancel,
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::DOWNLOAD_CANCELLED);
}

#[tokio::test(start_paused = true)]
async fn progress_is_reported_at_most_every_interval_and_ends_complete() {
    let stream = futures::stream::unfold(0u32, |n| async move {
        if n == 20 {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
        Some((Ok::<_, std::io::Error>(vec![n as u8; 4]), n + 1))
    });
    let bytes: Vec<u8> = (0..20u8).flat_map(|n| [n; 4]).collect();
    let mut reports: Vec<(Instant, DownloadProgress)> = Vec::new();

    collect(
        stream,
        limits(80, &sha256(&bytes)),
        Duration::from_secs(20),
        &CancellationToken::new(),
        &mut |progress| reports.push((Instant::now(), progress)),
    )
    .await
    .unwrap();

    assert!(reports.len() < 20, "not one report per chunk");
    assert!(reports
        .windows(2)
        .all(|pair| pair[0].1.loaded <= pair[1].1.loaded));
    let (last, rest) = reports.split_last().unwrap();
    assert_eq!(
        last.1,
        DownloadProgress {
            loaded: 80,
            total: 80
        }
    );
    for pair in rest.windows(2) {
        assert!(pair[1].0 - pair[0].0 >= PROGRESS_INTERVAL);
    }
}

#[tokio::test]
async fn a_stream_that_breaks_is_unreachable() {
    let stream = futures::stream::iter(vec![Ok(vec![1u8, 2]), Err(std::io::Error::other("reset"))]);

    let result = collect(
        stream,
        limits(10, &"00".repeat(32)),
        Duration::from_secs(20),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::SOURCE_UNREACHABLE);
}

#[tokio::test]
async fn an_empty_body_is_a_size_mismatch_not_a_valid_bundle() {
    let result = collect(
        chunks(Vec::new()),
        limits(5, &sha256(b"")),
        Duration::from_secs(20),
        &CancellationToken::new(),
        &mut |_| {},
    )
    .await;

    assert_eq!(reason(result.unwrap_err()), kinds::SIZE_MISMATCH);
}
