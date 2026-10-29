use std::path::Path;

use tokio_util::sync::CancellationToken;

use super::*;

#[test]
fn releasing_a_drive_cancels_only_the_searches_below_it() {
    let searches = SearchManager::default();
    let token = |root: Option<&str>| {
        let cancel = CancellationToken::new();
        searches.track(root.map(PathBuf::from), cancel.clone());
        cancel
    };
    let (on_stick, below, beside, storage) = (
        token(Some("/media/u/STICK")),
        token(Some("/media/u/STICK/fotos")),
        token(Some("/media/u/STICKER")),
        token(None),
    );

    assert_eq!(searches.cancel_under(Path::new("/media/u/STICK")), 2);

    assert!(on_stick.is_cancelled() && below.is_cancelled());
    assert!(!beside.is_cancelled() && !storage.is_cancelled());
    assert_eq!(searches.running().len(), 2);
}
