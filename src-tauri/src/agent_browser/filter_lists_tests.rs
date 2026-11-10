use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;

use super::filter_fetch::{looks_like_list, require_https, ListFetcher};
use super::filter_lists::{FilterLists, REFRESH_AFTER};
use super::filter_settings::{CustomList, TrackerSettings, DEFAULT_LISTS};
use super::filters::RequestInfo;

/// Answers from a table and counts what was asked.
#[derive(Default)]
struct Fake {
    answers: Mutex<BTreeMap<String, Result<String, String>>>,
    asked: Mutex<Vec<String>>,
}

impl Fake {
    fn answer(&self, url: &str, answer: Result<&str, &str>) {
        self.answers.lock().unwrap().insert(
            url.to_owned(),
            answer.map(str::to_owned).map_err(str::to_owned),
        );
    }

    fn times(&self, url: &str) -> usize {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|u| *u == url)
            .count()
    }
}

#[async_trait]
impl ListFetcher for Fake {
    async fn get(&self, url: &str) -> Result<String, String> {
        self.asked.lock().unwrap().push(url.to_owned());
        self.answers
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .unwrap_or_else(|| Err("no such list".to_owned()))
    }
}

const MINE: &str = "https://lists.example/mine.txt";

/// Only the one custom list is on.
fn only_mine() -> TrackerSettings {
    TrackerSettings {
        enabled: true,
        off: DEFAULT_LISTS
            .iter()
            .map(|list| list.id.to_owned())
            .collect(),
        custom: vec![CustomList {
            id: "mine".to_owned(),
            name: "Mine".to_owned(),
            url: MINE.to_owned(),
            enabled: true,
        }],
    }
}

fn request(url: &str) -> RequestInfo {
    RequestInfo {
        url: url.to_owned(),
        source: "https://news.example/".to_owned(),
        kind: "Image".to_owned(),
        main_frame: false,
        method: "GET".to_owned(),
    }
}

fn setup(settings: TrackerSettings) -> (Arc<FilterLists>, Arc<Fake>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let fake = Arc::new(Fake::default());
    let lists = FilterLists::new(dir.path().to_path_buf(), fake.clone());
    lists.set_settings(settings);
    (lists, fake, dir)
}

#[test]
fn the_default_lists_are_those_ublock_origin_enables() {
    let ids: Vec<_> = DEFAULT_LISTS.iter().map(|list| list.id).collect();
    for expected in [
        "ublock-filters",
        "ublock-privacy",
        "easylist",
        "easyprivacy",
        "urlhaus",
    ] {
        assert!(ids.contains(&expected), "{expected}");
    }
    assert!(DEFAULT_LISTS
        .iter()
        .all(|list| require_https(list.url).is_ok()));
    // By default every default list is on and blocking is on.
    let settings = TrackerSettings::default();
    assert!(settings.enabled);
    assert_eq!(settings.active().len(), DEFAULT_LISTS.len());
}

#[test]
fn a_list_switched_off_and_a_custom_list_decide_what_counts() {
    let mut settings = TrackerSettings::default();
    settings.off.insert("easylist".to_owned());
    settings.custom.push(CustomList {
        id: "mine".to_owned(),
        name: "Mine".to_owned(),
        url: MINE.to_owned(),
        enabled: true,
    });
    settings.custom.push(CustomList {
        id: "paused".to_owned(),
        name: "Paused".to_owned(),
        url: "https://lists.example/paused.txt".to_owned(),
        enabled: false,
    });
    let ids: Vec<_> = settings.active().into_iter().map(|list| list.id).collect();
    assert!(!ids.contains(&"easylist".to_owned()));
    assert!(ids.contains(&"mine".to_owned()));
    assert!(!ids.contains(&"paused".to_owned()));
}

#[test]
fn only_an_https_address_that_leads_to_a_list_is_taken() {
    assert!(require_https("https://lists.example/a.txt").is_ok());
    assert!(require_https("http://lists.example/a.txt").is_err());
    assert!(require_https("file:///etc/passwd").is_err());
    assert!(require_https("not an address").is_err());
    assert!(looks_like_list("||ads.example^\n".to_owned()).is_ok());
    assert!(looks_like_list("  \n".to_owned()).is_err());
    assert!(looks_like_list("<!DOCTYPE html><html></html>".to_owned()).is_err());
    assert!(looks_like_list("\u{feff}\n<!-- a page -->\n<html></html>".to_owned()).is_err());
    assert!(looks_like_list("<?xml version=\"1.0\"?><Error/>".to_owned()).is_err());
    assert!(looks_like_list("\u{feff}[Adblock Plus 2.0]\n||ads.example^\n".to_owned()).is_ok());
}

