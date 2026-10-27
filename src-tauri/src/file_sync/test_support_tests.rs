//! The place in memory behaves like a place: listings, moves into the rule trash, failures and an
//! unreachable switch.

use super::*;

#[tokio::test]
async fn it_lists_moves_and_fails_on_demand() {
    let place = MemPlace::new();
    place.put("a/b.txt", b"bb", 5);
    let listing = place.list_tree(&CancellationToken::new()).await.unwrap();
    assert_eq!(listing.files["a/b.txt"].size, 2);
    assert!(listing.folders.contains("a"));
    place.trash("a/b.txt", "run").await.unwrap();
    assert_eq!(place.paths(), [".holzi-trash/run/a/b.txt"]);
    assert_eq!(place.calls("trash"), 1);
    place.fail(".holzi-trash/run/a/b.txt");
    assert!(place.delete(".holzi-trash/run/a/b.txt").await.is_err());
    place.set_unreachable(true);
    assert_eq!(
        place
            .list_tree(&CancellationToken::new())
            .await
            .unwrap_err()
            .code,
        FilesErrorCode::Unreachable
    );
}
