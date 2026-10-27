//! Conflict copy names (FR-012): extension split, dotfiles, folders, a counter when taken.

use super::*;

const NONE: &dyn Fn(&str) -> bool = &|_| false;

#[test]
fn the_name_keeps_folder_and_extension() {
    assert_eq!(
        conflict_path("Fotos/a.jpg", "Laptop", "2026-10-27", NONE),
        "Fotos/a (Konflikt Laptop 2026-10-27).jpg"
    );
    assert_eq!(
        conflict_path("archiv.tar.gz", "Laptop", "2026-10-27", NONE),
        "archiv.tar (Konflikt Laptop 2026-10-27).gz"
    );
    assert_eq!(
        conflict_path(".haex-ignore", "Laptop", "2026-10-27", NONE),
        ".haex-ignore (Konflikt Laptop 2026-10-27)"
    );
    assert_eq!(
        conflict_path("notiz", "Lap/top", "2026-10-27", NONE),
        "notiz (Konflikt Lap-top 2026-10-27)"
    );
}

#[test]
fn a_taken_name_gets_a_number() {
    let taken = |path: &str| path == "a (Konflikt L 2026-10-27).txt";
    assert_eq!(
        conflict_path("a.txt", "L", "2026-10-27", &taken),
        "a (Konflikt L 2026-10-27 2).txt"
    );
}
