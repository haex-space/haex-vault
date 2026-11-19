//! Load test for spec 024 User Story 3 (T048, SC-003, SC-004, SC-014): three
//! devices write over a thousand changes while they exchange them over
//! changing, partly indirect ways with transfers that break off at random
//! places. After every exchange nobody's progress may lie above a change of
//! the same origin it lacks; no transaction group is ever applied in part; and
//! in the end all three hold the same data, every change naming the device
//! that wrote it.
//!
//! The exchanges are the real pull (`sync::outbound`, `sync::inbound`) driven
//! directly, as `sync_devices.rs`'s fixture does, so a transfer can be cut
//! after any page and the page size can vary; sessions, relays and sockets are
//! not under load here. Slow and random, so it is `#[ignore]`d and runs on
//! request:
//!
//! `cargo test --manifest-path src-tauri/Cargo.toml --test sync_three_devices -- --ignored --nocapture`
//!
//! The seed is printed; `HAEX_LOAD_SEED=<n>` replays a run.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use haex_crdt::rusqlite::params;
use haex_crdt::{compare_hlc_strings, Database};
use uuid::Uuid;

use haex_lib::identity::installation_id_path;
use haex_lib::instances::vault_config::vault_config;
use haex_lib::storage::query::{self, Query};
use haex_lib::sync::change::Page;
use haex_lib::sync::inbound::Inbox;
use haex_lib::sync::outbound::serve_pull_with_budget;
use haex_lib::sync::progress;
use haex_lib::sync::replica::Replica;

/// Writes of the run; each insert batch and each title update counts as one
/// change here, and one batch holds several rows, so the cells are far more.
const CHANGES: usize = 1_000;
const NAMES: [&str; 3] = ["A", "B", "C"];

/// A tiny deterministic generator (xorshift64*), so a failing run can be
/// replayed from its seed.
struct Rng(u64);

impl Rng {
    /// Advances the xorshift64* state and returns the next deterministic value.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Returns the next value reduced to `0..n`; `n` must be nonzero.
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).expect("fits")
    }
}

struct Dev {
    _dir: tempfile::TempDir,
    replica: Replica,
    /// The origin of what this device writes.
    origin: Uuid,
}

impl Dev {
    /// Creates a replica in a temporary vault with its own installation and origin.
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = vault_config(
            "sync-load-passphrase",
            &dir.path().join("vault.db"),
            &installation_id_path(dir.path()),
            true,
        );
        let db = Arc::new(Database::open(config).expect("open vault"));
        let origin = db.device_id();
        Self {
            _dir: dir,
            replica: Replica::new(db),
            origin,
        }
    }

    /// Borrows the replica's database for test reads and writes.
    fn db(&self) -> &Database {
        self.replica.db()
    }
}

/// What the writer knows about a row it wrote.
struct RowModel {
    writer: usize,
    insert_hlc: String,
    batch: String,
    title: String,
    title_hlc: String,
}

/// What every device should hold, kept by the writers.
#[derive(Default)]
struct Model {
    rows: BTreeMap<String, RowModel>,
    /// Batch key → how many rows the batch inserted in one transaction.
    batches: BTreeMap<String, usize>,
    changes: usize,
}

/// Reads the row HLC of an existing chat thread on its writing device.
fn hlc_of(dev: &Dev, id: &str) -> String {
    query::read(dev.db(), |r| {
        r.query_row(
            "SELECT haex_hlc_no_sync FROM chat_threads WHERE id = ?1",
            params![id],
            |row| row.get::<_, String>(0),
        )
    })
    .expect("read hlc")
    .expect("the row exists on its writer")
}

