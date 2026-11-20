//! Throughput of the peer path on loopback (no network in between, so this is haex-vault's own cost):
//! 20 files of 6 MB read from and written to another own device the way a sync rule does it. Never
//! in CI; run it with
//!
//! ```text
//! cargo test --lib --release -- --ignored files::peer::perf_tests --nocapture
//! ```

use std::sync::Arc;
use std::time::Instant as Clock;

use tokio::io::AsyncReadExt;
use tokio::time::{Duration, Instant};

use super::store::{peer_access, PeerStore};
use super::test_support::{own_pair, Fixed};
use super::wire::Caller;
use crate::files::transfer::local::CHUNK;
use crate::files::transfer::remote::PART;
use crate::remote_storage::ObjectStore;

const FILES: usize = 20;
const SIZE: usize = 6 * 1000 * 1000;

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(120)
}

fn mb_s(bytes: usize, took: Duration) -> f64 {
    bytes as f64 / 1_000_000.0 / took.as_secs_f64()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, see the module doc"]
async fn twenty_files_of_six_megabytes_each_way() {
    let (main, linked) = own_pair().await;
    let body: Vec<u8> = (0..SIZE).map(|i| (i % 251) as u8).collect();
    for i in 0..FILES {
        main.write(&format!("Fotos/img{i:02}.jpg"), &body);
    }
    let link = Arc::new(linked.link_to(&main).await.expect("link"));
    let store = PeerStore::new(Arc::new(Fixed(link)), Caller::User);
    let access = peer_access(&format!("peer:{}", main.uuid), None);
    let base = main
        .root()
        .to_string_lossy()
        .trim_start_matches('/')
        .to_owned();

    // Download, as `Run::fetch` does: one range per file, read in CHUNKs.
    let started = Clock::now();
    let mut buf = vec![0u8; CHUNK];
    for i in 0..FILES {
        let key = format!("{base}/Fotos/img{i:02}.jpg");
        let mut reader = store
            .get_range(&access, &key, 0, SIZE as u64, soon())
            .await
            .expect("range");
        let mut got = 0;
        loop {
            let n = reader.read(&mut buf).await.expect("read");
            if n == 0 {
                break;
            }
            got += n;
        }
        assert_eq!(got, SIZE);
    }
    let took = started.elapsed();
    println!(
        "download {FILES} x {SIZE} bytes: {took:?} = {:.1} MB/s",
        mb_s(FILES * SIZE, took)
    );

    // Upload, as `Run::upload` does: parts of PART, one after the other.
    let started = Clock::now();
    for i in 0..FILES {
        let key = format!("{base}/Hoch/img{i:02}.jpg");
        let id = store
            .begin_upload(&access, &key, soon())
            .await
            .expect("begin");
        let mut marks = Vec::new();
        for (n, part) in body.chunks(PART).enumerate() {
            let mark = store
                .upload_part(&access, &key, &id, (n + 1) as u16, part.to_vec(), soon())
                .await
                .expect("part");
            marks.push(mark);
        }
        store
            .finish_upload(&access, &key, &id, &marks, soon())
            .await
            .expect("finish");
    }
    let took = started.elapsed();
    println!(
        "upload   {FILES} x {SIZE} bytes: {took:?} = {:.1} MB/s",
        mb_s(FILES * SIZE, took)
    );
}
