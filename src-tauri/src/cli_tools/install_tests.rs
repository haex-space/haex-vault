//! Installing a tool: the happy path, and every way an attempt can end without touching what is
//! installed (spec 094, T015, FR-005 to FR-007, FR-022, SC-005). The programs are small shell
//! scripts served by a local server, so these run on Unix only.
#![cfg(unix)]

use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use flate2::write::GzEncoder;
use flate2::Compression;
use tar::{Builder, EntryType, Header};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{install, lock, InstallContext, InstallError, Phase, Progress, DOWNLOAD_FILE};
use crate::cli_tools::catalog::{Artifact, ArtifactKind, Catalog};
use crate::cli_tools::containment::EnvContainment;
use crate::cli_tools::installed::{InstallMarker, MARKER_FILE};
use crate::cli_tools::platform::PlatformKey;
use crate::cli_tools::record::Record;
use crate::models::hash::sha256_bytes_hex;

const PLATFORM: PlatformKey = PlatformKey::LinuxX64;
const VERSION: &str = "1.0.0";

fn script(body: &str) -> Vec<u8> {
    format!("#!/bin/sh\n{body}\n").into_bytes()
}

fn reports(version: &str) -> Vec<u8> {
    script(&format!("echo \"tool {version}\""))
}

fn artifact(url: &str, body: &[u8], kind: ArtifactKind, unpacked: Option<u64>) -> Artifact {
    Artifact {
        url: url.to_string(),
        sha256: sha256_bytes_hex(body),
        size: body.len() as u64,
        kind,
        unpacked_size: unpacked,
        requires_avx2: false,
    }
}

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    fn vendor_dir(&self, vendor: &str) -> std::path::PathBuf {
        self.dir.path().join("cli-tools").join(vendor)
    }

    fn context(&self, vendor: &str, artifact: Artifact) -> InstallContext {
        let definition = Catalog::shipped().vendor(vendor).unwrap().clone();
        InstallContext {
            vendor: Arc::new(definition),
            version: VERSION.to_string(),
            artifact,
            platform: PLATFORM,
            vendor_dir: self.vendor_dir(vendor),
            home: self.dir.path().join("home"),
            run_root: self.dir.path().join("home").join("run"),
            containment: Arc::new(EnvContainment),
            record: Record::new(self.dir.path().join("home").join("records")),
        }
    }

    /// Entries of the vendor folder other than the lock file.
    fn entries(&self, vendor: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.vendor_dir(vendor))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|name| name != ".install.lock")
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

async fn serve(body: Vec<u8>) -> (MockServer, String) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tool"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&server)
        .await;
    let url = format!("{}/tool", server.uri());
    (server, url)
}

async fn run(
    ctx: &InstallContext,
) -> Result<crate::cli_tools::installed::Installation, InstallError> {
    install(ctx, &CancellationToken::new(), |_| {}).await
}

fn tar_gz(build: impl FnOnce(&mut Builder<GzEncoder<Vec<u8>>>)) -> Vec<u8> {
    let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    build(&mut builder);
    builder.into_inner().unwrap().finish().unwrap()
}

fn add(builder: &mut Builder<GzEncoder<Vec<u8>>>, name: &str, mode: u32, data: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(mode);
    header.set_cksum();
    builder.append_data(&mut header, name, data).unwrap();
}

#[tokio::test]
async fn a_program_is_downloaded_checked_started_and_published() {
    let fixture = Fixture::new();
    let body = reports(VERSION);
    let (_server, url) = serve(body.clone()).await;
    let ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));
    let seen: Arc<Mutex<Vec<Progress>>> = Arc::default();
    let sink = seen.clone();

    let installation = install(&ctx, &CancellationToken::new(), move |p| {
        sink.lock().unwrap().push(p);
    })
    .await
    .unwrap();

    assert_eq!(installation.version, VERSION);
    assert_eq!(
        installation.executable,
        fixture.vendor_dir("claude").join(VERSION).join("claude")
    );
    assert!(!installation.confirmed);
    assert_eq!(fs::read(&installation.executable).unwrap(), body);
    assert_eq!(
        fixture.entries("claude"),
        [VERSION],
        "no staging folder is left"
    );
    let marker = InstallMarker::read(&installation.dir).unwrap();
    assert_eq!(marker.sha256, sha256_bytes_hex(&body));
    assert_eq!(marker.platform, "linux-x64");
    assert_eq!(marker.executable_size, body.len() as u64);
    assert!(installation.dir.join(MARKER_FILE).is_file());

    let phases: Vec<Phase> = seen.lock().unwrap().iter().map(|p| p.phase).collect();
    let mut firsts: Vec<Phase> = Vec::new();
    for phase in phases {
        if firsts.last() != Some(&phase) {
            firsts.push(phase);
        }
    }
    assert_eq!(
        firsts,
        [
            Phase::Downloading,
            Phase::Verifying,
            Phase::Unpacking,
            Phase::Publishing
        ]
    );
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.phase == Phase::Downloading && p.bytes_done == body.len() as u64));

    // The start check ran in a run folder of the vault, which is gone again.
    let run_root = fixture.dir.path().join("home").join("run");
    assert_eq!(fs::read_dir(run_root).map(|d| d.count()).unwrap_or(0), 0);
}

