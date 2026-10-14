//! The measurement of T066 (SC-003, SC-005) against a real S3 provider, never in CI: start a RustFS
//! and run
//!
//! ```text
//! HOLZI_PERF_S3=http://127.0.0.1:<port> HOLZI_PERF_KEY=<key> HOLZI_PERF_SECRET=<secret> \
//!   HOLZI_PERF_BUCKET=<bucket> [HOLZI_PERF_LARGE=1] \
//!   cargo test --lib -- --ignored files::encrypted::perf --nocapture
//! ```
//!
//! It prints the times and sizes; the PR records them.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::ops::create_encrypted_folder;
use super::test_support::{give_key, transfer};
use super::{EncryptedFolders, Scope};
use crate::files::storage_source::StorageFiles;
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;
use crate::remote_storage::s3::S3Store;
use crate::remote_storage::test_support::vault;
use crate::remote_storage::{
    Access, Addressing, Credentials, EndpointScope, Location, Protocol, ProviderKind, S3Credentials,
};
use crate::vault_gate::VaultDb;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

fn access() -> Access {
    Access {
        location: Location {
            protocol: Protocol::S3,
            provider_kind: ProviderKind::Rustfs,
            endpoint: env("HOLZI_PERF_S3"),
            endpoint_scope: EndpointScope::Local,
            region: "us-east-1".to_owned(),
            addressing: Addressing::Path,
            bucket: env("HOLZI_PERF_BUCKET"),
            host_key: None,
        },
        credentials: Credentials::S3(S3Credentials {
            access_key_id: env("HOLZI_PERF_KEY"),
            secret_access_key: Zeroizing::new(env("HOLZI_PERF_SECRET")),
            session_token: None,
        }),
    }
}

/// The storage with a scope; `folders` is this "device's" memory of opened folders.
fn files(db: &VaultDb, folders: Arc<EncryptedFolders>) -> StorageFiles {
    let db = db.clone();
    StorageFiles::new(Arc::new(S3Store::system()), access()).with_scope(Scope {
        storage_id: "perf".to_owned(),
        folders,
        vault: Arc::new(move || Ok(db.clone())),
        background: Arc::new(|_| {}),
    })
}

/// Forgets every cached head and sidecar: the first open on a device.
fn forget_cache(db: &VaultDb) {
    db.write_blocking(|tx| {
        tx.execute("DELETE FROM encrypted_folder_heads_no_sync", &[])?;
        tx.execute("DELETE FROM encrypted_folder_entries_no_sync", &[])?;
        Ok(())
    })
    .unwrap();
}

/// `count` small files copied into `to`.
async fn fill(files: &StorageFiles, to: &str, count: usize) {
    let dir = tempfile::tempdir().unwrap();
    let sources: Vec<String> = (0..count)
        .map(|n| {
            let file = dir.path().join(format!("datei-{n:05}.txt"));
            std::fs::write(&file, format!("Inhalt {n}")).unwrap();
            file.to_string_lossy().into_owned()
        })
        .collect();
    let started = Instant::now();
    transfer(
        TransferOp::Copy,
        Side::Device,
        &sources,
        Side::Storage(Arc::new(files_clone(files))),
        to,
        None,
        None,
    )
    .await
    .unwrap();
    println!("  filling {count} files took {:?}", started.elapsed());
}

fn files_clone(files: &StorageFiles) -> StorageFiles {
    StorageFiles::new(files.store().clone(), files.access().clone())
        .with_scope(files.scope().unwrap().clone())
}

/// The time of a first open (nothing cached) and of a second one (cached, fresh memory).
async fn opens(db: &VaultDb, path: &str, entries: usize) -> (Duration, Duration) {
    forget_cache(db);
    let started = Instant::now();
    let listed = files(db, Arc::default()).list(path).await.unwrap();
    let first = started.elapsed();
    assert_eq!(listed.len(), entries);
    let started = Instant::now();
    files(db, Arc::default()).list(path).await.unwrap();
    (first, started.elapsed())
}

