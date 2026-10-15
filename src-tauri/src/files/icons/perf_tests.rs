//! SC-004 (spec 053, T061): the numbers of research R13. Run on the target machine:
//! `cargo test --release --lib files::icons::perf -- --ignored --nocapture`.

use std::time::Instant;

use super::builtin;
use super::pack::open_pack;
use super::resolve::resolve;
use super::{BuiltinSet, IconQuery, Scheme};

fn entry(index: usize) -> IconQuery {
    const NAMES: [&str; 12] = [
        "main.rs",
        "README.md",
        "package.json",
        "photo.png",
        "song.mp3",
        "movie.mp4",
        "archive.zip",
        "notes.txt",
        "Dockerfile",
        "data.csv",
        "unknown.zzz",
        "src",
    ];
    let name = NAMES[index % NAMES.len()];
    IconQuery {
        name: if index >= NAMES.len() {
            format!("{index}-{name}")
        } else {
            name.to_owned()
        },
        is_dir: name == "src",
        no_access: false,
        encrypted: None,
    }
}

#[test]
#[ignore = "a measurement, run by hand on the target machine"]
fn measure_the_icon_sets() {
    let bytes = builtin::zip_bytes(BuiltinSet::Material);
    let started = Instant::now();
    let pack = open_pack(bytes, None).expect("opens");
    let opening = started.elapsed();

    let resolve_all = |count: usize, theme| {
        let started = Instant::now();
        for index in 0..count {
            let _ = resolve(theme, Scheme::Dark, &entry(index));
        }
        started.elapsed()
    };
    let visible = resolve_all(200, Some(pack.theme()));
    let folder_material = resolve_all(1000, Some(pack.theme()));
    let folder_lucide = resolve_all(1000, None);
    println!("open the Material pack: {opening:?}");
    println!("resolve 200 visible entries (Material): {visible:?}");
    println!("resolve 1000 entries: Material {folder_material:?}, Lucide {folder_lucide:?}");
    println!(
        "difference for 1000 entries: {:?}",
        folder_material.saturating_sub(folder_lucide)
    );
}
