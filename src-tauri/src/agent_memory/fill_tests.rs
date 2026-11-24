//! Tests for the generated pages of the hand test (spec 097 T063).

use super::super::store;
use super::super::Author;
use super::*;
use crate::agent_profiles::test_support::vault;

#[tokio::test]
async fn it_writes_the_pages_asked_for_and_skips_titles_that_exist() {
    let v = vault();
    let author = Author::user(v.device);
    let first = {
        let author = author.clone();
        v.db.write(move |tx| store::settle(fill_in(tx, &author, 10, 3)))
            .await
            .expect("write")
            .expect("filled")
    };
    assert_eq!(first, 3);
    let second =
        v.db.write(move |tx| store::settle(fill_in(tx, &author, 20, 2)))
            .await
            .expect("write")
            .expect("filled");
    assert_eq!(second, 2);
    let titles: Vec<String> =
        v.db.read(|q| store::list(q))
            .await
            .expect("read")
            .into_iter()
            .map(|page| page.title)
            .collect();
    assert_eq!(titles.len(), 5);
    for n in 1..=5 {
        let title = format!("Testseite {n:04}");
        assert!(titles.contains(&title), "{title}");
    }
}