#[tokio::test]
async fn a_package_is_unpacked_and_its_program_found_at_the_catalog_path() {
    let fixture = Fixture::new();
    let program = reports(VERSION);
    let archive = tar_gz(|b| {
        add(b, "codex-package.json", 0o644, b"{}");
        add(b, "bin/codex", 0o755, &program);
    });
    let (_server, url) = serve(archive.clone()).await;
    let unpacked = 2 + program.len() as u64;
    let ctx = fixture.context(
        "codex",
        artifact(&url, &archive, ArtifactKind::TarGz, Some(unpacked)),
    );

    let installation = run(&ctx).await.unwrap();

    assert_eq!(
        installation.executable,
        installation.dir.join("bin").join("codex")
    );
    assert!(installation.dir.join("codex-package.json").is_file());
    assert!(
        !installation.dir.join(DOWNLOAD_FILE).exists(),
        "the archive is removed"
    );
    assert_eq!(fixture.entries("codex"), [VERSION]);
}

#[tokio::test]
async fn a_download_that_differs_from_the_list_is_never_unpacked_or_run() {
    let fixture = Fixture::new();
    // The program would leave a trace if it ran.
    let marker = fixture.dir.path().join("ran");
    let body = script(&format!(
        "touch {}\necho \"tool {VERSION}\"",
        marker.display()
    ));
    let (_server, url) = serve(body.clone()).await;
    let mut listed = artifact(&url, &body, ArtifactKind::File, None);
    listed.sha256 = sha256_bytes_hex(b"the file the list was made for");
    let ctx = fixture.context("claude", listed);

    let error = run(&ctx).await.unwrap_err();

    assert_eq!(error, InstallError::IntegrityMismatch);
    assert_eq!(error.code(), "cli_tools.integrity_mismatch");
    assert!(
        !marker.exists(),
        "a download that fails its check is not run"
    );
    assert!(
        fixture.entries("claude").is_empty(),
        "{:?}",
        fixture.entries("claude")
    );
}

#[tokio::test]
async fn a_download_of_another_size_than_listed_is_an_integrity_failure() {
    let fixture = Fixture::new();
    let body = reports(VERSION);
    let (_server, url) = serve(body.clone()).await;
    let mut listed = artifact(&url, &body, ArtifactKind::File, None);
    listed.size += 1;
    let ctx = fixture.context("claude", listed);
    assert_eq!(
        run(&ctx).await.unwrap_err(),
        InstallError::IntegrityMismatch
    );
    assert!(fixture.entries("claude").is_empty());
}

#[tokio::test]
async fn an_error_answer_names_its_status_and_leaves_nothing() {
    let fixture = Fixture::new();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tool"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let body = reports(VERSION);
    let ctx = fixture.context(
        "claude",
        artifact(
            &format!("{}/tool", server.uri()),
            &body,
            ArtifactKind::File,
            None,
        ),
    );

    let error = run(&ctx).await.unwrap_err();

    assert_eq!(
        error,
        InstallError::ServerError {
            status: "404".into()
        }
    );
    assert!(fixture.entries("claude").is_empty());
}

#[tokio::test]
async fn a_host_that_cannot_be_reached_is_offline() {
    let fixture = Fixture::new();
    // A port that was free a moment ago and has no listener now.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    };
    let body = reports(VERSION);
    let ctx = fixture.context(
        "claude",
        artifact(
            &format!("http://127.0.0.1:{port}/tool"),
            &body,
            ArtifactKind::File,
            None,
        ),
    );

    assert_eq!(run(&ctx).await.unwrap_err(), InstallError::Offline);
    assert!(fixture.entries("claude").is_empty());
}