/// One transaction that inserts `size` rows: a group that arrives whole or not at all.
fn write_batch(model: &mut Model, devs: &[Dev], writer: usize, seq: usize, size: usize) {
    let batch = format!("{}{seq}", NAMES[writer]);
    let ids: Vec<String> = (0..size).map(|k| format!("{batch}.{k}")).collect();
    devs[writer]
        .db()
        .write(|tx| {
            for id in &ids {
                tx.execute(
                    "INSERT INTO chat_threads (id, title, created_at, updated_at) \
                     VALUES (?1, ?2, 1, 1)",
                    params![id, format!("{id} first")],
                )?;
            }
            Ok(())
        })
        .expect("write a batch");
    let hlc = hlc_of(&devs[writer], &ids[0]);
    model.batches.insert(batch.clone(), size);
    for id in ids {
        model.rows.insert(
            id.clone(),
            RowModel {
                writer,
                insert_hlc: hlc.clone(),
                batch: batch.clone(),
                title: format!("{id} first"),
                title_hlc: hlc.clone(),
            },
        );
    }
    model.changes += 1;
}

/// A title update of a row its writer wrote earlier.
fn rewrite_title(model: &mut Model, devs: &[Dev], rng: &mut Rng, writer: usize, n: usize) {
    let own: Vec<String> = model
        .rows
        .iter()
        .filter(|(_, row)| row.writer == writer)
        .map(|(id, _)| id.clone())
        .collect();
    if own.is_empty() {
        return;
    }
    let id = own[rng.below(own.len())].clone();
    let title = format!("{id} v{n}");
    devs[writer]
        .db()
        .write(|tx| {
            tx.execute(
                "UPDATE chat_threads SET title = ?2 WHERE id = ?1",
                params![id, title],
            )
        })
        .expect("update a title");
    let row = model.rows.get_mut(&id).expect("modelled");
    row.title = title;
    row.title_hlc = hlc_of(&devs[writer], &id);
    model.changes += 1;
}

/// What `to` pulls from `from`: pages of about `budget` bytes, cut off after
/// `cut_after` pages when given. A cut transfer is simply dropped, as a lost
/// connection drops it; the next one starts again from `to`'s progress.
/// Returns the pages applied.
fn exchange(from: &Dev, to: &Dev, budget: usize, cut_after: Option<usize>) -> usize {
    let theirs = to.replica.progress().expect("progress");
    let mut outbox = serve_pull_with_budget(&from.replica, &theirs, budget).expect("serve");
    let mut inbox = Inbox::new();
    let mut applied = 0;
    while let Some(page) = outbox.next_page() {
        if cut_after == Some(applied) {
            break;
        }
        inbox.receive(&to.replica, page).expect("receive a page");
        applied += 1;
    }
    applied
}

