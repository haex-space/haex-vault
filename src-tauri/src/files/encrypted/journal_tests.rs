//! A reload that began before this device wrote keeps what it wrote (research R7): a folder and its
//! sub folder written while the folder loaded again do not end up in "Wiederhergestellt".

use super::super::route::{load_view, open, route, view, Route};
use super::super::test_support::{filled, tree};

#[tokio::test(flavor = "multi_thread")]
async fn a_reload_that_began_before_a_write_keeps_it() {
    let scene = filled(&tree(&[("a.txt", "a")])).await;
    let files = scene.files();
    let Route::Encrypted(at) = route(&files, "/Unterlagen").await.unwrap() else {
        panic!("encrypted");
    };
    let folder = open(&files, &at).await.unwrap();
    drop(view(&files, &folder).await.unwrap());
    let mark = folder.mark().await;
    let stale = load_view(&files, &folder).await.unwrap();
    files.create_folder("/Unterlagen", "Belege").await.unwrap();
    files
        .create_folder("/Unterlagen/Belege", "d0")
        .await
        .unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    files
        .delete("/Unterlagen/a.txt", &cancel, &mut |_| {})
        .await
        .unwrap();
    folder.reload(stale, mark).await;

    let mut names: Vec<String> = files
        .list("/Unterlagen")
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    assert_eq!(names, ["Belege"], "no a.txt, no Wiederhergestellt");
    let inside = files.list("/Unterlagen/Belege").await.unwrap();
    assert_eq!(inside.len(), 1);
    assert_eq!(inside[0].name, "d0");
}