/// A server that promises more bytes than it sends and closes the connection.
async fn cut_off_server(promised: usize, sent: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {promised}\r\nconnection: close\r\n\r\n"
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&vec![b'x'; sent]).await;
            let _ = socket.shutdown().await;
        }
    });
    format!("http://{address}/tool")
}

#[tokio::test]
async fn a_body_that_is_cut_off_leaves_no_half_version_and_no_partial_file() {
    let fixture = Fixture::new();
    let url = cut_off_server(2000, 300).await;
    let body = vec![b'x'; 2000];
    let ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));

    let error = run(&ctx).await.unwrap_err();

    assert_eq!(error, InstallError::Offline);
    assert!(
        fixture.entries("claude").is_empty(),
        "{:?}",
        fixture.entries("claude")
    );
}

#[tokio::test]
async fn a_cancelled_install_stops_at_once_and_removes_what_it_made() {
    let fixture = Fixture::new();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tool"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(reports(VERSION))
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let body = reports(VERSION);
    let ctx = fixture.context(
        "claude",
        artifact(
            &format!("{}/tool", server.uri()),
            &body,
            ArtifactKind::File,
            None,
        ),
    );
    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        canceller.cancel();
    });

    let started = std::time::Instant::now();
    let result = install(&ctx, &token, |_| {}).await;

    assert_eq!(result.unwrap_err(), InstallError::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(fixture.entries("claude").is_empty());
}

#[tokio::test]
async fn a_second_install_of_the_same_tool_in_the_vault_is_refused_while_one_runs() {
    let fixture = Fixture::new();
    let body = reports(VERSION);
    let (_server, url) = serve(body.clone()).await;
    let ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));
    fs::create_dir_all(&ctx.vendor_dir).unwrap();

    let held = lock(&ctx.vendor_dir).unwrap();
    assert_eq!(run(&ctx).await.unwrap_err(), InstallError::Locked);
    assert!(fixture.entries("claude").is_empty());
    drop(held);

    run(&ctx).await.expect("the lock ends with its holder");
}

#[tokio::test]
async fn too_little_free_space_is_refused_before_anything_is_downloaded() {
    let fixture = Fixture::new();
    let server = MockServer::start().await;
    let body = reports(VERSION);
    let mut huge = artifact(
        &format!("{}/tool", server.uri()),
        &body,
        ArtifactKind::File,
        None,
    );
    huge.size = u64::MAX / 4;
    let ctx = fixture.context("claude", huge);

    let error = run(&ctx).await.unwrap_err();

    assert!(matches!(error, InstallError::NoSpace { .. }), "{error:?}");
    assert_eq!(error.code(), "cli_tools.no_space");
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(fixture.entries("claude").is_empty());
}

#[tokio::test]
async fn a_vendor_without_a_program_for_the_platform_is_unsupported() {
    let fixture = Fixture::new();
    let catalog = Catalog::parse(
        r#"{"schema":1,"vendors":[{"vendor":"solo","display_name":"Solo",
        "download_hosts":["dl.example.com"],"executable":{"linux-x64":"solo"},
        "config_env":"SOLO_HOME","versions":[{"version":"1.0.0","platforms":{"linux-x64":
        {"url":"https://dl.example.com/solo","sha256":"0000000000000000000000000000000000000000000000000000000000000001",
        "size":10,"kind":"file"}}}]}]}"#,
    )
    .unwrap();
    let body = reports(VERSION);
    let mut ctx = fixture.context(
        "claude",
        artifact("http://127.0.0.1:9/x", &body, ArtifactKind::File, None),
    );
    ctx.vendor = Arc::new(catalog.vendor("solo").unwrap().clone());
    ctx.platform = PlatformKey::Win32X64;

    assert_eq!(
        run(&ctx).await.unwrap_err(),
        InstallError::UnsupportedPlatform
    );
}

