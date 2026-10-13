use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use super::*;
use crate::files::encrypted::cache::HeadRow;
use crate::files::encrypted::format::encode_id;
use crate::files::encrypted::ops::create_encrypted_folder;
use crate::files::encrypted::route::{open, route, Route};
use crate::files::encrypted::test_support::{Scene, BUCKET};
use crate::files::encrypted::OpenFolder;
use crate::files::storage_source::StorageFiles;
use crate::remote_storage::test_support::{access, FakeStore, Op};
use crate::remote_storage::{DirListing, ObjectHead, ObjectInfo};

async fn opened(files: &StorageFiles, path: &str) -> Arc<OpenFolder> {
    let Route::Encrypted(at) = route(files, path).await.unwrap() else {
        panic!("{path} is not encrypted");
    };
    open(files, &at).await.unwrap()
}

fn count(fake: &FakeStore, op: Op) -> usize {
    fake.calls().iter().filter(|(o, _)| *o == op).count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_loads_with_one_listing_and_reads_only_new_sidecars() {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    for name in ["a", "b", "c"] {
        files.create_folder("/Unterlagen", name).await.unwrap();
    }
    let folder = opened(&files, "/Unterlagen").await;
    let load = |cached: Vec<EntryRow>| {
        let (fake, folder) = (scene.fake.clone(), Arc::clone(&folder));
        async move {
            load_folder(
                fake.as_ref(),
                &access(BUCKET),
                &folder.prefix,
                &folder.fid,
                folder.keys(),
                cached,
            )
            .await
            .unwrap()
        }
    };

    let (lists, gets) = (count(&scene.fake, Op::List), count(&scene.fake, Op::Get));
    let first = load(Vec::new()).await;
    assert_eq!(first.versions.len(), 3);
    assert_eq!(first.fetched.len(), 3);
    assert_eq!(count(&scene.fake, Op::List), lists + 1, "one listing");
    assert_eq!(count(&scene.fake, Op::Get), gets + 3);

    let gets = count(&scene.fake, Op::Get);
    let second = load(first.versions.clone()).await;
    assert_eq!(second.versions.len(), 3);
    assert!(second.fetched.is_empty());
    assert_eq!(
        count(&scene.fake, Op::Get),
        gets,
        "known ETags need no read"
    );

    files.create_folder("/Unterlagen", "d").await.unwrap();
    let gone = first.versions[0].sid;
    scene
        .fake
        .delete(
            &access(BUCKET),
            &folder.sidecar_key(&gone),
            tokio::time::Instant::now(),
        )
        .await
        .unwrap();
    let gets = count(&scene.fake, Op::Get);
    let third = load(first.versions.clone()).await;
    assert_eq!(
        count(&scene.fake, Op::Get),
        gets + 1,
        "only the new sidecar"
    );
    assert_eq!(third.vanished, [gone]);
    assert_eq!(third.versions.len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sidecar_that_does_not_open_is_broken_and_the_others_load() {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    files.create_folder("/Unterlagen", "a").await.unwrap();
    files.create_folder("/Unterlagen", "b").await.unwrap();
    let folder = opened(&files, "/Unterlagen").await;
    let first = load_folder(
        scene.fake.as_ref(),
        &access(BUCKET),
        &folder.prefix,
        &folder.fid,
        folder.keys(),
        Vec::new(),
    )
    .await
    .unwrap();
    let key = folder.sidecar_key(&first.versions[0].sid);
    let mut body = scene.fake.object(BUCKET, &key).unwrap();
    let last = body.len() - 1;
    body[last] ^= 1;
    scene.fake.insert(BUCKET, &key, &body);
    let again = load_folder(
        scene.fake.as_ref(),
        &access(BUCKET),
        &folder.prefix,
        &folder.fid,
        folder.keys(),
        first.versions.clone(),
    )
    .await
    .unwrap();
    assert_eq!(again.broken, [first.versions[0].sid]);
    assert_eq!(again.versions.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn heads_are_read_in_full_only_for_a_new_etag() {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    create_encrypted_folder(&files, "/", "Fotos").await.unwrap();
    let listing = scene
        .fake
        .list_dir(&access(BUCKET), "", 100, tokio::time::Instant::now())
        .await
        .unwrap();
    let held = scene.db.read(|q| HeldKeys::load(q)).await.unwrap();
    let load = |cached: Vec<HeadRow>| {
        let (fake, prefixes, held) = (scene.fake.clone(), listing.prefixes.clone(), &held);
        async move {
            load_heads(fake.as_ref(), &access(BUCKET), &prefixes, &cached, held)
                .await
                .unwrap()
        }
    };
    let (heads, gets) = (count(&scene.fake, Op::Head), count(&scene.fake, Op::Get));
    let first = load(Vec::new()).await;
    assert_eq!(count(&scene.fake, Op::Head), heads + 2);
    assert_eq!(count(&scene.fake, Op::Get), gets + 2);
    let mut names: Vec<_> = first.iter().filter_map(|h| h.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Fotos", "Unterlagen"]);
    assert!(first.iter().all(|h| h.fresh && h.fk.is_some()));

    let rows: Vec<HeadRow> = first
        .iter()
        .map(|head| HeadRow {
            object_key: format!("{}h", head.prefix),
            etag: head.etag.clone().unwrap(),
            folder_id: encode_id(&head.fid.unwrap()),
            name: head.name.clone(),
            state: head.state,
            key_id: head.key_id,
            open_ack: false,
        })
        .collect();
    let gets = count(&scene.fake, Op::Get);
    let second = load(rows.clone()).await;
    assert_eq!(
        count(&scene.fake, Op::Get),
        gets,
        "known heads need no read"
    );
    assert!(second
        .iter()
        .all(|h| !h.fresh && h.state == EncryptedState::Readable));

    // Another vault's state is never trusted from the cache.
    let mut other = rows;
    other[0].state = EncryptedState::OtherVault;
    let gets = count(&scene.fake, Op::Get);
    load(other).await;
    assert_eq!(count(&scene.fake, Op::Get), gets + 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_head_or_a_wrong_id_is_damaged() {
    let scene = Scene::new();
    let held = scene.db.read(|q| HeldKeys::load(q)).await.unwrap();
    let prefixes = [
        "aaaaaaaaaaaaaaaaaaaaaaaaaa.hxef/".to_owned(),
        "aaaaaaaaaaaaaaaaaaaaaaaaab.hxef/".to_owned(),
    ];
    let heads = load_heads(scene.fake.as_ref(), &access(BUCKET), &prefixes, &[], &held)
        .await
        .unwrap();
    assert!(heads.iter().all(|h| h.state == EncryptedState::Damaged));
    assert_eq!(heads[0].fid, Some([0; 16]));
    assert_eq!(heads[1].fid, None, "trailing bits make no id");
}

/// A storage that counts the `get`s in flight at once.
struct Gauge {
    inner: Arc<FakeStore>,
    now: AtomicUsize,
    most: AtomicUsize,
}

#[async_trait]
impl RemoteStore for Gauge {
    async fn get(
        &self,
        access: &Access,
        key: &str,
        max: usize,
        deadline: Instant,
    ) -> Result<Vec<u8>, StorageError> {
        let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        let body = self.inner.get(access, key, max, deadline).await;
        self.now.fetch_sub(1, Ordering::SeqCst);
        body
    }
    async fn list(
        &self,
        a: &Access,
        p: &str,
        m: usize,
        d: Instant,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        self.inner.list(a, p, m, d).await
    }
    async fn put(&self, a: &Access, k: &str, b: Vec<u8>, d: Instant) -> Result<(), StorageError> {
        self.inner.put(a, k, b, d).await
    }
    async fn delete(&self, a: &Access, k: &str, d: Instant) -> Result<(), StorageError> {
        self.inner.delete(a, k, d).await
    }
    async fn head(&self, a: &Access, k: &str, d: Instant) -> Result<ObjectHead, StorageError> {
        self.inner.head(a, k, d).await
    }
    async fn get_range(
        &self,
        a: &Access,
        k: &str,
        s: u64,
        l: u64,
        d: Instant,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, StorageError> {
        self.inner.get_range(a, k, s, l, d).await
    }
    async fn list_dir(
        &self,
        a: &Access,
        p: &str,
        m: usize,
        d: Instant,
    ) -> Result<DirListing, StorageError> {
        self.inner.list_dir(a, p, m, d).await
    }
    async fn copy(&self, a: &Access, f: &str, t: &str, d: Instant) -> Result<(), StorageError> {
        self.inner.copy(a, f, t, d).await
    }
    fn copies_on_provider(&self, a: &Access) -> bool {
        self.inner.copies_on_provider(a)
    }
    async fn rename_tree(
        &self,
        a: &Access,
        f: &str,
        t: &str,
        d: Instant,
    ) -> Result<(), StorageError> {
        self.inner.rename_tree(a, f, t, d).await
    }
    async fn remove_tree(&self, a: &Access, p: &str, d: Instant) -> Result<(), StorageError> {
        self.inner.remove_tree(a, p, d).await
    }
    async fn begin_upload(&self, a: &Access, k: &str, d: Instant) -> Result<String, StorageError> {
        self.inner.begin_upload(a, k, d).await
    }
    async fn upload_part(
        &self,
        a: &Access,
        k: &str,
        u: &str,
        n: u16,
        b: Vec<u8>,
        d: Instant,
    ) -> Result<String, StorageError> {
        self.inner.upload_part(a, k, u, n, b, d).await
    }
    async fn finish_upload(
        &self,
        a: &Access,
        k: &str,
        u: &str,
        e: &[String],
        d: Instant,
    ) -> Result<(), StorageError> {
        self.inner.finish_upload(a, k, u, e, d).await
    }
    async fn abort_upload(
        &self,
        a: &Access,
        k: &str,
        u: &str,
        d: Instant,
    ) -> Result<(), StorageError> {
        self.inner.abort_upload(a, k, u, d).await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sidecars_are_read_at_most_32_at_a_time() {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Unterlagen")
        .await
        .unwrap();
    for n in 0..80 {
        files
            .create_folder("/Unterlagen", &format!("{n}"))
            .await
            .unwrap();
    }
    let folder = opened(&files, "/Unterlagen").await;
    let gauge = Gauge {
        inner: scene.fake.clone(),
        now: AtomicUsize::new(0),
        most: AtomicUsize::new(0),
    };
    let loaded = load_folder(
        &gauge,
        &access(BUCKET),
        &folder.prefix,
        &folder.fid,
        folder.keys(),
        Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(loaded.versions.len(), 80);
    let most = gauge.most.load(Ordering::SeqCst);
    assert!(most > 1 && most <= AT_ONCE, "{most} at once");
}

#[tokio::test(flavor = "multi_thread")]
async fn foreign_files_are_counted_for_the_folder_and_never_entries() {
    use crate::files::encrypted::new_id;
    use crate::files::encrypted::test_support::{filled, tree};
    let scene = filled(&tree(&[("notiz.txt", "Notiz")])).await;
    let head = scene
        .keys()
        .into_iter()
        .find(|key| key.ends_with(".hxef/h"))
        .unwrap();
    let p = &head[..head.len() - 1];
    for key in [
        format!("{p}notiz (conflicted copy).txt"),
        format!("{p}m/kurz"),
        format!("{p}c/{}.sync-conflict", encode_id(&new_id())),
        format!("{p}fremd/x"),
    ] {
        scene.fake.insert(BUCKET, &key, b"fremd");
    }
    let files = scene.fresh_files();
    let names: Vec<String> = files
        .list("/Unterlagen")
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["notiz.txt"]);
    let folder = files.stat("/Unterlagen").await.unwrap();
    assert_eq!(folder.encrypted.unwrap().foreign, 4);
    let inside = files.stat("/Unterlagen/notiz.txt").await.unwrap();
    assert_eq!(
        inside.encrypted.unwrap().foreign,
        0,
        "only for the folder itself"
    );
    let listed = files.list("/").await.unwrap();
    assert_eq!(
        listed[0].encrypted.unwrap().foreign,
        0,
        "a listing reads only heads"
    );
}