#[tokio::test]
async fn lists_never_had_are_downloaded_and_the_engine_blocks_with_them() {
    let (lists, fake, dir) = setup(only_mine());
    fake.answer(MINE, Ok("||ads.example^\n"));
    let engine = lists.engine().await.expect("an engine");
    assert!(engine.blocks(&request("https://ads.example/b.png")));
    assert!(!engine.blocks(&request("https://news.example/logo.png")));
    assert!(dir.path().join("mine.txt").exists(), "kept on the device");
    // Asked again, the files are fresh: nothing is downloaded a second time.
    lists.engine().await.expect("an engine");
    assert_eq!(fake.times(MINE), 1);
}

#[tokio::test]
async fn a_list_that_fails_is_remembered_and_without_any_list_there_is_no_engine() {
    let (lists, fake, _dir) = setup(only_mine());
    fake.answer(MINE, Err("the server answered 404 Not Found"));
    assert!(lists.engine().await.is_none());
    assert_eq!(
        lists.error_of("mine").as_deref(),
        Some("the server answered 404 Not Found")
    );
}

#[tokio::test]
async fn blocking_switched_off_means_no_engine_and_no_download() {
    let mut settings = only_mine();
    settings.enabled = false;
    let (lists, fake, _dir) = setup(settings);
    fake.answer(MINE, Ok("||ads.example^\n"));
    assert!(lists.engine().await.is_none());
    assert_eq!(fake.times(MINE), 0);
}