#[tokio::test]
async fn a_program_that_fails_its_start_check_is_discarded_and_the_previous_version_stays() {
    let fixture = Fixture::new();
    let first = reports("0.9.0");
    let (_server, url) = serve(first.clone()).await;
    let mut older = fixture.context("claude", artifact(&url, &first, ArtifactKind::File, None));
    older.version = "0.9.0".into();
    let kept = run(&older).await.unwrap();

    for (body, reason) in [
        (script("exit 1"), "exit_status"),
        (reports("9.9.9"), "version_mismatch"),
        (reports("1.0.0-extra"), "version_mismatch"),
    ] {
        let (_server, url) = serve(body.clone()).await;
        let ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));
        let error = run(&ctx).await.unwrap_err();
        assert_eq!(error, InstallError::SmokeFailed { reason }, "{reason}");
        assert_eq!(error.code(), "cli_tools.smoke_failed");
        assert_eq!(
            fixture.entries("claude"),
            ["0.9.0"],
            "{reason}: nothing published"
        );
        assert!(
            kept.executable.is_file(),
            "the previous version is untouched"
        );
    }
}

#[tokio::test]
async fn a_program_that_cannot_start_fails_the_start_check() {
    let fixture = Fixture::new();
    let body = b"this is not a program".to_vec();
    let (_server, url) = serve(body.clone()).await;
    let ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));
    let error = run(&ctx).await.unwrap_err();
    assert_eq!(
        error,
        InstallError::SmokeFailed {
            reason: "start_failed"
        }
    );
    assert!(fixture.entries("claude").is_empty());
}

#[tokio::test]
async fn a_package_with_a_link_in_it_is_refused() {
    let fixture = Fixture::new();
    let archive = tar_gz(|b| {
        add(b, "bin/codex", 0o755, &reports(VERSION));
        let mut header = Header::new_gnu();
        header.set_entry_type(EntryType::Symlink);
        header.set_size(0);
        header.set_cksum();
        b.append_link(&mut header, "bin/evil", "/etc/passwd")
            .unwrap();
    });
    let (_server, url) = serve(archive.clone()).await;
    let ctx = fixture.context(
        "codex",
        artifact(&url, &archive, ArtifactKind::TarGz, Some(1000)),
    );

    let error = run(&ctx).await.unwrap_err();

    assert!(
        matches!(error, InstallError::UnpackRefused { .. }),
        "{error:?}"
    );
    assert!(fixture.entries("codex").is_empty());
}

#[tokio::test]
async fn a_package_without_its_program_is_refused() {
    let fixture = Fixture::new();
    let archive = tar_gz(|b| add(b, "readme.txt", 0o644, b"hello"));
    let (_server, url) = serve(archive.clone()).await;
    let ctx = fixture.context(
        "codex",
        artifact(&url, &archive, ArtifactKind::TarGz, Some(5)),
    );
    let error = run(&ctx).await.unwrap_err();
    assert!(
        matches!(error, InstallError::UnpackRefused { .. }),
        "{error:?}"
    );
    assert!(fixture.entries("codex").is_empty());
}

#[tokio::test]
async fn a_damaged_folder_of_the_same_version_is_replaced() {
    let fixture = Fixture::new();
    let body = reports(VERSION);
    let (_server, url) = serve(body.clone()).await;
    let ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));
    let broken = ctx.vendor_dir.join(VERSION);
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("claude"), "half a program").unwrap();

    let installation = run(&ctx).await.unwrap();

    assert_eq!(fs::read(installation.executable).unwrap(), body);
    assert_eq!(
        fixture.entries("claude"),
        [VERSION],
        "the damaged folder is gone"
    );
}

#[tokio::test]
async fn nothing_that_is_written_names_a_path_or_the_vault() {
    let fixture = Fixture::new();
    let body = reports("5.5.5");
    let (_server, url) = serve(body.clone()).await;
    let ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));
    let error = run(&ctx).await.unwrap_err();
    let text = error.to_string();
    let home = fixture.dir.path().to_string_lossy().into_owned();
    assert!(!text.contains(&home), "{text}");
    let record = fs::read_to_string(
        fixture
            .dir
            .path()
            .join("home")
            .join("records")
            .join("tools.log"),
    )
    .unwrap();
    assert!(record.contains("install_started"));
    assert!(record.contains("install_failed"));
    assert!(record.contains("code=smoke_failed"));
    assert!(!record.contains(&home) && !record.contains('/'), "{record}");
}

/// Installs `version` of Claude, a program that reports it, into the fixture's vault.
async fn put_version(
    fixture: &Fixture,
    version: &str,
) -> crate::cli_tools::installed::Installation {
    let body = reports(version);
    let (_server, url) = serve(body.clone()).await;
    let mut ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));
    ctx.version = version.to_string();
    run(&ctx).await.unwrap()
}