/// Every thread a device holds: id → (title, row HLC).
fn held(dev: &Dev) -> BTreeMap<String, (String, String)> {
    query::read(dev.db(), |r| {
        Ok(r.query_map(
            "SELECT id, title, haex_hlc_no_sync FROM chat_threads",
            &[],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?
        .into_iter()
        .map(|(id, title, hlc)| (id, (title, hlc)))
        .collect())
    })
    .expect("read threads")
}

/// SC-014 and the rules around it, checked on every device.
fn check(model: &Model, devs: &[Dev], context: &str) {
    for (i, dev) in devs.iter().enumerate() {
        let rows = held(dev);
        let progress = dev.replica.progress().expect("progress");
        // Nothing the writers never wrote, and each row names its writer.
        for (id, (_, hlc)) in &rows {
            let row = model.rows.get(id).unwrap_or_else(|| {
                panic!("{context}: {} holds {id}, which nobody wrote", NAMES[i])
            });
            assert_eq!(
                progress::origin_of(hlc),
                Some(devs[row.writer].origin),
                "{context}: {id} on {} does not name its writer {}",
                NAMES[i],
                NAMES[row.writer]
            );
        }
        for (id, row) in &model.rows {
            let origin = devs[row.writer].origin;
            let covered = |hlc: &str| {
                progress
                    .get(&origin)
                    .is_some_and(|p| compare_hlc_strings(hlc, p) != Ordering::Greater)
            };
            // Progress never above a change that is missing (SC-014).
            if covered(&row.insert_hlc) {
                let (title, _) = rows.get(id).unwrap_or_else(|| {
                    panic!(
                        "{context}: {}'s progress for {} covers {id}, but {id} is missing",
                        NAMES[i], NAMES[row.writer]
                    )
                });
                if covered(&row.title_hlc) {
                    assert_eq!(
                        title, &row.title,
                        "{context}: {}'s progress covers the last title of {id}, which it lacks",
                        NAMES[i]
                    );
                }
            }
        }
        // No group applied in part: a batch is there whole or not at all.
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for id in rows.keys() {
            *seen.entry(model.rows[id].batch.as_str()).or_default() += 1;
        }
        for (batch, count) in seen {
            assert_eq!(
                count, model.batches[batch],
                "{context}: {} holds {count} of the {} rows of batch {batch}",
                NAMES[i], model.batches[batch]
            );
        }
    }
}

/// Reads `HAEX_LOAD_SEED` as a `u64`, defaulting to `0xC0FFEE` if absent or invalid.
fn seed() -> u64 {
    std::env::var("HAEX_LOAD_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0xC0FFEE)
}

/// Checks convergence, progress and batch integrity under interrupted transfers,
/// then verifies that reconnecting sends only changes the receiver lacks.
#[test]
#[ignore = "slow and random: run on request, see the module docs"]
fn three_devices_converge_over_changing_ways_with_cut_transfers() {
    let seed = seed();
    println!("sync_three_devices: seed {seed} (HAEX_LOAD_SEED replays it)");
    let mut rng = Rng(seed | 1);
    let devs = [Dev::new(), Dev::new(), Dev::new()];
    let mut model = Model::default();
    let (mut seq, mut exchanges, mut cuts) = (0usize, 0usize, 0usize);

    while model.changes < CHANGES {
        // A device writes a few changes ...
        let writer = rng.below(3);
        for _ in 0..1 + rng.below(4) {
            if rng.below(3) == 0 {
                rewrite_title(&mut model, &devs, &mut rng, writer, seq);
            } else {
                write_batch(&mut model, &devs, writer, seq, 1 + rng.below(4));
            }
            seq += 1;
        }
        // ... and some devices pull from others, mostly A-B and B-C, so that
        // A's changes reach C only through B, now and then A-C directly. A
        // transfer breaks off at a random place about every third time.
        for _ in 0..1 + rng.below(3) {
            let (from, to) = match rng.below(10) {
                0 => (0, 2),
                1 => (2, 0),
                2..=4 => [(0, 1), (1, 0)][rng.below(2)],
                _ => [(1, 2), (2, 1)][rng.below(2)],
            };
            let budget = 300 + rng.below(3_700);
            let cut = (rng.below(3) == 0).then(|| rng.below(6));
            let pages = exchange(&devs[from], &devs[to], budget, cut);
            exchanges += 1;
            cuts += usize::from(cut.is_some_and(|c| c <= pages));
            check(
                &model,
                &devs,
                &format!(
                    "after {} <- {} (budget {budget}, cut {cut:?})",
                    NAMES[to], NAMES[from]
                ),
            );
        }
    }
    println!(
        "sync_three_devices: {} changes, {} rows, {exchanges} exchanges, {cuts} of them cut",
        model.changes,
        model.rows.len()
    );

    // Everybody pulls from everybody, whole, until nothing is left to send.
    for _ in 0..3 {
        for from in 0..3 {
            for to in 0..3 {
                if from != to {
                    exchange(&devs[from], &devs[to], 64 * 1024, None);
                }
            }
        }
    }
    check(&model, &devs, "after the final exchanges");

    // SC-003: the same state on all three, no change missing or doubled, every
    // change naming its true origin.
    let expected: BTreeMap<String, (String, String)> = model
        .rows
        .iter()
        .map(|(id, row)| (id.clone(), (row.title.clone(), row.title_hlc.clone())))
        .collect();
    for (i, dev) in devs.iter().enumerate() {
        assert_eq!(
            held(dev),
            expected,
            "{} does not hold the final state",
            NAMES[i]
        );
    }
    let progresses: Vec<_> = devs
        .iter()
        .map(|d| d.replica.progress().expect("progress"))
        .collect();
    assert!(
        progresses.windows(2).all(|w| w[0] == w[1]),
        "the three devices do not end with the same progress: {progresses:?}"
    );
    let origins: BTreeSet<_> = devs.iter().map(|d| d.origin).collect();
    assert_eq!(
        progresses[0].keys().copied().collect::<BTreeSet<_>>(),
        origins,
        "progress names exactly the three writers"
    );

    // SC-004: after a break in which B wrote N changes, reconnecting sends
    // those N and nothing A already has, and a second reconnect sends nothing.
    const N: usize = 50;
    for k in 0..N {
        write_batch(&mut model, &devs, 1, seq + k, 1);
    }
    let theirs = devs[0].replica.progress().expect("progress");
    let mut outbox = serve_pull_with_budget(&devs[1].replica, &theirs, 4 * 1024).expect("serve");
    let mut groups: BTreeSet<String> = BTreeSet::new();
    let mut pages: Vec<Page> = Vec::new();
    while let Some(page) = outbox.next_page() {
        for change in &page.changes {
            groups.insert(change.hlc.clone());
            let origin = progress::origin_of(&change.hlc).expect("an origin");
            assert!(
                theirs
                    .get(&origin)
                    .is_none_or(|have| compare_hlc_strings(&change.hlc, have) == Ordering::Greater),
                "the reconnect resends a change A already has"
            );
        }
        pages.push(page);
    }
    assert_eq!(
        groups.len(),
        N,
        "the reconnect sends exactly the {N} new changes"
    );
    let mut inbox = Inbox::new();
    for page in pages {
        inbox.receive(&devs[0].replica, page).expect("receive");
    }
    let again = serve_pull_with_budget(
        &devs[1].replica,
        &devs[0].replica.progress().expect("progress"),
        4 * 1024,
    )
    .expect("serve");
    assert!(again.is_empty(), "a second reconnect sends nothing");
}

/// The smallest case the load test found: a row whose title was changed after
/// it was created, before another device ever pulled it. The pull has to apply
/// the row whole (it must not fail on a missing `title`).
#[test]
fn a_row_changed_after_its_creation_reaches_a_fresh_device_whole() {
    let (a, b) = (Dev::new(), Dev::new());
    let mut model = Model::default();
    let devs = [a, b];
    write_batch(&mut model, &devs, 0, 0, 1);
    let mut rng = Rng(7);
    rewrite_title(&mut model, &devs, &mut rng, 0, 1);

    for budget in [64 * 1024, 150] {
        let b = Dev::new();
        exchange(&devs[0], &b, budget, None);

        let expected: BTreeMap<String, (String, String)> = model
            .rows
            .iter()
            .map(|(id, row)| (id.clone(), (row.title.clone(), row.title_hlc.clone())))
            .collect();
        assert_eq!(held(&b), expected, "with pages of {budget} bytes");
    }
}

/// A pull that breaks off after the first page, which holds the group of a
/// row that cannot be created without a later one: nothing of that row is
/// kept and progress stays below it, so the next pull sends it again whole.
#[test]
fn a_cut_pull_holds_back_progress_below_a_row_it_could_not_create() {
    let devs = [Dev::new(), Dev::new()];
    let mut model = Model::default();
    write_batch(&mut model, &devs, 0, 0, 1);
    let mut rng = Rng(7);
    rewrite_title(&mut model, &devs, &mut rng, 0, 1);
    let first_group = model.rows["A0.0"].insert_hlc.clone();

    // The first page holds the group of the creation, the rest follows.
    let pages = exchange(&devs[0], &devs[1], 150, Some(1));

    assert_eq!(pages, 1);
    assert!(held(&devs[1]).is_empty(), "the row was not created in part");
    let progress = devs[1].replica.progress().expect("progress");
    assert!(
        progress
            .get(&devs[0].origin)
            .is_none_or(|p| compare_hlc_strings(p, &first_group) == Ordering::Less),
        "progress covers a group that was held back"
    );
    exchange(&devs[0], &devs[1], 150, None);
    assert_eq!(held(&devs[1])["A0.0"].0, model.rows["A0.0"].title);
}
