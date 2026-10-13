//! A search of the device goes past the raw directories of encrypted folders and on into them by
//! their decrypted names (spec 048 US7, FR-049, T076).

use tokio_util::sync::CancellationToken;

use super::*;
use crate::files::encrypted::device_scene::DeviceScene;
use crate::files::local::places::OwnPlaces;
use crate::files::search::{search_passing, SearchFilters, SearchLimits};

fn options() -> SearchOptions {
    SearchOptions {
        filters: SearchFilters::default(),
        show_hidden: false,
        own: OwnPlaces::default(),
        hide_own: false,
        limits: SearchLimits::USER,
    }
}

/// The device paths of the hits for `query` from the home folder: the walk, then the encrypted
/// folders it passed.
async fn hits(device: &DeviceScene, query: &str) -> Vec<String> {
    let cancel = CancellationToken::new();
    let mut found = Vec::new();
    let mut passed = Vec::new();
    let end = search_passing(
        device.home.path(),
        query,
        &options(),
        &cancel,
        &mut |hits| found.extend(hits),
        &mut passed,
    );
    assert_eq!(end, SearchEnd::Done { truncated: false });
    assert_eq!(passed.len(), 1, "the walk passed the encrypted folder");
    let walked = found.len();
    let end = search_passed(
        &device.files(),
        &device.root,
        passed,
        query,
        &options(),
        walked,
        &cancel,
        &mut |hits| found.extend(hits),
    )
    .await;
    assert_eq!(end, SearchEnd::Done { truncated: false });
    let mut paths: Vec<String> = found.into_iter().map(|hit| hit.entry.path).collect();
    paths.sort();
    paths
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_search_finds_by_decrypted_names_and_never_raw_files() {
    let device = DeviceScene::new();
    device.with_folder().await;
    std::fs::write(device.home.path().join("notizbuch.txt"), "x").unwrap();
    let home = device.home.path();
    assert_eq!(
        hits(&device, "notiz").await,
        [
            home.join("Unterlagen/notiz.txt")
                .to_string_lossy()
                .into_owned(),
            home.join("notizbuch.txt").to_string_lossy().into_owned(),
        ]
    );
    assert_eq!(
        hits(&device, "Unterlagen").await,
        [home.join("Unterlagen").to_string_lossy().into_owned()]
    );
    // The raw names (`h`, the random directory) are never hits.
    assert!(hits(&device, "h")
        .await
        .iter()
        .all(|path| !path.contains(".hxef")));
}