fn lease_of(fixture: &Fixture, version: &str) -> crate::cli_tools::uses::Lease {
    crate::cli_tools::uses::Lease::take(
        fixture.vendor_dir("claude"),
        version.to_string(),
        Arc::new(Catalog::shipped().vendor("claude").unwrap().clone()),
        Some(PLATFORM),
        None,
    )
}

#[tokio::test]
async fn an_update_installs_next_to_the_version_in_use_and_leaves_it_untouched() {
    let fixture = Fixture::new();
    let first = put_version(&fixture, "1.0.0").await;
    let before = fs::read(&first.executable).unwrap();

    let updated = put_version(&fixture, "1.1.0").await;

    assert_eq!(fixture.entries("claude"), ["1.0.0", "1.1.0"]);
    assert_eq!(fs::read(&first.executable).unwrap(), before);
    assert!(!updated.confirmed, "the new version has not run yet");
    let record = InstallMarker::read(&fixture.vendor_dir("claude").join("1.0.0")).unwrap();
    assert_eq!(record.version, "1.0.0");
}

#[tokio::test]
async fn the_series_of_an_update_leaves_one_folder_after_a_failure_two_after_a_good_update_and_one_after_the_first_run(
) {
    let fixture = Fixture::new();
    put_version(&fixture, "1.0.0").await;
    lease_of(&fixture, "1.0.0").confirm().unwrap();
    assert_eq!(fixture.entries("claude"), ["1.0.0"]);

    // An update whose program fails its start check: one folder, and no staging folder.
    let bad = script("exit 1");
    let (_server, url) = serve(bad.clone()).await;
    let mut ctx = fixture.context("claude", artifact(&url, &bad, ArtifactKind::File, None));
    ctx.version = "1.1.0".into();
    assert!(matches!(
        run(&ctx).await.unwrap_err(),
        InstallError::SmokeFailed { .. }
    ));
    assert_eq!(fixture.entries("claude"), ["1.0.0"]);

    // The same update with a program that works: two folders until the first run ends.
    put_version(&fixture, "1.1.0").await;
    assert_eq!(fixture.entries("claude"), ["1.0.0", "1.1.0"]);

    // The first normally ended run with the new version: one folder.
    lease_of(&fixture, "1.1.0").confirm().unwrap();
    assert_eq!(fixture.entries("claude"), ["1.1.0"]);

    // Another update, and a run still uses the version it replaces when the new one is
    // confirmed: the old folder waits for that run, then goes.
    put_version(&fixture, "1.2.0").await;
    let old_run = lease_of(&fixture, "1.1.0");
    lease_of(&fixture, "1.2.0").confirm().unwrap();
    assert_eq!(fixture.entries("claude"), ["1.1.0", "1.2.0"]);
    drop(old_run);
    assert_eq!(fixture.entries("claude"), ["1.2.0"]);
}

#[tokio::test]
async fn an_update_whose_download_fails_leaves_the_version_in_use_and_no_staging_folder() {
    let fixture = Fixture::new();
    let first = put_version(&fixture, "1.0.0").await;
    let body = reports("1.1.0");
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/tool"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let mut ctx = fixture.context(
        "claude",
        artifact(
            &format!("{}/tool", server.uri()),
            &body,
            ArtifactKind::File,
            None,
        ),
    );
    ctx.version = "1.1.0".into();

    assert!(matches!(
        run(&ctx).await.unwrap_err(),
        InstallError::ServerError { .. }
    ));

    assert_eq!(fixture.entries("claude"), ["1.0.0"]);
    assert!(first.executable.is_file());
}

#[tokio::test]
#[cfg(unix)]
async fn a_program_that_is_busy_for_a_moment_is_started_after_it() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("tool");
    // While this handle is open for writing, the kernel refuses to execute the file, as it does
    // when a forking thread has inherited the handle of a program an install has just written.
    let writer = fs::File::create(&program).unwrap();
    fs::write(&program, reports("1.0.0")).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let busy = std::process::Command::new(&program).spawn().unwrap_err();
    assert_eq!(
        busy.kind(),
        std::io::ErrorKind::ExecutableFileBusy,
        "the setup must make the program busy"
    );
    let mut command = tokio::process::Command::new(&program);
    let mut writer = Some(writer);
    let mut release_after_busy = || {
        drop(writer.take());
        std::future::ready(())
    };
    let mut child = super::spawn_retrying_busy(&mut command, &mut release_after_busy)
        .await
        .expect("started once the program is free");
    assert!(child.wait().await.unwrap().success());
}