#[tokio::test]
async fn a_stale_list_is_used_as_it_is_and_replaced_by_a_download_that_works() {
    let (lists, fake, dir) = setup(only_mine());
    let path = dir.path().join("mine.txt");
    std::fs::write(&path, "||old.example^\n").unwrap();
    let old = SystemTime::now() - REFRESH_AFTER - Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    fake.answer(MINE, Ok("||new.example^\n"));
    let live = lists.live().await.expect("blocking is on");
    let engine = live.get().expect("the old copy still blocks");
    assert!(engine.blocks(&request("https://old.example/b.png")));
    // The refresh runs on its own and rebuilds the engine afterwards, for a tab already open too.
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while !live
        .get()
        .is_some_and(|engine| engine.blocks(&request("https://new.example/b.png")))
    {
        assert!(std::time::Instant::now() < until, "refreshed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("new.example"));
    assert_eq!(fake.times(MINE), 1);
}

/// Lets a download finish only when the test says so.
struct Gated {
    open: tokio::sync::Semaphore,
}

#[async_trait]
impl ListFetcher for Gated {
    async fn get(&self, _url: &str) -> Result<String, String> {
        let _permit = self
            .open
            .acquire()
            .await
            .map_err(|error| error.to_string())?;
        Ok("||ads.example^\n".to_owned())
    }
}

#[tokio::test(start_paused = true)]
async fn a_list_that_arrives_after_the_first_wait_reaches_the_tabs_already_open() {
    let dir = tempfile::tempdir().unwrap();
    let gated = Arc::new(Gated {
        open: tokio::sync::Semaphore::new(0),
    });
    let lists = FilterLists::new(dir.path().to_path_buf(), gated.clone());
    lists.set_settings(only_mine());
    let live = lists.live().await.expect("blocking is on");
    assert!(
        live.get().is_none(),
        "the first wait is over, nothing is there yet"
    );
    gated.open.add_permits(1);
    for _ in 0..500 {
        if live.get().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let engine = live.get().expect("the late list reaches the open tab");
    assert!(engine.blocks(&request("https://ads.example/b.png")));
}

#[tokio::test]
async fn blocking_switched_off_takes_the_engine_from_open_tabs() {
    let (lists, fake, _dir) = setup(only_mine());
    fake.answer(MINE, Ok("||ads.example^\n"));
    let live = lists.live().await.expect("blocking is on");
    assert!(live.get().is_some());
    let mut settings = only_mine();
    settings.enabled = false;
    lists.set_settings(settings);
    assert!(lists.live().await.is_none());
    assert!(live.get().is_none(), "an open tab blocks nothing any more");
}

#[tokio::test]
async fn a_failed_refresh_keeps_the_last_good_copy() {
    let (lists, fake, dir) = setup(only_mine());
    let path = dir.path().join("mine.txt");
    std::fs::write(&path, "||keep.example^\n").unwrap();
    let old = SystemTime::now() - REFRESH_AFTER - Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    fake.answer(MINE, Err("offline"));
    let engine = lists.engine().await.expect("an engine");
    assert!(engine.blocks(&request("https://keep.example/b.png")));
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while lists.error_of("mine").is_none() {
        assert!(std::time::Instant::now() < until, "the failure is noted");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("keep.example"));
}

#[tokio::test]
async fn changed_settings_build_the_engine_anew() {
    let (lists, fake, _dir) = setup(only_mine());
    fake.answer(MINE, Ok("||ads.example^\n"));
    let first = lists.engine().await.expect("an engine");
    assert!(first.blocks(&request("https://ads.example/b.png")));
    let mut settings = only_mine();
    settings.custom[0].enabled = false;
    lists.set_settings(settings);
    assert!(lists.engine().await.is_none(), "no list is on");
}

#[tokio::test]
async fn the_choice_is_checked_and_applied_at_once() {
    let (lists, fake, _dir) = setup(TrackerSettings::default());
    fake.answer(MINE, Ok("||ads.example^\n"));
    let mut choice = only_mine();
    choice.custom[0].id = String::new();
    choice.custom[0].name = String::new();
    let kept = lists.apply(choice).await.expect("a valid choice");
    assert!(!kept.custom[0].id.is_empty(), "a new list gets an id");
    assert_eq!(kept.custom[0].name, "lists.example", "named by its host");
    assert_eq!(lists.settings(), kept, "it counts from now on");
    // Applied: the list that is new is downloaded without any browser asking.
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while fake.times(MINE) == 0 {
        assert!(std::time::Instant::now() < until, "the new list is fetched");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_choice_that_is_not_valid_is_refused_and_changes_nothing() {
    let (lists, _fake, _dir) = setup(TrackerSettings::default());
    let mut choice = only_mine();
    choice.custom[0].url = "http://lists.example/mine.txt".to_owned();
    assert!(lists.apply(choice).await.is_err());
    assert_eq!(lists.settings(), TrackerSettings::default());
}

#[tokio::test]
async fn the_overview_shows_every_list_with_its_file_its_error_and_what_is_loading() {
    let (lists, fake, _dir) = setup(only_mine());
    fake.answer(MINE, Err("the server answered 404 Not Found"));
    assert!(lists.engine().await.is_none());
    let overview = lists.overview();
    assert!(overview.enabled);
    assert_eq!(overview.lists.len(), DEFAULT_LISTS.len() + 1);
    let mine = overview
        .lists
        .iter()
        .find(|list| list.id == "mine")
        .unwrap();
    assert!(mine.custom && mine.enabled);
    assert_eq!(mine.fetched_at, None);
    assert_eq!(
        mine.error.as_deref(),
        Some("the server answered 404 Not Found")
    );
    let easylist = overview
        .lists
        .iter()
        .find(|list| list.id == "easylist")
        .unwrap();
    assert!(
        !easylist.custom && !easylist.enabled,
        "switched off in this choice"
    );
    assert!(!overview.loading, "nothing downloads any more");

    fake.answer(MINE, Ok("||ads.example^\n"));
    lists.apply(only_mine()).await.unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let shown = lists.overview();
        let mine = shown.lists.iter().find(|list| list.id == "mine").unwrap();
        if mine.fetched_at.is_some() && mine.error.is_none() {
            break;
        }
        assert!(std::time::Instant::now() < until, "the list arrives");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_new_list_already_counts_as_loading_in_the_answer_of_the_choice() {
    let (lists, fake, _dir) = setup(TrackerSettings::default());
    fake.answer(MINE, Ok("||ads.example^\n"));
    lists.apply(only_mine()).await.unwrap();
    // No download has to have started yet: the page polls only while this says so.
    assert!(lists.overview().loading);
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while lists.overview().loading {
        assert!(std::time::Instant::now() < until, "loading ends");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_list_that_is_removed_leaves_no_file_and_no_error_behind() {
    let (lists, fake, dir) = setup(TrackerSettings::default());
    fake.answer(MINE, Ok("||ads.example^\n"));
    lists.apply(only_mine()).await.unwrap();
    let file = dir.path().join("mine.txt");
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while !file.exists() {
        assert!(std::time::Instant::now() < until, "the list arrives");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut none = only_mine();
    none.custom.clear();
    lists.apply(none).await.unwrap();
    assert!(!file.exists());
    assert_eq!(lists.error_of("mine"), None);
}
