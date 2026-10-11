//! The cleanup of T043: replaced sidecars and content objects nothing points at go only when the
//! provider changed them more than 24 h ago.

use super::*;
use crate::files::encrypted::format::encode_id;
use crate::files::encrypted::new_id;
use crate::files::encrypted::route::{load_view, open, route, Route};
use crate::files::encrypted::test_support::{filled, tree, version_of};
use crate::files::encrypted::test_support::{Scene, BUCKET};
use crate::files::storage_source::parse_iso_ms;
use crate::remote_storage::test_support::Op;
use crate::remote_storage::StorageError;

const NOW: &str = "2026-10-06T12:00:00.000Z";
const TWO_DAYS_AGO: &str = "2026-10-04T12:00:00.000Z";

fn now() -> i64 {
    parse_iso_ms(NOW).unwrap()
}

/// The folder's prefix `P`.
fn prefix(scene: &Scene) -> String {
    let head = scene
        .keys()
        .into_iter()
        .find(|key| key.ends_with(".hxef/h"))
        .unwrap();
    head[..head.len() - 1].to_owned()
}

async fn loaded(scene: &Scene) -> LoadedFolder {
    let files = scene.fresh_files();
    let Route::Encrypted(at) = route(&files, "/Unterlagen").await.unwrap() else {
        panic!("encrypted");
    };
    let folder = open(&files, &at).await.unwrap();
    load_view(&files, &folder).await.unwrap()
}

/// "Unterlagen" with `notiz.txt` renamed to `strom.txt`, its replaced sidecar left behind, and two
/// content objects nothing points at.
async fn with_leftovers() -> (Scene, String, [String; 3]) {
    let scene = filled(&tree(&[("notiz.txt", "Strom")])).await;
    let first = version_of(&scene.files(), "/Unterlagen/notiz.txt").await;
    scene.fake.fail_once(Op::Delete, StorageError::Network);
    scene
        .files()
        .rename("/Unterlagen/notiz.txt", "strom.txt")
        .await
        .unwrap();
    let p = prefix(&scene);
    let replaced = format!("{p}m/{}", encode_id(&first.sid));
    let old_orphan = format!("{p}c/{}", encode_id(&new_id()));
    let young_orphan = format!("{p}c/{}", encode_id(&new_id()));
    scene.fake.insert(BUCKET, &old_orphan, b"abandoned upload");
    scene
        .fake
        .insert(BUCKET, &young_orphan, b"upload of another device");
    (scene, p, [replaced, old_orphan, young_orphan])
}

#[tokio::test(flavor = "multi_thread")]
async fn only_leftovers_older_than_a_day_go() {
    let (scene, p, [replaced, old_orphan, young_orphan]) = with_leftovers().await;
    // Everything was written two hours before NOW; the two leftovers are two days old.
    for key in [&replaced, &old_orphan] {
        scene.fake.set_modified(BUCKET, key, TWO_DAYS_AGO);
    }
    let found = leftovers(&loaded(&scene).await, now());
    assert_eq!(found.sidecars.len(), 1);
    assert_eq!(found.contents.len(), 1);
    assert!(replaced.ends_with(&encode_id(&found.sidecars[0])));
    assert!(old_orphan.ends_with(&encode_id(&found.contents[0])));

    let files = scene.files();
    clean(files.store().as_ref(), files.access(), &p, &found).await;
    let keys = scene.keys();
    assert!(!keys.contains(&replaced));
    assert!(!keys.contains(&old_orphan));
    assert!(
        keys.contains(&young_orphan),
        "another device may be uploading"
    );
    let other = scene.fresh_files();
    let listed = other.list("/Unterlagen").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "strom.txt");
    assert_eq!(
        other
            .read_start("/Unterlagen/strom.txt", 5, 100)
            .await
            .unwrap(),
        b"Strom"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn young_leftovers_stay() {
    let (scene, ..) = with_leftovers().await;
    assert!(leftovers(&loaded(&scene).await, now()).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sidecar_that_does_not_open_keeps_every_content_object() {
    let (scene, p, [replaced, old_orphan, _]) = with_leftovers().await;
    let broken = format!("{p}m/{}", encode_id(&new_id()));
    scene
        .fake
        .insert(BUCKET, &broken, b"HXEF\x01\x02 not a sidecar");
    for key in [&replaced, &old_orphan, &broken] {
        scene.fake.set_modified(BUCKET, key, TWO_DAYS_AGO);
    }
    let found = leftovers(&loaded(&scene).await, now());
    assert!(
        found.contents.is_empty(),
        "the broken sidecar may point at one"
    );
    assert_eq!(found.sidecars.len(), 1, "the replaced sidecar still goes");
}