async fn object_bytes(files: &StorageFiles, prefix: &str) -> u64 {
    files
        .store()
        .list(
            files.access(),
            prefix,
            100_000,
            tokio::time::Instant::now() + Duration::from_secs(60),
        )
        .await
        .unwrap()
        .iter()
        .map(|object| object.size)
        .sum()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a real S3 provider, see the module comment"]
async fn perf_against_a_real_provider() {
    let (_dir, db) = vault();
    give_key(&db, 1);
    let run = format!("perf-{}", uuid::Uuid::new_v4().simple());
    let window = files(&db, Arc::default());
    window.create_folder("/", &run).await.unwrap();
    let base = format!("/{run}");

    println!("SC-003 1 000 entries:");
    create_encrypted_folder(&window, &base, "Tausend")
        .await
        .unwrap();
    fill(&window, &format!("{base}/Tausend"), 1_000).await;
    let (first, second) = opens(&db, &format!("{base}/Tausend"), 1_000).await;
    println!("  first open {first:?} (< 5 s), second {second:?} (< 1 s)");

    if std::env::var("HOLZI_PERF_LARGE").is_ok() {
        println!("SC-003 10 000 entries:");
        create_encrypted_folder(&window, &base, "Zehntausend")
            .await
            .unwrap();
        fill(&window, &format!("{base}/Zehntausend"), 10_000).await;
        let (first, second) = opens(&db, &format!("{base}/Zehntausend"), 10_000).await;
        println!("  first open {first:?} (< 30 s), second {second:?}");
        println!("  for comparison, an ordinary folder:");
        window.create_folder(&base, "Offen10k").await.unwrap();
        fill(&window, &format!("{base}/Offen10k"), 10_000).await;
    }

    println!("SC-003 20 heads:");
    let heads = format!("{base}/Koepfe");
    window.create_folder(&base, "Koepfe").await.unwrap();
    for n in 0..20 {
        create_encrypted_folder(&window, &heads, &format!("Ordner {n:02}"))
            .await
            .unwrap();
    }
    let (first, second) = opens(&db, &heads, 20).await;
    println!("  first listing {first:?} (< 2 s), second {second:?}");

    println!("SC-005 8 MiB file:");
    let large = tempfile::tempdir().unwrap();
    let file = large.path().join("gross.bin");
    let data: Vec<u8> = (0..8 * 1024 * 1024u32).map(|n| (n % 251) as u8).collect();
    std::fs::write(&file, &data).unwrap();
    let source = [file.to_string_lossy().into_owned()];
    create_encrypted_folder(&window, &base, "Gross")
        .await
        .unwrap();
    window.create_folder(&base, "Offen").await.unwrap();
    // Three rounds each, the median counts; downloads go through the window that uploaded, which has
    // the folders open, as when the user starts a download from it.
    let reader = files_clone(&window);
    let median = |mut runs: Vec<Duration>| {
        runs.sort();
        runs[runs.len() / 2]
    };
    let mut times = Vec::new();
    for target in ["Offen", "Gross"] {
        let to = format!("{base}/{target}");
        let (mut ups, mut downs) = (Vec::new(), Vec::new());
        for _ in 0..3 {
            let started = Instant::now();
            transfer(
                TransferOp::Copy,
                Side::Device,
                &source,
                Side::Storage(Arc::new(files_clone(&window))),
                &to,
                Some(crate::files::transfer::ConflictChoice::Replace),
                None,
            )
            .await
            .unwrap();
            ups.push(started.elapsed());
            let back = tempfile::tempdir().unwrap();
            let started = Instant::now();
            transfer(
                TransferOp::Copy,
                Side::Storage(Arc::new(files_clone(&reader))),
                &[format!("{to}/gross.bin")],
                Side::Device,
                &back.path().to_string_lossy(),
                None,
                None,
            )
            .await
            .unwrap();
            downs.push(started.elapsed());
            assert_eq!(std::fs::read(back.path().join("gross.bin")).unwrap(), data);
        }
        let (up, down) = (median(ups), median(downs));
        times.push((up, down));
        println!("  {target}: up {up:?}, down {down:?} (median of 3)");
    }
    let plain = object_bytes(&window, &format!("{run}/Offen/")).await;
    let mut gross = 0;
    for object in window
        .store()
        .list(
            window.access(),
            &format!("{run}/"),
            100_000,
            tokio::time::Instant::now() + Duration::from_secs(60),
        )
        .await
        .unwrap()
    {
        if object.key.contains("/c/") && object.size > 1_000_000 {
            gross += object.size;
        }
    }
    println!(
        "  size: plain {plain} B, encrypted content {gross} B (+{:.3} %, <= 1 %)",
        (gross as f64 / plain as f64 - 1.0) * 100.0
    );
    let ratio = |(plain, encrypted): (Duration, Duration)| {
        (encrypted.as_secs_f64() / plain.as_secs_f64() - 1.0) * 100.0
    };
    println!(
        "  slower: up {:+.1} %, down {:+.1} % (<= 20 %)",
        ratio((times[0].0, times[1].0)),
        ratio((times[0].1, times[1].1))
    );
    let cancel = CancellationToken::new();
    window.delete(&base, &cancel, &mut |_| {}).await.unwrap();
}