#[tokio::test]
async fn a_program_that_cannot_be_started_fails_at_once_and_the_record_says_why() {
    let fixture = Fixture::new();
    // The interpreter does not exist, so the program can never start.
    let body = b"#!/no/such/interpreter\necho \"tool 1.0.0\"\n".to_vec();
    let (_server, url) = serve(body.clone()).await;
    let ctx = fixture.context("claude", artifact(&url, &body, ArtifactKind::File, None));

    let error = run(&ctx).await.unwrap_err();

    assert!(matches!(
        error,
        InstallError::SmokeFailed {
            reason: "start_failed"
        }
    ));
    let record = fs::read_to_string(
        fixture
            .dir
            .path()
            .join("home")
            .join("records")
            .join("tools.log"),
    )
    .unwrap();
    assert!(record.contains("start_check_failed"), "{record}");
    assert!(record.contains("kind=NotFound"), "{record}");
    assert!(!record.contains('/'), "{record}");
}

// A zip package, as the Windows builds of a vendor come (spec 096).

fn zipped(build: impl FnOnce(&mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>)) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    build(&mut writer);
    writer.finish().unwrap().into_inner()
}

fn add_zipped(
    writer: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
    name: &str,
    mode: u32,
    data: &[u8],
) {
    use std::io::Write;
    let options = zip::write::SimpleFileOptions::default().unix_permissions(mode);
    writer.start_file(name, options).unwrap();
    writer.write_all(data).unwrap();
}

#[tokio::test]
async fn a_zip_package_is_unpacked_and_its_program_found_at_the_catalog_path() {
    let fixture = Fixture::new();
    let program = reports(VERSION);
    let archive = zipped(|w| {
        add_zipped(w, "codex-package.json", 0o644, b"{}");
        add_zipped(w, "bin/codex", 0o755, &program);
    });
    let (_server, url) = serve(archive.clone()).await;
    let unpacked = 2 + program.len() as u64;
    let ctx = fixture.context(
        "codex",
        artifact(&url, &archive, ArtifactKind::Zip, Some(unpacked)),
    );

    let installation = run(&ctx).await.unwrap();

    assert_eq!(
        installation.executable,
        installation.dir.join("bin").join("codex")
    );
    assert!(installation.dir.join("codex-package.json").is_file());
    assert!(
        !installation.dir.join(DOWNLOAD_FILE).exists(),
        "the archive is removed"
    );
    assert_eq!(fixture.entries("codex"), [VERSION]);
}

#[tokio::test]
async fn a_zip_with_an_entry_that_leaves_the_folder_is_refused_and_leaves_nothing() {
    let fixture = Fixture::new();
    let program = reports(VERSION);
    let archive = zipped(|w| {
        add_zipped(w, "bin/codex", 0o755, &program);
        add_zipped(w, "../escape.txt", 0o644, b"evil");
    });
    let (_server, url) = serve(archive.clone()).await;
    let ctx = fixture.context(
        "codex",
        artifact(
            &url,
            &archive,
            ArtifactKind::Zip,
            Some(program.len() as u64 + 4),
        ),
    );

    let error = run(&ctx).await.err().unwrap();

    assert!(
        matches!(error, InstallError::UnpackRefused { .. }),
        "{error:?}"
    );
    assert!(fixture.entries("codex").is_empty());
    assert!(!fixture.dir.path().join("escape.txt").exists());
}

#[tokio::test]
async fn a_zip_larger_than_the_catalog_lists_is_refused() {
    let fixture = Fixture::new();
    let program = reports(VERSION);
    let archive = zipped(|w| {
        add_zipped(w, "bin/codex", 0o755, &program);
        add_zipped(w, "big.bin", 0o644, &vec![0u8; 3 * 1024 * 1024]);
    });
    let (_server, url) = serve(archive.clone()).await;
    let ctx = fixture.context(
        "codex",
        artifact(
            &url,
            &archive,
            ArtifactKind::Zip,
            Some(program.len() as u64),
        ),
    );

    let error = run(&ctx).await.err().unwrap();

    assert!(
        matches!(error, InstallError::UnpackRefused { .. }),
        "{error:?}"
    );
    assert!(fixture.entries("codex").is_empty());
}
